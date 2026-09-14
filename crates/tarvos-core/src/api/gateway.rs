//! Axum gateway for the Tarvos SaaS control plane.
//!
//! The gateway deliberately exposes a narrow protocol:
//! - `POST /api/v1/tarvos/compile` accepts one `.py` or `.zip` upload.
//! - `GET /api/v1/tarvos/terminal` upgrades to a WebSocket and accepts only
//!   `analyze`, `compile`, and `validate`.
//!
//! Terminal commands are never passed through a shell on the host. They are
//! translated to fixed arguments and run in a read-only, network-disabled
//! Docker container.

use std::{
    env,
    ffi::OsStr,
    io::Cursor,
    path::{Path, PathBuf},
    process::Stdio,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    body::Body,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Multipart, State,
    },
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::{
    fs,
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    process::Command,
    sync::mpsc,
};

use crate::CompilePipeline;

const MAX_UPLOAD_BYTES: usize = 50 * 1024 * 1024;
const SANDBOX_IMAGE_ENV: &str = "TARVOS_SANDBOX_IMAGE";
const DEFAULT_SANDBOX_IMAGE: &str = "tarvos/sandbox:1.5.0";

#[derive(Clone, Debug)]
pub struct GatewayConfig {
    pub cache_root: PathBuf,
    pub workspace_root: PathBuf,
    pub sandbox_image: String,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            cache_root: cache_root(),
            workspace_root: cache_root(),
            sandbox_image: env::var(SANDBOX_IMAGE_ENV)
                .unwrap_or_else(|_| DEFAULT_SANDBOX_IMAGE.to_owned()),
        }
    }
}

pub fn router(config: GatewayConfig) -> Router {
    Router::new()
        .route("/", get(health))
        .route("/health", get(health))
        .route("/api/v1/tarvos/compile", post(compile))
        .route("/api/v1/tarvos/terminal", get(terminal))
        .with_state(config)
}

async fn health() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        r#"{"status":"operational","version":"1.5.0"}"#,
    )
}

async fn compile(
    State(config): State<GatewayConfig>,
    mut multipart: Multipart,
) -> Result<Response, GatewayError> {
    let mut payload = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(GatewayError::bad_request)?
    {
        let filename = field.file_name().unwrap_or("upload.py").to_owned();
        let bytes = field.bytes().await.map_err(GatewayError::bad_request)?;
        if bytes.len() > MAX_UPLOAD_BYTES {
            return Err(GatewayError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "upload exceeds the 50 MiB limit",
            ));
        }
        if payload.is_some() {
            return Err(GatewayError::new(
                StatusCode::BAD_REQUEST,
                "send exactly one Python file or ZIP project",
            ));
        }
        payload = Some((filename, bytes.to_vec()));
    }
    let (filename, bytes) = payload.ok_or_else(|| {
        GatewayError::new(StatusCode::BAD_REQUEST, "multipart payload is required")
    })?;

    let job = unique_path(&config.cache_root, "gateway-job");
    fs::create_dir_all(&job)
        .await
        .map_err(GatewayError::internal)?;
    let result = compile_upload(&job, &filename, &bytes).await;
    let cleanup = fs::remove_dir_all(&job).await;
    if let Err(error) = cleanup {
        eprintln!(
            "Tarvos gateway cleanup failed for {}: {error}",
            job.display()
        );
    }
    let binary = result?;

    let mut response = Response::new(Body::from(binary));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"tarvos-app\""),
    );
    Ok(response)
}

async fn compile_upload(job: &Path, filename: &str, bytes: &[u8]) -> Result<Vec<u8>, GatewayError> {
    let source = if filename.to_ascii_lowercase().ends_with(".zip") {
        extract_zip(job, bytes).map_err(GatewayError::bad_request)?
    } else if filename.to_ascii_lowercase().ends_with(".py") {
        let path = job.join("main.py");
        fs::write(&path, bytes)
            .await
            .map_err(GatewayError::internal)?;
        path
    } else {
        return Err(GatewayError::new(
            StatusCode::BAD_REQUEST,
            "only .py and .zip uploads are supported",
        ));
    };

    let rust_source = tokio::task::spawn_blocking(move || CompilePipeline::transpile_file(&source))
        .await
        .map_err(GatewayError::internal)?
        .map_err(GatewayError::compiler)?;
    let rust_file = job.join("main.rs");
    let binary_file = job.join(if cfg!(windows) {
        "tarvos-app.exe"
    } else {
        "tarvos-app"
    });
    fs::write(&rust_file, rust_source)
        .await
        .map_err(GatewayError::internal)?;

    let status = Command::new("rustc")
        .args([
            "--edition",
            "2021",
            "-C",
            "opt-level=3",
            "-C",
            "strip=symbols",
            "-C",
            "panic=abort",
        ])
        .arg(&rust_file)
        .arg("-o")
        .arg(&binary_file)
        .status()
        .await
        .map_err(GatewayError::toolchain)?;
    if !status.success() {
        return Err(GatewayError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "generated Rust failed to compile",
        ));
    }
    fs::read(binary_file).await.map_err(GatewayError::internal)
}

fn extract_zip(job: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|error| error.to_string())?;
    let mut entrypoint = None;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| "ZIP contains an unsafe path".to_owned())?
            .to_path_buf();
        if relative.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir | std::path::Component::RootDir
            )
        }) {
            return Err("ZIP contains an unsafe path".to_owned());
        }
        let destination = job.join(&relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
            continue;
        }
        if entry.size() > MAX_UPLOAD_BYTES as u64 {
            return Err("ZIP entry exceeds the 50 MiB limit".to_owned());
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut output = std::fs::File::create(&destination).map_err(|error| error.to_string())?;
        std::io::copy(&mut entry, &mut output).map_err(|error| error.to_string())?;
        if entrypoint.is_none() && destination.file_name() == Some(OsStr::new("main.py")) {
            entrypoint = Some(destination);
        }
    }
    entrypoint.ok_or_else(|| "ZIP project must contain a main.py entrypoint".to_owned())
}

async fn terminal(
    State(config): State<GatewayConfig>,
    upgrade: WebSocketUpgrade,
) -> impl IntoResponse {
    upgrade.on_upgrade(move |socket| terminal_session(socket, config))
}

#[derive(Debug, Deserialize)]
struct TerminalRequest {
    #[serde(rename = "type")]
    message_type: String,
    protocol: Option<String>,
    command: Option<String>,
    path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum TerminalEvent {
    #[serde(rename = "stdout")]
    Stdout { data: String },
    #[serde(rename = "stderr")]
    Stderr { data: String },
}

async fn terminal_session(mut socket: WebSocket, config: GatewayConfig) {
    let _ = send_event(
        &mut socket,
        TerminalEvent::Stdout {
            data: "Tarvos v1.5.0 sandbox gateway ready\r\n".to_owned(),
        },
    )
    .await;
    let mut handshake_complete = false;
    while let Some(Ok(message)) = socket.next().await {
        let Message::Text(text) = message else {
            continue;
        };
        let request = match serde_json::from_str::<TerminalRequest>(&text) {
            Ok(request) => request,
            Err(error) => {
                let _ = send_event(
                    &mut socket,
                    TerminalEvent::Stderr {
                        data: format!("invalid request: {error}\r\n"),
                    },
                )
                .await;
                continue;
            }
        };
        if request.message_type == "handshake" {
            if request.protocol.as_deref() != Some("tarvos.v1") {
                let _ = send_event(
                    &mut socket,
                    TerminalEvent::Stderr {
                        data: "unsupported terminal protocol\r\n".to_owned(),
                    },
                )
                .await;
                continue;
            }
            handshake_complete = true;
            let _ = send_event(
                &mut socket,
                TerminalEvent::Stdout {
                    data: "handshake accepted: tarvos.v1\r\n".to_owned(),
                },
            )
            .await;
            continue;
        }
        if !handshake_complete {
            let _ = send_event(
                &mut socket,
                TerminalEvent::Stderr {
                    data: "handshake required before commands\r\n".to_owned(),
                },
            )
            .await;
            continue;
        }
        if request.message_type != "command" {
            let _ = send_event(
                &mut socket,
                TerminalEvent::Stderr {
                    data: "only command messages are accepted\r\n".to_owned(),
                },
            )
            .await;
            continue;
        }
        let Some(command) = request.command.as_deref().and_then(allowed_command) else {
            let _ = send_event(
                &mut socket,
                TerminalEvent::Stderr {
                    data: "command not allowed: use analyze, compile, or validate\r\n".to_owned(),
                },
            )
            .await;
            continue;
        };
        let requested_path = request.path.as_deref().unwrap_or(".");
        if requested_path != "." {
            let _ = send_event(
                &mut socket,
                TerminalEvent::Stderr {
                    data: "only the isolated project root is available\r\n".to_owned(),
                },
            )
            .await;
            continue;
        }
        stream_sandbox(&mut socket, &config, command).await;
    }
}

fn allowed_command(command: &str) -> Option<&'static str> {
    match command {
        "analyze" => Some("analyze"),
        "compile" => Some("compile"),
        "validate" => Some("validate"),
        _ => None,
    }
}

async fn stream_sandbox(socket: &mut WebSocket, config: &GatewayConfig, command: &str) {
    let mut child = match Command::new("docker")
        .args([
            "run",
            "--rm",
            "--read-only",
            "--network",
            "none",
            "--cap-drop",
            "ALL",
            "--security-opt",
            "no-new-privileges",
            "--pids-limit",
            "64",
            "--memory",
            "256m",
            "--cpus",
            "1",
            "--tmpfs",
            "/tmp:rw,noexec,nosuid,size=64m",
            "--volume",
            &format!("{}:/workspace:ro", config.workspace_root.to_string_lossy()),
            "--workdir",
            "/workspace",
            config.sandbox_image.as_str(),
            "tarvos",
            command,
            ".",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            let _ = send_event(
                socket,
                TerminalEvent::Stderr {
                    data: format!("sandbox unavailable: {error}\r\n"),
                },
            )
            .await;
            return;
        }
    };
    let Some(stdout) = child.stdout.take() else {
        return;
    };
    let Some(stderr) = child.stderr.take() else {
        return;
    };
    let (sender, mut receiver) = mpsc::channel::<TerminalEvent>(32);
    let out_sender = sender.clone();
    tokio::spawn(pipe_lines(stdout, out_sender, false));
    tokio::spawn(pipe_lines(stderr, sender.clone(), true));
    drop(sender);
    while let Some(event) = receiver.recv().await {
        if send_event(socket, event).await.is_err() {
            break;
        }
    }
    let _ = child.wait().await;
}

async fn pipe_lines<R: AsyncRead + Unpin>(
    reader: R,
    sender: mpsc::Sender<TerminalEvent>,
    is_stderr: bool,
) {
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let event = if is_stderr {
            TerminalEvent::Stderr {
                data: format!("{line}\r\n"),
            }
        } else {
            TerminalEvent::Stdout {
                data: format!("{line}\r\n"),
            }
        };
        if sender.send(event).await.is_err() {
            break;
        }
    }
}

async fn send_event(socket: &mut WebSocket, event: TerminalEvent) -> Result<(), axum::Error> {
    socket
        .send(Message::Text(
            serde_json::to_string(&event).expect("terminal event serialization cannot fail"),
        ))
        .await
}

fn cache_root() -> PathBuf {
    let home = env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir);
    home.join(".tarvos").join("cache").join("v1.5.0")
}

fn unique_path(root: &Path, prefix: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    root.join(format!("{prefix}-{}-{timestamp}", std::process::id()))
}

#[derive(Debug)]
struct GatewayError {
    status: StatusCode,
    message: String,
}

impl GatewayError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    fn bad_request(error: impl std::fmt::Display) -> Self {
        Self::new(StatusCode::BAD_REQUEST, error.to_string())
    }

    fn compiler(error: anyhow::Error) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, error.to_string())
    }

    fn internal(error: impl std::fmt::Display) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
    }

    fn toolchain(error: impl std::fmt::Display) -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE, error.to_string())
    }
}

impl IntoResponse for GatewayError {
    fn into_response(self) -> Response {
        (self.status, self.message).into_response()
    }
}
