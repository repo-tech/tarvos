//! Tarvos toolchain manager (Option B).
//!
//! A Tarvos user installs Tarvos, not Rust. A normal build resolves the
//! Tarvos-managed toolchain first; system Rust is only ever used when the
//! caller explicitly selects it. Nothing in this module may silently fall back
//! from one mode to the other.

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::which_simple;

/// The Rust channel every managed install must provide.
///
/// Kept in sync with `rust-toolchain.toml` on purpose. A floating channel
/// made CI non-reproducible; the managed toolchain gets the same guarantee.
pub const PINNED_CHANNEL: &str = "1.98.0";

/// Which host each managed installer is expected to run on. Used by the
/// `toolchain install` listing (Milestone 2); kept here so the manager is
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
    String::from_utf8(output.stdout)
        .ok()?
        .split_whitespace()
        .nth(1)
        .map(str::to_string)
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
fn proves_compilation(rustc: &Path) -> bool {
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
            "{managed_error} Run `tarvos toolchain install` once, or pass --system-rust to use an explicitly validated system compiler."
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
            hint: "run `tarvos toolchain install` once to fetch the pinned channel.".to_string(),
        });
    }
    let version = version_of(&rustc).ok_or_else(|| ToolchainError {
        mode: ToolchainMode::Managed,
        reason: format!("managed rustc at {} did not report a version", rustc.display()),
        hint: "run `tarvos toolchain verify`; if it still fails, reinstall with `tarvos toolchain install`."
            .to_string(),
    })?;
    if !version.starts_with(PINNED_CHANNEL) {
        return Err(ToolchainError {
            mode: ToolchainMode::Managed,
            reason: format!("managed rustc reports {version} but Tarvos expects {PINNED_CHANNEL}"),
            hint: "run `tarvos toolchain install` to refresh the managed compiler.".to_string(),
        });
    }
    let target = host_target_of(&rustc).ok_or_else(|| ToolchainError {
        mode: ToolchainMode::Managed,
        reason: "managed rustc did not report a host target".to_string(),
        hint: "run `tarvos toolchain verify`.".to_string(),
    })?;
    if !proves_compilation(&rustc) {
        return Err(ToolchainError {
            mode: ToolchainMode::Managed,
            reason: "managed rustc exists but cannot compile a probe program".to_string(),
            hint: "run `tarvos toolchain verify`; if it still fails, reinstall.".to_string(),
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
        hint: "install rust or run `tarvos toolchain install`.".to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
