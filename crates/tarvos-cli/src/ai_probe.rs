//! Read-only detection of the optional local AI capability (Ollama).
//!
//! This module is a *probe*. It never starts, stops, downloads, or configures
//! anything, and it never leaves the loopback interface. That separation is the
//! point: installation may ask "is the local AI capability present?", but the
//! answer must not be a side effect.
//!
//! Everything here is bounded — every socket has a timeout — so a missing or
//! wedged Ollama degrades the report instead of hanging `tarvos doctor`.

use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;

/// Loopback-only by construction: a configured endpoint is rejected unless it
/// resolves to loopback, so a stray environment variable cannot turn a health
/// check into an outbound request.
const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:11434";
const ENV_ENDPOINT: &str = "TARVOS_OLLAMA_ENDPOINT";

/// Short enough that a dead port is reported immediately, long enough not to
/// race a service that is still binding.
const CONNECT_TIMEOUT: Duration = Duration::from_millis(400);
const READ_TIMEOUT: Duration = Duration::from_millis(1500);

/// How the local AI capability on this machine came to exist.
///
/// This distinction is what protects a pre-existing or enterprise-managed
/// install from being "cleaned up" by a future repair or uninstall step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Ownership {
    /// Tarvos provisioned this installation and therefore owns its lifecycle.
    TarvosProvisioned,
    /// Present before Tarvos. Tarvos depends on it; it must not be modified,
    /// stopped, or removed on Tarvos's behalf.
    External,
    /// No local AI capability is present.
    Absent,
}

/// Result of probing the optional local AI capability.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AiCapability {
    /// Whether the `ollama` executable was found. Detection is path-based and
    /// does not execute the binary.
    pub binary_present: bool,
    /// Where it was found, when it was.
    pub binary_path: Option<PathBuf>,
    /// The endpoint probed, after loopback validation.
    pub endpoint: String,
    /// Whether the endpoint could be contacted at all.
    pub service_reachable: bool,
    /// Models the local service reports. A local model, never a remote one.
    pub models: Vec<String>,
    /// Provenance, never inferred from presence alone.
    pub ownership: Ownership,
    /// Why the probe could not report something, if that happened.
    pub note: Option<String>,
}

impl AiCapability {
    /// The capability is usable only when a service actually answers.
    ///
    /// A binary with no running service is deliberately *not* sufficient: the
    /// compiler would have to start it, which this subsystem does not do.
    pub fn is_usable(&self) -> bool {
        self.service_reachable
    }
}

/// Resolve the endpoint to probe, rejecting anything that is not loopback.
///
/// Returns an error for a non-loopback host rather than dialing it. This is the
/// single place the local-only guarantee is enforced.
pub fn resolve_endpoint() -> Result<String, String> {
    let configured = std::env::var(ENV_ENDPOINT).unwrap_or_else(|_| DEFAULT_ENDPOINT.to_string());
    let host = host_of(&configured)
        .ok_or_else(|| format!("{ENV_ENDPOINT} is not a valid URL: {configured:?}"))?;
    if !is_loopback_host(&host) {
        return Err(format!(
            "{ENV_ENDPOINT} must point at a loopback address, but {host:?} is not loopback. \
             Tarvos never contacts a remote AI service."
        ));
    }
    Ok(configured.trim_end_matches('/').to_string())
}

/// Extract the host from `scheme://host:port/...`, or a bare `host:port`.
fn host_of(endpoint: &str) -> Option<String> {
    let after_scheme = match endpoint.find("://") {
        Some(index) => &endpoint[index + 3..],
        None => endpoint,
    };
    let authority = after_scheme
        .split('/')
        .next()
        .unwrap_or(after_scheme)
        .trim();
    if authority.is_empty() {
        return None;
    }
    // `[::1]:11434` keeps its brackets in IPv6 form; bare hosts do not.
    if let Some(rest) = authority.strip_prefix('[') {
        return rest.split(']').next().map(|host| host.to_string());
    }
    Some(
        authority
            .rsplit_once(':')
            .map(|(host, _port)| host)
            .unwrap_or(authority)
            .to_string(),
    )
}

fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    // Accept the whole 127.0.0.0/8 block, not just 127.0.0.1.
    if let Some(octets) = host.strip_prefix("127.") {
        return octets.split('.').count() == 3
            && octets.split('.').all(|part| {
                part.parse::<u8>()
                    .map(|value| (0..=255).contains(&value))
                    .unwrap_or(false)
            });
    }
    host == "::1" || host == "0:0:0:0:0:0:0:1"
}

/// Find an executable on `PATH` without invoking a shell.
///
/// Using the platform path splitter keeps this correct on Windows (where `PATH`
/// entries are separated by `;` and may be quoted) and avoids the
/// command-injection surface of shelling out to `where`/`which`.
pub fn find_on_path(name: &str, path_var: Option<std::ffi::OsString>) -> Option<PathBuf> {
    let file_name = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let path = path_var.or_else(|| std::env::var_os("PATH"))?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(&file_name))
        .find(|candidate| candidate.is_file())
}

/// Well-known install locations checked in addition to `PATH`.
///
/// The Windows installer does not add Ollama to the current user's `PATH`, so a
/// path scan alone would report a false negative on exactly the platform that
/// matters most here.
pub fn well_known_locations() -> Vec<PathBuf> {
    let file_name = format!("ollama{}", std::env::consts::EXE_SUFFIX);
    let mut candidates = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(
            PathBuf::from(local)
                .join("Programs")
                .join("Ollama")
                .join(&file_name),
        );
    }
    if let Some(program_files) = std::env::var_os("ProgramFiles") {
        candidates.push(PathBuf::from(program_files).join("Ollama").join(&file_name));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(
            PathBuf::from(local)
                .join("Programs")
                .join("Ollama")
                .join("ollama.exe"),
        );
    }
    candidates
}

/// Find the Ollama executable, preferring `PATH` and falling back to the
/// well-known install locations.
pub fn find_ollama_binary() -> Option<PathBuf> {
    find_on_path("ollama", None).or_else(|| {
        well_known_locations()
            .into_iter()
            .find(|candidate| candidate.is_file())
    })
}

/// Probe the local AI capability without changing anything.
///
/// The result deliberately never claims [`Ownership::TarvosProvisioned`]: a
/// read-only probe can observe that a capability is *present*, but only an
/// actual provisioner can know that Tarvos *installed* it. Everything else is
/// recomputed from scratch on every probe, so a stale record can never make an
/// absent capability look present (or the reverse).
pub fn probe() -> AiCapability {
    let binary_path = find_ollama_binary();
    let binary_present = binary_path.is_some();

    let endpoint = match resolve_endpoint() {
        Ok(endpoint) => endpoint,
        Err(reason) => {
            return AiCapability {
                binary_present,
                binary_path,
                endpoint: DEFAULT_ENDPOINT.to_string(),
                service_reachable: false,
                models: Vec::new(),
                ownership: Ownership::Absent,
                note: Some(reason),
            };
        }
    };

    match query_service(&endpoint) {
        Ok(models) => AiCapability {
            binary_present,
            binary_path,
            endpoint,
            service_reachable: true,
            models,
            // Present but not proven to be ours: treat as external.
            ownership: Ownership::External,
            note: None,
        },
        Err(reason) => AiCapability {
            binary_present,
            binary_path,
            endpoint,
            service_reachable: false,
            models: Vec::new(),
            // Presence of a binary is not presence of a capability. With no
            // service answering, the honest answer is "absent".
            ownership: Ownership::Absent,
            note: Some(reason),
        },
    }
}

/// Ask the local service which models it has, via `GET /api/tags`.
///
/// A minimal HTTP/1.1 request is written directly rather than pulling in an HTTP
/// client: the dependency would dwarf the functionality, and it keeps the
/// installed surface small. `Connection: close` makes the service close the
/// socket, so the read terminates on a clean end-of-stream.
fn query_service(endpoint: &str) -> Result<Vec<String>, String> {
    let host = host_of(endpoint).ok_or_else(|| "endpoint has no host".to_string())?;
    let authority = authority_of(endpoint);
    let addr = authority
        .to_socket_addrs()
        .map_err(|error| format!("cannot resolve {authority}: {error}"))?
        .next()
        .ok_or_else(|| format!("no address for {authority}"))?;

    let mut stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)
        .map_err(|error| format!("cannot reach {authority}: {error}"))?;
    stream.set_read_timeout(Some(READ_TIMEOUT)).ok();
    stream.set_write_timeout(Some(READ_TIMEOUT)).ok();

    let request = format!(
        "GET /api/tags HTTP/1.1\r\nHost: {host}\r\nAccept: application/json\r\n\
         User-Agent: tarvos-ai-status\r\nConnection: close\r\n\r\n"
    );
    {
        use std::io::Write;
        stream
            .write_all(request.as_bytes())
            .map_err(|error| format!("request failed: {error}"))?;
    }

    let mut response = Vec::new();
    {
        use std::io::Read;
        // Bounded by READ_TIMEOUT, so a service that never answers cannot hang
        // the command.
        let _ = stream.read_to_end(&mut response);
    }
    parse_tags_response(&String::from_utf8_lossy(&response))
}

fn authority_of(endpoint: &str) -> String {
    let after_scheme = match endpoint.find("://") {
        Some(index) => &endpoint[index + 3..],
        None => endpoint,
    };
    after_scheme
        .split('/')
        .next()
        .unwrap_or(after_scheme)
        .trim()
        .to_string()
}

/// Parse a raw `GET /api/tags` response into model names.
///
/// Split out from the socket code so the parsing is testable without a service.
pub fn parse_tags_response(response: &str) -> Result<Vec<String>, String> {
    let (status, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "malformed HTTP response".to_string())?;
    let status_ok = status
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .map(|code| code == "200")
        .unwrap_or(false);
    if !status_ok {
        return Err(format!(
            "local AI service returned status {}",
            status.lines().next().unwrap_or("?")
        ));
    }
    #[derive(Deserialize)]
    struct TagsResponse {
        #[serde(default)]
        models: Vec<TagModel>,
    }
    #[derive(Deserialize)]
    struct TagModel {
        #[serde(default)]
        name: String,
    }
    let parsed: TagsResponse =
        serde_json::from_str(body.trim()).map_err(|error| format!("invalid JSON: {error}"))?;
    Ok(parsed
        .models
        .into_iter()
        .map(|model| model.name)
        .filter(|name| !name.is_empty())
        .collect())
}

/// Seconds since the Unix epoch, saturating rather than panicking on a clock
/// set before 1970.
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0)
}

/// State file recording what the probe observed and who owns the capability.
///
/// Persisting this is what lets a later repair, upgrade, or uninstall step tell
/// a Tarvos-provisioned install apart from one it must leave alone.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AiState {
    /// Bumped when the shape changes so an older record is re-probed rather
    /// than misread.
    pub schema: u32,
    pub ownership: Ownership,
    pub binary_path: Option<PathBuf>,
    pub endpoint: String,
    pub service_reachable: bool,
    pub models: Vec<String>,
    pub checked_at_unix: u64,
}

/// Current on-disk schema version.
pub const AI_STATE_SCHEMA: u32 = 1;

impl AiState {
    /// Build a record from a probe result.
    ///
    /// Only [`Ownership::TarvosProvisioned`] is sticky. That value is a fact
    /// established by a real provisioner, so it must survive later probes.
    /// `External` and `Absent` both mean "not ours" and are recomputed from the
    /// live probe, so a record written while nothing was running cannot make a
    /// later, present capability look absent (or vice versa).
    pub fn from_probe(capability: &AiCapability, previous: Option<&AiState>) -> AiState {
        let ownership = previous
            .filter(|state| state.ownership == Ownership::TarvosProvisioned)
            .map(|state| state.ownership)
            .unwrap_or(capability.ownership);
        AiState {
            schema: AI_STATE_SCHEMA,
            ownership,
            binary_path: capability.binary_path.clone(),
            endpoint: capability.endpoint.clone(),
            service_reachable: capability.service_reachable,
            models: capability.models.clone(),
            checked_at_unix: now_unix(),
        }
    }

    /// Read a record, treating an unknown schema as absent rather than guessing.
    pub fn read(path: &Path) -> Option<AiState> {
        let contents = std::fs::read_to_string(path).ok()?;
        let state: AiState = serde_json::from_str(&contents).ok()?;
        (state.schema == AI_STATE_SCHEMA).then_some(state)
    }

    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let encoded = serde_json::to_string_pretty(self)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        std::fs::write(path, encoded)
    }
}

/// Human-readable summary used by both the CLI report and installer logs.
pub fn describe_ownership(ownership: Ownership) -> &'static str {
    match ownership {
        Ownership::TarvosProvisioned => "tarvos-provisioned",
        Ownership::External => "external (detected, not managed by Tarvos)",
        Ownership::Absent => "absent",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capability(usable: bool) -> AiCapability {
        AiCapability {
            binary_present: usable,
            binary_path: None,
            endpoint: DEFAULT_ENDPOINT.to_string(),
            service_reachable: usable,
            models: if usable {
                vec!["m:latest".to_string()]
            } else {
                Vec::new()
            },
            ownership: if usable {
                Ownership::External
            } else {
                Ownership::Absent
            },
            note: None,
        }
    }

    fn state(ownership: Ownership) -> AiState {
        AiState {
            schema: AI_STATE_SCHEMA,
            ownership,
            binary_path: None,
            endpoint: DEFAULT_ENDPOINT.to_string(),
            service_reachable: true,
            models: Vec::new(),
            checked_at_unix: 0,
        }
    }

    #[test]
    fn only_loopback_endpoints_are_accepted() {
        // `is_loopback_host` takes an already-extracted host, so bracket-stripping
        // of `[::1]` is `host_of`'s job and is covered by the parsing test.
        for host in ["127.0.0.1", "127.1.2.3", "localhost", "::1"] {
            assert!(
                is_loopback_host(host),
                "{host} should be treated as loopback"
            );
        }
        for host in [
            "example.com",
            "10.0.0.1",
            "0.0.0.0",
            "128.0.0.1",
            "127.0.0.1.evil.com",
            "192.168.1.10",
            "[::1]",
        ] {
            assert!(
                !is_loopback_host(host),
                "{host} must not be treated as a bare loopback host"
            );
        }
    }

    #[test]
    fn host_parsing_handles_scheme_port_and_ipv6() {
        assert_eq!(
            host_of("http://127.0.0.1:11434"),
            Some("127.0.0.1".to_string())
        );
        assert_eq!(host_of("127.0.0.1:11434"), Some("127.0.0.1".to_string()));
        assert_eq!(host_of("http://[::1]:11434"), Some("::1".to_string()));
        assert_eq!(host_of("localhost:11434"), Some("localhost".to_string()));
        assert_eq!(host_of("http://"), None);
        assert_eq!(host_of(""), None);
    }

    #[test]
    fn a_non_loopback_endpoint_is_refused_rather_than_dialled() {
        let previous = std::env::var(ENV_ENDPOINT).ok();
        std::env::set_var(ENV_ENDPOINT, "http://api.example.com:11434");
        let rejected = resolve_endpoint().is_err();
        match previous {
            Some(value) => std::env::set_var(ENV_ENDPOINT, value),
            None => std::env::remove_var(ENV_ENDPOINT),
        }
        assert!(rejected, "a remote endpoint must be refused");
    }

    #[test]
    fn tags_response_parsing_extracts_model_names() {
        let response = concat!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n",
            r#"{"models":[{"name":"a:latest"},{"name":"b:7b"}]}"#
        );
        assert_eq!(
            parse_tags_response(response).unwrap(),
            vec!["a:latest".to_string(), "b:7b".to_string()]
        );
    }

    #[test]
    fn tags_response_parsing_handles_empty_and_error_responses() {
        let empty = "HTTP/1.1 200 OK\r\n\r\n{\"models\":[]}";
        assert!(parse_tags_response(empty).unwrap().is_empty());

        let missing_field = "HTTP/1.1 200 OK\r\n\r\n{}";
        assert!(parse_tags_response(missing_field).unwrap().is_empty());

        let error = "HTTP/1.1 404 Not Found\r\n\r\n{}";
        assert!(parse_tags_response(error).is_err());

        let garbage = "not an http response";
        assert!(parse_tags_response(garbage).is_err());
    }

    #[test]
    fn a_stale_absent_record_does_not_mask_a_present_capability() {
        // Regression: a record written while nothing was listening used to keep
        // reporting "absent" even after the service came up, which would mislead
        // a later repair or uninstall step.
        let previous = state(Ownership::Absent);
        let now = AiState::from_probe(&capability(true), Some(&previous));
        assert_eq!(now.ownership, Ownership::External);
        assert!(now.service_reachable);
    }

    #[test]
    fn a_probe_can_never_claim_tarvos_provisioned_ownership() {
        // Only a real provisioner may assert this, so a probe must never
        // manufacture it — and must not lose it either.
        let fresh = AiState::from_probe(&capability(true), None);
        assert_eq!(fresh.ownership, Ownership::External);

        let provisioned = state(Ownership::TarvosProvisioned);
        let preserved = AiState::from_probe(&capability(true), Some(&provisioned));
        assert_eq!(preserved.ownership, Ownership::TarvosProvisioned);
    }

    #[test]
    fn an_absent_capability_is_recorded_as_absent() {
        let now = AiState::from_probe(&capability(false), None);
        assert_eq!(now.ownership, Ownership::Absent);
        assert!(!now.service_reachable);
    }

    #[test]
    fn find_on_path_locates_an_executable_without_a_shell() {
        let dir = std::env::temp_dir().join(format!("tarvos-ai-probe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp probe dir");
        let file_name = format!("probe-tool{}", std::env::consts::EXE_SUFFIX);
        let target = dir.join(&file_name);
        std::fs::write(&target, b"").expect("write stub executable");

        let found = find_on_path("probe-tool", Some(std::ffi::OsString::from(dir.clone())));
        assert_eq!(found.as_deref(), Some(target.as_path()));

        // A directory with no such file must not match.
        assert!(find_on_path("absent-tool", Some(std::ffi::OsString::from(dir.clone()))).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn state_round_trips_and_rejects_an_unknown_schema() {
        let dir = std::env::temp_dir().join(format!("tarvos-ai-state-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp state dir");
        let path = dir.join("nested").join("ai-capability.json");

        let original = AiState::from_probe(&capability(true), None);
        original.write(&path).expect("write state");
        assert_eq!(AiState::read(&path), Some(original.clone()));

        let mut stale = original;
        stale.schema = AI_STATE_SCHEMA + 1;
        stale.write(&path).expect("write stale state");
        assert_eq!(
            AiState::read(&path),
            None,
            "an unknown schema must be re-probed rather than misread"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
