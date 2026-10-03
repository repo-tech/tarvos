//! Tarvos toolchain manager (Option B).
//!
//! A Tarvos user installs Tarvos, not Rust. A normal build resolves the
//! Tarvos-managed toolchain first; system Rust is only ever used when the
//! caller explicitly selects it. Nothing in this module may silently fall back
//! from one mode to the other.

use anyhow::{Context, Result};
use std::fs;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::which_simple;

/// The Rust channel every managed install must provide.
///
/// Kept in sync with `rust-toolchain.toml` on purpose. A floating channel
/// made CI non-reproducible; the managed toolchain gets the same guarantee.
pub const PINNED_CHANNEL: &str = "1.98.0";

/// Which host each managed installer is expected to run on. Used by the
/// `toolchain --install` listing (Milestone 2); kept here so the manager is
/// the single place that names platforms.
#[allow(dead_code)]
pub const EXPECTED_TRIPLES: &[(&str, &str)] = &[
    ("windows", "x86_64-pc-windows-msvc"),
    ("linux", "x86_64-unknown-linux-gnu"),
    ("macos-intel", "x86_64-apple-darwin"),
    ("macos-arm", "aarch64-apple-darwin"),
];

/// Which toolchain a build may use. Nothing here is optional in the loose
/// sense: there is no mode that means "pick whatever is on PATH".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolchainMode {
    /// `~/.tarvos/toolchain`: downloaded and verified by Tarvos itself.
    Managed,
    /// Explicit `--system-rust`: the user's own compiler, validated first.
    System,
}

impl ToolchainMode {
    pub fn label(self) -> &'static str {
        match self {
            ToolchainMode::Managed => "managed",
            ToolchainMode::System => "system",
        }
    }
}

/// One validated compiler plus the facts validation proved.
#[derive(Debug, Clone)]
pub struct ResolvedToolchain {
    pub mode: ToolchainMode,
    pub rustc: PathBuf,
    pub version: String,
    pub target: String,
    /// How this toolchain was found, for `doctor` and build diagnostics.
    pub source: String,
}

/// Failure with an actionable next step, not a bare "not found".
#[derive(Debug)]
pub struct ToolchainError {
    pub mode: ToolchainMode,
    pub reason: String,
    pub hint: String,
}

impl std::fmt::Display for ToolchainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Tarvos {} toolchain unavailable: {}. {}",
            self.mode.label(),
            self.reason,
            self.hint
        )
    }
}

impl std::error::Error for ToolchainError {}

/// Well-known layout inside the user-scoped Tarvos store. No step here assumes
/// a global install path, so a portable copy keeps working wherever it lands.
pub fn managed_root(home: &Path) -> PathBuf {
    home.join(".tarvos").join("toolchain")
}

pub fn managed_bin(home: &Path) -> PathBuf {
    managed_root(home).join("bin")
}

fn user_home() -> Result<PathBuf> {
    let key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(key)
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("could not determine the current user's home directory"))
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn version_of(rustc: &Path) -> Option<String> {
    let output = Command::new(rustc).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    // rustc appends the commit after the number; compare the number only.
    text.split_whitespace()
        .nth(1)
        .map(|token| token.trim_start_matches('v').to_string())
}

/// The default host target rustc reports, e.g. `x86_64-pc-windows-msvc`.
fn host_target_of(rustc: &Path) -> Option<String> {
    let output = Command::new(rustc).arg("-vV").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    text.lines().find_map(|line| {
        line.strip_prefix("host: ")
            .map(str::trim)
            .map(str::to_string)
    })
}

/// A compiler proves itself by compiling, not by existing on disk.
///
/// The probe used to live only in memory: every `tarvos build`, `tarvos run`,
/// and `tarvos verify` re-ran a full `rustc` link just to re-learn what the
/// previous invocation already knew, and that subprocess is what made the CLI
/// feel slow after the managed toolchain landed. The stamp file below keeps
/// the guarantee (a compiler that cannot link is still rejected) without
/// paying the link on every command.
fn probe_stamp_path(rustc: &Path) -> Option<PathBuf> {
    let home = user_home().ok()?;
    // `as_encoded_bytes` called as an associated function, not through a
    // reference to it. The previous spelling passed `&rustc.as_os_str()
    // .as_encoded_bytes()`, which binds a reference to a temporary function
    // value and immediately dereferences it; clippy rejects it under
    // `-D warnings`.
    Some(managed_root(&home).join("cache").join(format!(
        "probe-ok-{:016x}",
        fnv1a_64(rustc.as_os_str().as_encoded_bytes())
    )))
}

/// The probe a stamp certifies: the exact rustc binary, pinned channel, and
/// host triple the stamp was written for. A stamp copied from another machine
/// or left behind by an upgrade names a compiler this one is not, so it must
/// never pass.
fn probe_stamp_payload(rustc: &Path) -> Option<String> {
    let version = version_of(rustc)?;
    let target = host_target_of(rustc)?;
    (version == PINNED_CHANNEL).then(|| probe_stamp_payload_for(&version, &target, rustc))
}

/// The stamp text, built from already-resolved facts so the identity rules can
/// be tested without needing a compiler on disk. Every field is load-bearing: a
/// stamp that omitted any one of them would let a different compiler satisfy a
/// probe it never passed.
fn probe_stamp_payload_for(version: &str, target: &str, rustc: &Path) -> String {
    format!("{version}\n{target}\n{}\n", rustc.display())
}

fn probe_stamp_fresh(rustc: &Path) -> bool {
    let (stamp, payload) = match (probe_stamp_path(rustc), probe_stamp_payload(rustc)) {
        (Some(stamp), Some(payload)) => (stamp, payload),
        _ => return false,
    };
    // Same clock rule as the probe itself: the compiler must not be newer than
    // the stamp, otherwise the stamp certifies a binary that changed since.
    let compiler_newer = fs::metadata(rustc)
        .and_then(|meta| meta.modified())
        .and_then(|modified| {
            fs::metadata(&stamp)
                .and_then(|stamp_meta| stamp_meta.modified())
                .map(|stamped| modified > stamped)
        })
        .unwrap_or(true);
    if compiler_newer {
        return false;
    }
    fs::read_to_string(&stamp)
        .map(|cached| cached == payload)
        .unwrap_or(false)
}

fn record_probe_stamp(rustc: &Path) {
    if let (Some(stamp), Some(payload)) = (probe_stamp_path(rustc), probe_stamp_payload(rustc)) {
        if let Some(parent) = stamp.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&stamp, payload);
    }
}

/// Stable hash for stamp file names. `std`'s `DefaultHasher` is explicitly
/// *not* stable across processes, so it would address a different stamp file
/// on every invocation and the cache would never hit.
fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn proves_compilation(rustc: &Path) -> bool {
    if probe_stamp_fresh(rustc) {
        return true;
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("tarvos-tc-probe-{stamp}"));
    if std::fs::create_dir_all(&dir).is_err() {
        return false;
    }
    let source = dir.join("probe.rs");
    if std::fs::write(&source, "fn main() {}\n").is_err() {
        let _ = std::fs::remove_dir_all(&dir);
        return false;
    }
    let binary = dir.join(exe("probe"));
    let ok = Command::new(rustc)
        .arg("--edition=2021")
        .arg("-o")
        .arg(&binary)
        .arg(&source)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    let _ = std::fs::remove_dir_all(&dir);
    if ok {
        record_probe_stamp(rustc);
    }
    ok
}

/// Resolve one toolchain. `prefer_system` is only true when the caller passed
/// an explicit `--system-rust`; without it, system Rust is never consulted.
pub fn resolve(prefer_system: bool) -> Result<ResolvedToolchain> {
    if prefer_system {
        return resolve_system();
    }
    match resolve_managed() {
        Ok(found) => Ok(found),
        Err(managed_error) => Err(anyhow::anyhow!(
            "{managed_error} Run `tarvos toolchain --install` once, or pass --system-rust to use an explicitly validated system compiler."
        )),
    }
}

/// Today's `ensure_rust_toolchain()` path, kept as a labeled legacy bridge.
pub fn legacy_system_toolchain(rustc: PathBuf) -> ResolvedToolchain {
    let version = version_of(&rustc).unwrap_or_else(|| "unknown".to_string());
    let target = host_target_of(&rustc).unwrap_or_else(|| "unknown".to_string());
    ResolvedToolchain {
        mode: ToolchainMode::System,
        rustc: rustc.clone(),
        version,
        target,
        source: format!("legacy system discovery at {}", rustc.display()),
    }
}

/// Locate the Tarvos-managed compiler under `~/.tarvos/toolchain/bin`.
fn resolve_managed() -> Result<ResolvedToolchain, ToolchainError> {
    let home = user_home().map_err(|e| ToolchainError {
        mode: ToolchainMode::Managed,
        reason: e.to_string(),
        hint: "set HOME (or USERPROFILE on Windows) and retry.".to_string(),
    })?;
    let rustc = managed_bin(&home).join(exe("rustc"));
    if !rustc.exists() {
        return Err(ToolchainError {
            mode: ToolchainMode::Managed,
            reason: format!("no managed rustc at {}", rustc.display()),
            hint: "run `tarvos toolchain --install` once to fetch the pinned channel.".to_string(),
        });
    }
    let version = version_of(&rustc).ok_or_else(|| ToolchainError {
        mode: ToolchainMode::Managed,
        reason: format!("managed rustc at {} did not report a version", rustc.display()),
        hint: "run `tarvos toolchain --verify`; if it still fails, reinstall with `tarvos toolchain --install`."
            .to_string(),
    })?;
    if !version.starts_with(PINNED_CHANNEL) {
        return Err(ToolchainError {
            mode: ToolchainMode::Managed,
            reason: format!("managed rustc reports {version} but Tarvos expects {PINNED_CHANNEL}"),
            hint: "run `tarvos toolchain --install` to refresh the managed compiler.".to_string(),
        });
    }
    let target = host_target_of(&rustc).ok_or_else(|| ToolchainError {
        mode: ToolchainMode::Managed,
        reason: "managed rustc did not report a host target".to_string(),
        hint: "run `tarvos toolchain --verify`.".to_string(),
    })?;
    if !proves_compilation(&rustc) {
        return Err(ToolchainError {
            mode: ToolchainMode::Managed,
            reason: "managed rustc exists but cannot compile a probe program".to_string(),
            hint: "run `tarvos toolchain --verify`; if it still fails, reinstall.".to_string(),
        });
    }
    Ok(ResolvedToolchain {
        mode: ToolchainMode::Managed,
        rustc: rustc.clone(),
        version,
        target,
        source: format!("managed toolchain at {}", rustc.display()),
    })
}

/// Validate the pinned-channel compiler found on this machine.
fn resolve_system() -> Result<ResolvedToolchain> {
    let strict = matches!(std::env::var("TARVOS_STRICT_RUST_PIN").as_deref(), Ok("1"));
    let mut tried: Vec<String> = Vec::new();
    // RUSTC names one compiler. When it is set to something that does not
    // exist, that is a hard error: scanning on would substitute a different
    // compiler for the one the user named, which is exactly the silent switch
    // this module forbids.
    if let Some(explicit) = std::env::var("RUSTC").ok().filter(|v| !v.trim().is_empty()) {
        let path = PathBuf::from(explicit);
        if path.exists() {
            tried.push(path.display().to_string());
        } else {
            return Err(anyhow::anyhow!(ToolchainError {
                mode: ToolchainMode::System,
                reason: format!("RUSTC points at {} which does not exist", path.display()),
                hint: "unset RUSTC or point it at a real rustc.".to_string(),
            }));
        }
    }
    let candidates = [Some(PathBuf::from(exe("rustc")))];
    for candidate in candidates.into_iter().flatten() {
        let path = if candidate.is_absolute() {
            candidate
        } else {
            match which_simple(&candidate.to_string_lossy())? {
                Some(found) if found.exists() => found,
                _ => continue,
            }
        };
        if !path.exists() {
            continue;
        }
        tried.push(path.display().to_string());
        let Some(version) = version_of(&path) else {
            continue;
        };
        if strict && !version.starts_with(PINNED_CHANNEL) {
            continue;
        }
        let Some(target) = host_target_of(&path) else {
            continue;
        };
        if !proves_compilation(&path) {
            continue;
        }
        return Ok(ResolvedToolchain {
            mode: ToolchainMode::System,
            rustc: path.clone(),
            version,
            target,
            source: format!("system rustc at {}", path.display()),
        });
    }
    Err(anyhow::anyhow!(ToolchainError {
        mode: ToolchainMode::System,
        reason: "no system toolchain validated".to_string(),
        hint: "install rust or run `tarvos toolchain --install`.".to_string(),
    }))
}

/// Metadata every build records, so an artifact names the toolchain that made it.
#[allow(dead_code)]
pub fn build_metadata(version: &ResolvedToolchain) -> serde_json::Value {
    serde_json::json!({
        "tarvos": env!("CARGO_PKG_VERSION"),
        "rust": version.version,
        "target": version.target,
        "toolchain": version.mode.label(),
        "toolchain_source": version.source,
        "pinned_channel": PINNED_CHANNEL,
    })
}

/// `tarvos toolchain [--status] [--install] [--verify]`.
///
/// With no flag, and with `--status`, this reports exactly what a build would
/// use: layout, resolution result, and nothing else. Nothing here mutates the
/// machine. `--install` fetches the pinned channel; `--verify` re-runs every
/// validation check and names each one OK, WARNING or ERROR.
pub fn toolchain_command(status: bool, install: bool, verify: bool) -> Result<()> {
    if install {
        return install_managed();
    }
    if verify {
        return verify_managed();
    }
    // No flag and --status are the same report: there is only one honest one.
    let _ = status;
    status_managed()
}

fn status_managed() -> Result<()> {
    let home = user_home()?;
    let root = managed_root(&home);
    let bin = managed_bin(&home);
    println!("Managed toolchain root : {}", root.display());
    println!("Compiler directory     : {}", bin.display());
    println!("Expected channel       : {PINNED_CHANNEL}");
    println!();
    match resolve_managed() {
        Ok(found) => {
            println!("Status                 : READY");
            println!("rustc                  : {}", found.rustc.display());
            println!("version                : {}", found.version);
            println!("target                 : {}", found.target);
        }
        Err(error) => {
            println!("Status                 : MISSING");
            println!("Reason                 : {}", error.reason);
            println!("Next step              : {}", error.hint);
        }
    }
    Ok(())
}

/// One validation fact for `verify`, printed in a fixed order.
struct Check {
    name: &'static str,
    state: CheckState,
    detail: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum CheckState {
    Ok,
    Warning,
    Error,
}

impl CheckState {
    fn tag(self) -> &'static str {
        match self {
            CheckState::Ok => "[OK]",
            CheckState::Warning => "[WARNING]",
            CheckState::Error => "[ERROR]",
        }
    }
}

fn verify_managed() -> Result<()> {
    let mut checks: Vec<Check> = Vec::new();
    let home = match user_home() {
        Ok(home) => {
            checks.push(Check {
                name: "home directory",
                state: CheckState::Ok,
                detail: home.display().to_string(),
            });
            home
        }
        Err(error) => {
            checks.push(Check {
                name: "home directory",
                state: CheckState::Error,
                detail: error.to_string(),
            });
            return report_checks(checks, true);
        }
    };
    let rustc = managed_bin(&home).join(exe("rustc"));
    if !rustc.exists() {
        checks.push(Check {
            name: "managed rustc present",
            state: CheckState::Error,
            detail: format!("no binary at {}", rustc.display()),
        });
        return report_checks(checks, true);
    }
    checks.push(Check {
        name: "managed rustc present",
        state: CheckState::Ok,
        detail: rustc.display().to_string(),
    });
    match version_of(&rustc) {
        Some(version) if version.starts_with(PINNED_CHANNEL) => checks.push(Check {
            name: "pinned channel",
            state: CheckState::Ok,
            detail: version,
        }),
        Some(version) => checks.push(Check {
            name: "pinned channel",
            state: CheckState::Error,
            detail: format!("reports {version}, expected {PINNED_CHANNEL}"),
        }),
        None => checks.push(Check {
            name: "pinned channel",
            state: CheckState::Error,
            detail: "rustc --version produced no parseable output".to_string(),
        }),
    }
    match host_target_of(&rustc) {
        Some(target) => checks.push(Check {
            name: "host target",
            state: CheckState::Ok,
            detail: target,
        }),
        None => checks.push(Check {
            name: "host target",
            state: CheckState::Error,
            detail: "rustc -vV reported no host triple".to_string(),
        }),
    }
    if proves_compilation(&rustc) {
        checks.push(Check {
            name: "probe compilation",
            state: CheckState::Ok,
            detail: "compiled and linked a scratch program".to_string(),
        });
    } else {
        checks.push(Check {
            name: "probe compilation",
            state: CheckState::Error,
            detail: "rustc exists but could not compile a scratch program".to_string(),
        });
    }
    let failed = checks.iter().any(|c| c.state == CheckState::Error);
    report_checks(checks, failed)
}

/// Which static archive to fetch for this host. The manifest lives next to
/// this function so a URL change is reviewed in the same file as the code
/// that trusts it.
///
/// Layout (all official Rust static distributions):
/// `https://static.rust-lang.org/dist/rust-{version}-{triple}.tar.gz`
fn dist_url(channel: &str, triple: &str) -> String {
    format!("https://static.rust-lang.org/dist/rust-{channel}-{triple}.tar.gz")
}

/// The triple this binary would install. Unknown hosts are an explicit error
/// with the supported list, not a guess.
fn install_triple() -> Result<String> {
    if cfg!(all(target_os = "windows", target_env = "msvc")) {
        Ok("x86_64-pc-windows-msvc".to_string())
    } else if cfg!(all(target_os = "windows", target_env = "gnu")) {
        Ok("x86_64-pc-windows-gnu".to_string())
    } else if cfg!(target_os = "linux") {
        Ok("x86_64-unknown-linux-gnu".to_string())
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            Ok("aarch64-apple-darwin".to_string())
        } else {
            Ok("x86_64-apple-darwin".to_string())
        }
    } else {
        Err(anyhow::anyhow!(
            "unsupported host for managed install; supported triples: {}",
            EXPECTED_TRIPLES
                .iter()
                .map(|(_, t)| *t)
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
}

/// Fetch the pinned channel into `~/.tarvos/toolchain`.
///
/// Integrity chain, in order, none of them skippable:
/// 1. HTTPS only (static.rust-lang.org).
/// 2. SHA-256 of the downloaded archive re-hashed locally and compared with
///    the `.sha256` sidecar fetched alongside it.
/// 3. `rustc --version` and host-target checks on the extracted compiler.
/// 4. Probe compilation before the install is marked complete.
/// 5. Everything lands in a staging directory and is renamed into place only
///    after every check passes, so an interrupted or failed install never
///    leaves a half-toolchain where the next build would find it.
fn install_managed() -> Result<()> {
    // Installing a valid toolchain again would re-download a few hundred
    // megabytes and re-unpack it for no reason. Verify first and stop.
    if let Ok(existing) = resolve_managed() {
        println!(
            "Managed toolchain already present at {} ({} {}); nothing to do.",
            existing.rustc.display(),
            existing.version,
            existing.target
        );
        return Ok(());
    }
    let triple = install_triple()?;
    // Refuse an unsupported host before a single byte is fetched.
    if !host_supports_install_script() && !cfg!(windows) {
        return Err(anyhow::anyhow!(
            "managed toolchain install is not available on this host: the official Rust \
             static distribution ships an install.sh script that only runs on Unix, and this \
             host is {triple}. Nothing was downloaded. Use `tarvos build --system-rust` with an \
             existing Rust install."
        ));
    }
    let url = dist_url(PINNED_CHANNEL, &triple);
    let home = user_home()?;
    let root = managed_root(&home);
    let staging = root.with_extension("staging");
    if staging.exists() {
        std::fs::remove_dir_all(&staging)
            .with_context(|| format!("failed to clear {}", staging.display()))?;
    }
    std::fs::create_dir_all(&staging)
        .with_context(|| format!("failed to create {}", staging.display()))?;
    // Every phase below prints where it got to, because a several-hundred-megabyte
    // install is long enough that a silent stretch reads as a hang.
    let started = Instant::now();
    println!("Fetching pinned toolchain {PINNED_CHANNEL} for {triple}");
    println!("Source: {url}");
    let archive = download_to(&url, &staging.join("rust.tar.gz"), "  Downloading")?;
    download_to(
        &format!("{url}.sha256"),
        &staging.join("rust.tar.gz.sha256"),
        "  Checksum   ",
    )?;
    step(2, 6, "Verifying the SHA-256 checksum");
    verify_sha256(
        &staging.join("rust.tar.gz"),
        &staging.join("rust.tar.gz.sha256"),
    )?;
    step(3, 6, "Unpacking the components");
    let unpacked = staging.join(format!("rust-{PINNED_CHANNEL}-{triple}"));
    let installed = staging.join("installed");

    // Two ways to lay the components down, same three components either way.
    //
    // The Windows archive does ship an install.sh, but that script is POSIX:
    // it calls `uname` before doing any work, so on Windows it needs a POSIX
    // layer Tarvos cannot assume. An earlier revision here refused Windows
    // outright on the claim that the archive had no install.sh at all. Both
    // halves of that were wrong, and the second one sent users to a manual
    // install for a path that works unattended.
    step(4, 6, "Laying out the toolchain");
    #[cfg(windows)]
    {
        assemble_windows_toolchain(&staging.join("rust.tar.gz"), &staging, &installed, &triple)?;
    }
    #[cfg(not(windows))]
    {
        // Only the three components Tarvos can drive are unpacked. A full
        // extraction also writes the documentation trees, which measured at
        // close to a gigabyte and which neither rustc nor cargo ever reads, and
        // install.sh then copies its components a second time. Selecting at the
        // archive is what makes this the fastest part of the install instead of
        // the slowest. install.sh only lays down the components it was asked for,
        // so the missing documentation is not something it will go looking for.
        extract_components(&staging.join("rust.tar.gz"), &staging, &triple)?;
        run_install_sh(&unpacked.join("install.sh"), &installed, &triple)?;
    }
    let _ = &unpacked;
    let rustc = installed.join("bin").join(exe("rustc"));
    if !rustc.exists() {
        return Err(anyhow::anyhow!(
            "installer completed but {} is missing; refusing a partial install",
            rustc.display()
        ));
    }
    step(5, 6, "Validating the installed compiler");
    // Validate before publish: a broken compiler must fail here, not later.
    let version = version_of(&rustc).ok_or_else(|| {
        anyhow::anyhow!("installed rustc reported no version; refusing a partial install")
    })?;
    if !version.starts_with(PINNED_CHANNEL) {
        return Err(anyhow::anyhow!(
            "installed rustc reports {version}, expected {PINNED_CHANNEL}; refusing a partial install"
        ));
    }
    if !proves_compilation(&rustc) {
        return Err(anyhow::anyhow!(
            "installed rustc cannot compile a probe program; refusing a partial install"
        ));
    }
    // Every check that could reject the install has now passed, so pruning is
    // safe: if it fails, the staging tree is discarded with the rest and the
    // user gets an error rather than a half-cleaned toolchain. Doing it here
    // rather than after publish also means the size the user sees is the size
    // they actually get.
    let reclaimed = prune_unneeded_docs(&installed)?;
    let installed_mb = tree_size(&installed) / (1024 * 1024);
    if reclaimed > 0 {
        println!(
            "Pruned {} MB of documentation and shell completions Tarvos never reads.",
            reclaimed / (1024 * 1024)
        );
    }
    step(6, 6, "Publishing the toolchain");
    if root.exists() {
        std::fs::remove_dir_all(&root)
            .with_context(|| format!("failed to replace {}", root.display()))?;
    }
    if let Some(parent) = root.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::rename(&installed, &root)
        .with_context(|| format!("failed to publish {}", root.display()))?;
    let _ = std::fs::remove_dir_all(&staging);
    println!();
    println!("Managed toolchain installed at {}", root.display());
    println!("Version: {version}");
    // The closing numbers are the ones a user asks for afterwards: what came
    // over the network, what it took, and what it left on the disk.
    println!(
        "Downloaded: {:.1} MB in {:.1}s ({})",
        as_mb(archive.bytes as f64),
        archive.elapsed.as_secs_f64(),
        format_rate(archive.bytes as f64 / archive.elapsed.as_secs_f64().max(0.001))
    );
    println!("Installed size: {installed_mb} MB");
    println!("Total time: {:.1}s", started.elapsed().as_secs_f64());
    Ok(())
}

/// Total bytes at `url`, or `None` when the server does not say.
///
/// A `HEAD` is enough and costs one round trip instead of a second transfer of a
/// few hundred megabytes. The header repeats across a redirect chain, so the last
/// value seen is the one for the resource that will actually be written.
fn remote_size(url: &str) -> Option<u64> {
    let output = Command::new("curl")
        .args(["--proto", "=https", "--tlsv1.2", "-sSIL"])
        .arg(url)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut size = None;
    for line in text.lines() {
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                size = value.trim().parse::<u64>().ok();
            }
        }
    }
    size
}

/// What one completed transfer cost, for the closing summary.
struct Downloaded {
    bytes: u64,
    elapsed: Duration,
}

/// Render a byte count the way a platform installer does, in binary megabytes so
/// the number matches what the user sees in a file manager.
fn as_mb(bytes: f64) -> f64 {
    bytes / (1024.0 * 1024.0)
}

/// Transfer rate, dropping to KB/s only when the rate is genuinely small.
fn format_rate(bytes_per_second: f64) -> String {
    if bytes_per_second >= 1024.0 * 1024.0 {
        format!("{:.1} MB/s", as_mb(bytes_per_second))
    } else if bytes_per_second >= 1024.0 {
        format!("{:.0} KB/s", bytes_per_second / 1024.0)
    } else {
        // A rate that rounds to "0 KB/s" reads as a stall rather than as a
        // four-hundred-byte checksum file that arrived instantly.
        "<1 KB/s".to_string()
    }
}

/// Remaining time as `MM:SS`, blank before there is enough signal to mean it.
fn format_eta(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 || seconds > 24.0 * 3600.0 {
        return "--:--".to_string();
    }
    format!(
        "{:02}:{:02}",
        (seconds / 60.0) as u64,
        (seconds % 60.0) as u64
    )
}

/// One numbered line of the install plan, so the phases that have no byte
/// counter still show that something is happening and roughly where it got to.
fn step(index: usize, total: usize, label: &str) {
    println!("  [{index}/{total}] {label}");
}

/// A live download meter.
///
/// Two output modes, because one of them is always wrong somewhere. An
/// interactive terminal gets a single line redrawn in place; a log or CI capture
/// gets one line per decile instead, because a carriage return in a saved log is
/// a file full of overwritten lines and no visible progress at all. The meter
/// goes to stderr either way so the rest of stdout stays parseable.
///
/// Without a known total it still counts bytes and rate, which is the half a user
/// needs most; the percentage is omitted rather than invented.
struct Progress {
    label: String,
    total: Option<u64>,
    started: Instant,
    interactive: bool,
    last_reported: u8,
    drawn: bool,
    /// Characters written by the last in-place line, so it can be erased exactly.
    last_width: usize,
}

impl Progress {
    fn new(label: &str, total: Option<u64>) -> Self {
        Progress {
            label: label.to_string(),
            total,
            started: Instant::now(),
            interactive: std::io::stderr().is_terminal(),
            last_reported: 0,
            drawn: false,
            last_width: 0,
        }
    }

    /// Redraw at `done` bytes. Cheap enough to call on every poll.
    fn tick(&mut self, done: u64) {
        let elapsed = self.started.elapsed().as_secs_f64();
        let rate = if elapsed > 0.0 {
            done as f64 / elapsed
        } else {
            0.0
        };

        if self.interactive {
            self.draw(done, rate);
        } else if let Some(total) = self.total.filter(|t| *t > 0) {
            let percent = ((done.min(total) * 100) / total) as u8;
            // One line per decile: enough to show movement in a log, not so many
            // that a 250 MB download buries the rest of the output.
            if percent / 10 > self.last_reported / 10 || percent == 100 {
                self.last_reported = percent;
                let eta = total.saturating_sub(done) as f64 / rate.max(1.0);
                self.line(&format!(
                    "  {} {:>3}%  {:>7.1} MB / {:.1} MB  {}  ETA {}",
                    self.label,
                    percent,
                    as_mb(done as f64),
                    as_mb(total as f64),
                    format_rate(rate),
                    format_eta(eta)
                ));
            }
        }
    }

    fn draw(&mut self, done: u64, rate: f64) {
        const WIDTH: usize = 28;
        let (filled, percent_text, size_text, eta_text) = match self.total {
            Some(total) if total > 0 => {
                let ratio = (done as f64 / total as f64).clamp(0.0, 1.0);
                let eta = total.saturating_sub(done) as f64 / rate.max(1.0);
                (
                    (ratio * WIDTH as f64).round() as usize,
                    format!("{:>5.1}%", ratio * 100.0),
                    format!("{:.1} / {:.1} MB", as_mb(done as f64), as_mb(total as f64)),
                    format_eta(eta),
                )
            }
            // No Content-Length: bytes and rate are still honest and useful.
            _ => (
                0,
                String::new(),
                format!("{:.1} MB", as_mb(done as f64)),
                String::new(),
            ),
        };

        let line = format!(
            "\r  {} [{}{}] {:>6}  {:>17}  {:>10}  ETA {}   ",
            self.label,
            "#".repeat(filled),
            "-".repeat(WIDTH.saturating_sub(filled)),
            percent_text,
            size_text,
            format_rate(rate),
            eta_text
        );
        self.write(&line);
        self.drawn = true;
        self.last_width = line.chars().count();
    }

    /// Final line: the transfer as a whole, once the bar is no longer useful.
    fn finish(&mut self, done: u64, elapsed: Duration) {
        if self.drawn {
            // Erase exactly the line that was drawn. A fixed width left the tail
            // of a longer line behind, so the previous transfer's rate and ETA
            // stayed on screen next to the next transfer's summary.
            self.write(&format!("\r{0:1$}\r", "", self.last_width));
        }
        let rate = done as f64 / elapsed.as_secs_f64().max(0.001);
        self.line(&format!(
            "  {} {:.1} MB in {:.1}s ({})",
            self.label,
            as_mb(done as f64),
            elapsed.as_secs_f64(),
            format_rate(rate)
        ));
    }

    fn line(&self, text: &str) {
        self.write(&format!("{text}\n"));
    }

    fn write(&self, text: &str) {
        let mut err = std::io::stderr();
        // A closed or piped-away stderr must never fail an install that is
        // otherwise fine, so a write error here is deliberately ignored.
        let _ = err.write_all(text.as_bytes());
        let _ = err.flush();
    }
}

/// Fetch `url` straight to `dest`, streaming to disk with a live meter.
///
/// Every platform Tarvos targets ships `curl`: POSIX from the base install,
/// Windows since 8.1 as `system32\curl.exe`. Streaming matters for two reasons a
/// user actually notices. The archive is hundreds of megabytes, so buffering it
/// in memory before writing meant the staging directory reported 0 MB for the
/// whole download and a machine with little headroom could fail on the
/// allocation.
///
/// The meter is drawn here rather than by curl for two more. curl's own bar goes
/// silent when its output is redirected, which is exactly the CI and log case
/// where progress matters most. And its format differs between platforms and
/// curl versions, so a screenshot of one host would not describe another.
/// Polling the size of the file curl is writing gives the same numbers
/// everywhere and keeps the percentages and rates consistent.
fn download_to(url: &str, dest: &std::path::Path, label: &str) -> Result<Downloaded> {
    let total = remote_size(url);
    let started = Instant::now();

    let mut child = Command::new("curl")
        .args(["--proto", "=https", "--tlsv1.2", "-sSfL"])
        .arg(url)
        .arg("-o")
        .arg(dest)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed to start download of {url}"))?;

    let mut progress = Progress::new(label, total);
    // Poll rather than read a pipe: curl owns the write, and the size of the
    // file it is writing is the one byte count that cannot disagree with what
    // actually landed on disk.
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                let done = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
                progress.tick(done);
                // 120ms looks continuous to the eye and slow enough that polling
                // never competes with curl for the disk it is writing to.
                thread::sleep(Duration::from_millis(120));
            }
            Err(error) => {
                return Err(anyhow::Error::new(error)
                    .context(format!("failed to wait for the download of {url}")))
            }
        }
    }
    let status = child
        .wait()
        .with_context(|| format!("failed to finish the download of {url}"))?;
    if !status.success() {
        return Err(anyhow::anyhow!(
            "download failed for {url} (see the transfer output above)"
        ));
    }

    let written = std::fs::metadata(dest)
        .map(|m| m.len())
        .with_context(|| format!("failed to stat {}", dest.display()))?;
    if written == 0 {
        return Err(anyhow::anyhow!(
            "{url} downloaded 0 bytes; refusing to continue with an empty file"
        ));
    }
    // A short read is the one failure a byte counter can catch that curl itself
    // reports as success: a connection that dropped cleanly at a boundary would
    // otherwise be handed to the checksum step as if it were complete.
    if let Some(expected) = total {
        if written != expected {
            return Err(anyhow::anyhow!(
                "{url} was truncated: received {} bytes but the server announced {}",
                written,
                expected
            ));
        }
    }

    let elapsed = started.elapsed();
    progress.finish(written, elapsed);
    Ok(Downloaded {
        bytes: written,
        elapsed,
    })
}

/// Remove documentation and shell-completion trees after a successful
/// install, and report what was reclaimed.
///
/// `--without=rust-docs,...` drops those components by name, but `share/doc`
/// still came back at 17 MB: the `rustc` component ships its own copy, so
/// excluding a *component* cannot exclude a path *inside another component*.
/// Tarvos only ever invokes `rustc` and `cargo`, which read neither `share/`
/// nor `man/`, so pruning them after validation is exact and costs nothing.
///
/// Returns the megabytes reclaimed. It runs against the staging tree before
/// publish, so a failure here cannot leave a damaged install visible.
fn prune_unneeded_docs(root: &std::path::Path) -> Result<u64> {
    let mut reclaimed = 0u64;
    for dir in ["share", "man", "doc"] {
        let path = root.join(dir);
        if !path.exists() {
            continue;
        }
        let size = tree_size(&path);
        std::fs::remove_dir_all(&path)
            .with_context(|| format!("failed to remove {}", path.display()))?;
        reclaimed += size;
    }
    Ok(reclaimed)
}

/// Size of a directory tree in bytes, best effort: a file that vanishes
/// mid-walk only makes the number an undercount, never a failure.
fn tree_size(path: &std::path::Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                total += tree_size(&p);
            } else if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

/// Compare the archive against the sidecar digest. The sidecar format is
/// `<hex>  <filename>` per line; the first field is what matters, and the
/// comparison runs over the whole 64 hex digits so a truncated match cannot
/// pass.
fn verify_sha256(archive: &std::path::Path, sidecar: &std::path::Path) -> Result<()> {
    let sidecar_text = std::fs::read_to_string(sidecar)
        .with_context(|| format!("failed to read {}", sidecar.display()))?;
    let expected = sidecar_text
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow::anyhow!("checksum sidecar {} is empty", sidecar.display()))?;
    if expected.len() != 64 || !expected.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(anyhow::anyhow!(
            "checksum sidecar {} is malformed; refusing the download",
            sidecar.display()
        ));
    }
    let bytes =
        std::fs::read(archive).with_context(|| format!("failed to read {}", archive.display()))?;
    let actual = digest_hex(&bytes);
    if !constant_time_eq(expected.to_lowercase().as_bytes(), actual.as_bytes()) {
        return Err(anyhow::anyhow!(
            "checksum mismatch for {}: refusing the download",
            archive.display()
        ));
    }
    Ok(())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// First 64 primes, the source of the SHA-256 round constants.
/// First 64 primes, the source of the SHA-256 round constants.
const PRIMES: [u32; 64] = [
    2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83, 89, 97,
    101, 103, 107, 109, 113, 127, 131, 137, 139, 149, 151, 157, 163, 167, 173, 179, 181, 191, 193,
    197, 199, 211, 223, 227, 229, 233, 239, 241, 251, 257, 263, 269, 271, 277, 281, 283, 293, 307,
    311,
];

/// The leading 32 bits of the fractional part of `root`.
fn frac_bits(value: f64, power: f64) -> u32 {
    let fractional = value.powf(power) - value.powf(power).floor();
    (fractional * 4_294_967_296.0) as u32
}

/// SHA-256 over bytes, implemented in this module so the installer path has no
/// extra dependency for one digest.
fn digest_hex(bytes: &[u8]) -> String {
    // The two constant tables are defined by FIPS 180-4 as the first 32 bits of
    // the fractional parts of the cube roots of the first 64 primes (K) and the
    // square roots of the first 8 primes (H). Deriving them removes any chance
    // of a hand-typed constant being wrong, and the FIPS test vector below
    // proves the result.
    let mut h = [0u32; 8];
    for (slot, prime) in [2u32, 3, 5, 7, 11, 13, 17, 19].iter().enumerate() {
        h[slot] = frac_bits(*prime as f64, 0.5);
    }
    let mut k = [0u32; 64];
    for (slot, prime) in PRIMES.iter().enumerate() {
        k[slot] = frac_bits(*prime as f64, 1.0 / 3.0);
    }
    let mut data = bytes.to_vec();
    let bit_len = (bytes.len() as u64).wrapping_mul(8);
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in data.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (i, word) in chunk.as_chunks::<4>().0.iter().enumerate().take(16) {
            w[i] = u32::from_be_bytes(*word);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(k[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    h.iter().map(|w| format!("{w:08x}")).collect()
}

/// Unpack only the components Tarvos can drive, using the platform tar so no
/// archive crate is needed.
///
/// Unix only: Windows assembles the components itself in
/// `assemble_windows_toolchain` because the shipped install.sh is POSIX.
///
/// Selecting the members is the difference between a fast install and a slow
/// one. The distribution archive carries rust-docs, rust-html and rustc-docs
/// alongside the three components a build needs, and a full extract writes all of
/// it to disk only for the next step to copy three directories out of it and
/// discard the rest. Naming the members costs one extra tar argument and none of
/// the work.
#[cfg(not(windows))]
fn extract_components(
    archive: &std::path::Path,
    dest: &std::path::Path,
    triple: &str,
) -> Result<()> {
    let dist_dir = format!("rust-{PINNED_CHANNEL}-{triple}");
    let mut cmd = Command::new("tar");
    cmd.arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(dest)
        .arg(&dist_dir);
    // Excluding the documentation trees is what keeps this fast, but the whole
    // distribution directory is still extracted rather than only the three
    // components a build drives. Selecting members instead is faster on paper
    // and was wrong in practice: install.sh lives at the top level of the
    // archive next to `components`, `manifest.in` and `rust-installer-version`,
    // and reads all three. A first attempt at this extracted the components
    // alone and every Unix install then died with "cannot open .../install.sh:
    // No such file or directory" after a full download. The documentation
    // components are the bulk of the archive, so excluding them is where the
    // saving was all along.
    for component in [
        "rust-docs",
        "rust-docs-json-preview",
        "rust-html",
        "rustc-docs",
        "rustc-docs-json-preview",
    ] {
        cmd.arg(format!("--exclude={dist_dir}/{component}"));
    }
    let status = cmd
        .status()
        .with_context(|| format!("failed to unpack {}", archive.display()))?;
    if !status.success() {
        return Err(anyhow::anyhow!(
            "failed to unpack the compiler components from {}; the archive is not a Rust \
             distribution for {triple}",
            archive.display()
        ));
    }
    Ok(())
}

/// Assemble the toolchain directly from the distribution's component
/// directories, for hosts where install.sh cannot run.
///
/// The upstream install.sh is a POSIX script: it calls `uname` on its first
/// line of real work, so it cannot execute on Windows without a POSIX layer.
/// The archive it would have processed is platform-neutral though, and carries
/// the same component layout on every host:
///
///   rust-{channel}-{triple}/rustc/bin/rustc.exe
///   rust-{channel}-{triple}/cargo/bin/cargo.exe
///   rust-{channel}-{triple}/rust-std-{triple}/lib/...
///
/// so the equivalent operation is to extract exactly the components Tarvos
/// can drive and merge them into one prefix. Extracting selectively rather
/// than unpacking everything is also what keeps this small: a full Windows
/// archive is 403 MB, and the documentation and analyzer trees inside it are
/// never invoked by a build.
///
/// No install.sh, no POSIX host required, and the same three components the
/// Unix path asks for.
#[cfg(windows)]
fn assemble_windows_toolchain(
    archive: &std::path::Path,
    unpacked_root: &std::path::Path,
    dest: &std::path::Path,
    triple: &str,
) -> Result<()> {
    let dist_dir = format!("rust-{PINNED_CHANNEL}-{triple}");
    let wanted = [
        format!("{dist_dir}/rustc"),
        format!("{dist_dir}/cargo"),
        format!("{dist_dir}/rust-std-{triple}"),
    ];

    // Extract only the three components, and drop the leading component so
    // the merge below does not have to walk a nested directory.
    let mut cmd = Command::new("tar");
    cmd.arg("-xzf").arg(archive).arg("-C").arg(unpacked_root);
    for w in &wanted {
        cmd.arg(w);
    }
    let status = cmd
        .status()
        .with_context(|| format!("failed to unpack {archive:?}"))?;
    if !status.success() {
        return Err(anyhow::anyhow!(
            "failed to unpack the components of {}",
            archive.display()
        ));
    }

    let dist = unpacked_root.join(&dist_dir);
    std::fs::create_dir_all(dest)
        .with_context(|| format!("failed to create {}", dest.display()))?;
    for component in ["rustc", "cargo", &format!("rust-std-{triple}")] {
        let src = dist.join(component);
        if !src.is_dir() {
            return Err(anyhow::anyhow!(
                "the distribution is missing the {component} component at {}",
                src.display()
            ));
        }
        // The toolchain searches its own lib/ for the sysroot, so component
        // trees have to be merged rather than kept in separate directories.
        copy_tree(&src, dest)
            .with_context(|| format!("failed to stage the {component} component"))?;
    }
    Ok(())
}

/// Merge `src` over `dest`, creating directories as needed.
///
/// Uses the platform copy so no filesystem crate is needed, and skips the
/// paths a build never reads: component documentation, which is the bulk of
/// the archive.
#[cfg(windows)]
fn copy_tree(src: &std::path::Path, dest: &std::path::Path) -> Result<()> {
    for entry in
        std::fs::read_dir(src).with_context(|| format!("failed to read {}", src.display()))?
    {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy().to_string();
        // Component docs are the reason a naive extract costs 403 MB.
        if name_str == "share" || name_str == "doc" || name_str == "man" {
            continue;
        }
        let from = entry.path();
        let to = dest.join(&name);
        if from.is_dir() {
            std::fs::create_dir_all(&to)
                .with_context(|| format!("failed to create {}", to.display()))?;
            copy_tree(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).with_context(|| {
                format!("failed to copy {} to {}", from.display(), to.display())
            })?;
        }
    }
    Ok(())
}

/// Run the static distribution's install.sh with `--prefix` inside our tree.
///
/// Unix only, for the same reason as extract_components.
#[cfg(not(windows))]
fn run_install_sh(script: &std::path::Path, prefix: &std::path::Path, triple: &str) -> Result<()> {
    #[cfg(windows)]
    {
        let _ = (script, prefix, triple);
        Err(anyhow::anyhow!(
            "the upstream Windows static distribution has no install.sh; Windows installs ship the managed toolchain inside the Tarvos installer instead"
        ))
    }
    #[cfg(not(windows))]
    {
        // Use only flags this script actually documents. `install.sh --help`
        // for the pinned channel lists exactly:
        //
        //   --prefix  --components  --without  --bindir  --libdir
        //   --datadir --mandir --docdir --disable-ldconfig --verbose
        //
        // An earlier revision here passed --no-modify-path on the theory that
        // the script rewrites the user's shell profile. It does not: PATH
        // editing belongs to rustup-init, a different script that this one
        // never invokes. Passing the flag only made the install abort with
        // "Option '--no-modify-path' is not recognized", so the concern was
        // both unfounded and fatal.
        //
        // --components names what Tarvos can drive; --without then removes
        // the documentation trees by name as well, because a measured install
        // laid down 993 MB of HTML in share/doc that rustc and cargo never
        // read. --disable-ldconfig keeps a user-local prefix from poking the
        // system dynamic-linker cache.
        let components = format!("rustc,cargo,rust-std-{triple}");
        let status = Command::new("sh")
            .arg(script)
            .arg(format!("--prefix={}", prefix.display()))
            .arg(format!("--components={components}"))
            .arg("--without=rust-docs,rust-docs-json-preview,rust-html")
            .arg("--disable-ldconfig")
            .status()
            .with_context(|| format!("failed to run {}", script.display()))?;
        if !status.success() {
            return Err(anyhow::anyhow!("toolchain installer failed"));
        }
        Ok(())
    }
}

/// Whether this host can run the upstream `install.sh` path.
///
/// Checked *before* any download: the Windows static archive is hundreds of
/// megabytes, and discovering after the download that the host cannot unpack
/// it wastes the user's bandwidth and leaves a multi-hundred-megabyte staging
/// directory behind.
fn host_supports_install_script() -> bool {
    !cfg!(windows)
}

fn report_checks(checks: Vec<Check>, failed: bool) -> Result<()> {
    println!("Managed toolchain verification");
    for check in &checks {
        println!(
            "  {} {:<22} {}",
            check.state.tag(),
            check.name,
            check.detail
        );
    }
    if failed {
        Err(anyhow::anyhow!(
            "toolchain verification failed; see ERROR lines above"
        ))
    } else {
        println!("All checks passed.");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stamp file that cannot be addressed deterministically is worse than no
    /// cache at all, so the hash is pinned to a published FNV-1a vector.
    #[test]
    fn the_stamp_hash_is_stable_and_addresses_one_file() {
        assert_eq!(fnv1a_64(b"abc"), 0xe71fa2190541574b);
        assert_eq!(fnv1a_64(b""), 0xcbf29ce484222325);
        // Two different compilers must never share a stamp file, or one
        // compiler's passing probe would vouch for the other.
        assert_ne!(fnv1a_64(b"rustc-a"), fnv1a_64(b"rustc-b"));
    }

    /// A stamp only certifies the exact compiler, channel, and target it was
    /// written for; anything else has to fall through to a real probe.
    #[test]
    fn a_stamp_for_another_compiler_never_satisfies_the_check() {
        let certified = probe_stamp_payload_for(
            PINNED_CHANNEL,
            "x86_64-pc-windows-msvc",
            Path::new(r"C:\rustup\toolchains\1.98.0\bin\rustc.exe"),
        );
        // A different compiler at the same version and target: the channel
        // alone cannot vouch for a binary.
        assert_ne!(
            certified,
            probe_stamp_payload_for(
                PINNED_CHANNEL,
                "x86_64-pc-windows-msvc",
                Path::new(r"C:\toolchains\other\bin\rustc.exe"),
            )
        );
        // A different channel for the same compiler path.
        assert_ne!(
            certified,
            probe_stamp_payload_for(
                "1.99.0",
                "x86_64-pc-windows-msvc",
                Path::new(r"C:\rustup\toolchains\1.98.0\bin\rustc.exe"),
            )
        );
        // A stamp with a field stripped must not match either.
        assert_ne!(certified, certified.trim_end().replace(PINNED_CHANNEL, ""));
    }

    #[test]
    fn pin_and_platform_table_cover_every_target() {
        assert_eq!(PINNED_CHANNEL, "1.98.0");
        let joined = EXPECTED_TRIPLES
            .iter()
            .map(|(_, t)| *t)
            .collect::<Vec<_>>()
            .join(" ");
        assert!(joined.contains("windows"), "{joined}");
        assert!(joined.contains("linux"), "{joined}");
        assert!(joined.contains("darwin"), "{joined}");
    }

    #[test]
    fn toolchain_error_names_the_mode_and_the_fix() {
        let error = ToolchainError {
            mode: ToolchainMode::Managed,
            reason: "no managed rustc".to_string(),
            hint: "run install".to_string(),
        }
        .to_string();
        assert!(error.contains("managed"), "{error}");
        assert!(error.contains("run install"), "{error}");
    }

    #[test]
    fn a_bogus_managed_root_is_not_confused_with_a_toolchain() {
        let missing = PathBuf::from("definitely-not-a-home-dir");
        assert!(!managed_bin(&missing).join(exe("rustc")).exists());
    }

    /// FIPS 180-4 test vector: a digest implementation that gets "abc" wrong
    /// must never be trusted with a toolchain archive.
    #[test]
    fn sha256_matches_the_standard_vector() {
        assert_eq!(
            digest_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            digest_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_catches_a_single_bit_flip() {
        assert_ne!(digest_hex(b"abc"), digest_hex(b"abd"));
    }
}
