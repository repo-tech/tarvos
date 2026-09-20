//! Axum gateway for the Tarvos SaaS control plane.
//!
//! The gateway deliberately exposes a narrow protocol:
//! - `POST /api/v1/tarvos/compile` accepts one `.py` or `.zip` upload.
//! - `GET /api/v1/tarvos/terminal` upgrades to a raw WebSocket-backed shell
//!   inside a constrained, network-disabled Docker container.
//!
//! Terminal input is never executed by a host shell. It is forwarded only to
//! the non-root container process and the container is destroyed on disconnect.

use std::{
    env,
    ffi::OsStr,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    process::{Command as StdCommand, Stdio},
    time::{Duration, UNIX_EPOCH},
};

use axum::{
    body::Body,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Multipart, Query, State,
    },
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use tokio::{fs, process::Command};
use tower_http::cors::{Any, CorsLayer};

use crate::CompilePipeline;

const MAX_UPLOAD_BYTES: usize = 50 * 1024 * 1024;
const SANDBOX_IMAGE_ENV: &str = "TARVOS_SANDBOX_IMAGE";
const DEFAULT_SANDBOX_IMAGE: &str = "tarvos-sandbox:latest";
const SANDBOX_STARTUP_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Debug)]
pub struct GatewayConfig {
    pub cache_root: PathBuf,
    pub workspace_root: PathBuf,
    pub sandbox_image: String,
    pub docker_binary: String,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            cache_root: cache_root(),
            workspace_root: workspace_root(),
            sandbox_image: env::var(SANDBOX_IMAGE_ENV)
                .unwrap_or_else(|_| DEFAULT_SANDBOX_IMAGE.to_owned()),
            docker_binary: env::var("TARVOS_DOCKER_BIN").unwrap_or_else(|_| docker_binary()),
        }
    }
}

pub fn router(config: GatewayConfig) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(Any);

    Router::new()
        .route("/", get(health))
        .route("/health", get(health))
        .route("/api/v1/tarvos/sessions", post(create_session))
        .route("/api/v1/tarvos/compile", post(compile))
        .route("/api/v1/tarvos/terminal", get(terminal))
        .route(
            "/api/v1/tarvos/files",
            get(get_files).put(put_file).delete(delete_file),
        )
        .route("/api/v1/tarvos/files/mkdir", post(mkdir))
        .route("/api/v1/tarvos/files/rename", post(rename_file))
        .route("/api/v1/tarvos/files/raw", get(get_raw_file))
        .layer(cors)
        .with_state(config)
}

async fn health() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        r#"{"status":"operational","version":"1.0.0"}"#,
    )
}

async fn create_session(
    State(config): State<GatewayConfig>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let session_id: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(48)
        .map(char::from)
        .collect();
    let workspace = config
        .workspace_root
        .join("sessions")
        .join(&session_id)
        .join("workspace");
    fs::create_dir_all(&workspace)
        .await
        .map_err(GatewayError::internal)?;
    Ok(Json(serde_json::json!({ "sessionId": session_id })))
}

async fn compile(
    State(config): State<GatewayConfig>,
    headers: HeaderMap,
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

    let (session_root, cleanup) = session_job(&config, headers.get("x-tarvos-session-id"))?;
    let job = if cleanup {
        session_root.clone()
    } else {
        session_root.join("workspace")
    };
    fs::create_dir_all(&job)
        .await
        .map_err(GatewayError::internal)?;
    let result = compile_upload(&job, &filename, &bytes).await;
    if cleanup {
        if let Err(error) = fs::remove_dir_all(&job).await {
            eprintln!(
                "Tarvos gateway cleanup failed for {}: {error}",
                job.display()
            );
        }
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
        let source_name = Path::new(filename)
            .file_name()
            .filter(|name| name.to_string_lossy().to_ascii_lowercase().ends_with(".py"))
            .ok_or_else(|| GatewayError::bad_request("Python upload filename is invalid"))?;
        let path = job.join(source_name);
        fs::write(&path, bytes)
            .await
            .map_err(GatewayError::internal)?;
        if path.file_name() != Some(OsStr::new("main.py")) {
            fs::copy(&path, job.join("main.py"))
                .await
                .map_err(GatewayError::internal)?;
        }
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
    Query(query): Query<TerminalQuery>,
    upgrade: WebSocketUpgrade,
) -> impl IntoResponse {
    upgrade.on_upgrade(move |socket| terminal_session(socket, config, query))
}

#[derive(Debug, Deserialize)]
struct TerminalQuery {
    session_id: Option<String>,
}

async fn terminal_session(socket: WebSocket, config: GatewayConfig, query: TerminalQuery) {
    let session_header = query.session_id.as_deref().map(HeaderValue::from_str);
    let session_header = match session_header {
        Some(Ok(value)) => Some(value),
        Some(Err(_)) => {
            let (mut sender, _) = socket.split();
            let _ = sender
                .send(Message::Text("session id must be ASCII\r\n".to_owned()))
                .await;
            return;
        }
        None => None,
    };
    let (session_root, cleanup) = match session_job(&config, session_header.as_ref()) {
        Ok(job) => job,
        Err(error) => {
            let (mut sender, _) = socket.split();
            let _ = sender.send(Message::Text(error.message)).await;
            return;
        }
    };
    if let Err(error) = fs::create_dir_all(&session_root).await {
        let (mut sender, _) = socket.split();
        let _ = sender
            .send(Message::Text(format!(
                "sandbox directory error: {error}\r\n"
            )))
            .await;
        return;
    }

    let workspace = session_root.join("workspace");
    if let Err(error) = fs::create_dir_all(&workspace).await {
        let (mut sender, _) = socket.split();
        let _ = sender
            .send(Message::Text(format!("workspace error: {error}\r\n")))
            .await;
        if cleanup {
            let _ = fs::remove_dir_all(&session_root).await;
        }
        return;
    }

    let container_suffix = format!(
        "{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    let container_name = format!("tarvos-box-{}", container_suffix);

    let mut child = match StdCommand::new(&config.docker_binary)
        .args([
            "run",
            "--rm",
            "--name",
            &container_name,
            "--init",
            "--interactive",
            "--network=none",
            "--read-only",
            "--memory=2048m",
            "--cpus=4.0",
            "--cap-drop=ALL",
            "--security-opt=no-new-privileges",
            "--pids-limit=128",
            "--tmpfs=/tmp:rw,noexec,nosuid,size=256m",
            "--tmpfs=/home/tarvosuser/.tarvos:rw,exec,nosuid,size=512m",
            "--user",
            "tarvosuser",
            "--env",
            "TERM=xterm-256color",
            "--volume",
        ])
        .arg(format!(
            "{}:/home/tarvosuser/workspace",
            workspace.to_string_lossy()
        ))
        .args([
            "--workdir",
            "/home/tarvosuser/workspace",
            "--entrypoint",
            "/usr/bin/script",
            &config.sandbox_image,
            "-q",
            "-e",
            "-c",
            "printf 'code() { local f=${1:-.}; printf \"\\033]777;tarvos:open;%s\\007\" \"$f\"; }\\nexport PS1=\"tarvos@sandbox:\\w$ \"\\n' > /tmp/.bashrc && exec /bin/bash --rcfile /tmp/.bashrc -i",
            "/dev/null",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            let (mut sender, _) = socket.split();
            let _ = sender
                .send(Message::Text(format!("sandbox unavailable: {error}\r\n")))
                .await;
            if cleanup {
                let _ = fs::remove_dir_all(&session_root).await;
            }
            return;
        }
    };
    let Some(stdin) = child.stdin.take() else {
        let _ = child.kill();
        return;
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        return;
    };
    let Some(stderr) = child.stderr.take() else {
        let _ = child.kill();
        return;
    };

    let (mut sender, mut receiver) = socket.split();
    let (output_tx, mut output_rx) = tokio::sync::mpsc::channel::<Message>(32);
    let (input_tx, input_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let stdin_thread = std::thread::spawn(move || {
        let mut stdin = stdin;
        while let Ok(bytes) = input_rx.recv() {
            if stdin.write_all(&bytes).is_err() {
                break;
            }
            if stdin.flush().is_err() {
                break;
            }
        }
    });
    let stdout_tx = output_tx.clone();
    let stderr_tx = output_tx.clone();
    let stdout_thread = std::thread::spawn(move || {
        forward_process_output(stdout, stdout_tx);
    });
    let stderr_thread = std::thread::spawn(move || {
        forward_process_output(stderr, stderr_tx);
    });
    drop(output_tx);
    let startup_timeout = tokio::time::sleep(SANDBOX_STARTUP_TIMEOUT);
    tokio::pin!(startup_timeout);
    let mut sandbox_ready = false;

    loop {
        tokio::select! {
            message = receiver.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        if input_tx.send(text.as_bytes().to_vec()).is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Binary(bytes))) => {
                        if input_tx.send(bytes.to_vec()).is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => {}
                    Some(Err(_)) => break,
                }
            }
            output = output_rx.recv() => {
                match output {
                    Some(message) => {
                        sandbox_ready = true;
                        if sender.send(message).await.is_err() { break; }
                    }
                    None => {
                        if !sandbox_ready {
                            let _ = sender
                                .send(Message::Text(
                                    "sandbox exited before the shell became ready\r\n".to_owned(),
                                ))
                                .await;
                        }
                        break;
                    }
                }
            }
            _ = &mut startup_timeout, if !sandbox_ready => {
                let _ = sender
                    .send(Message::Text(
                        "sandbox startup timed out; Docker did not provide a shell\r\n"
                            .to_owned(),
                    ))
                    .await;
                break;
            }
        }
    }

    drop(input_tx);
    drop(stdin_thread);
    drop(stdout_thread);
    drop(stderr_thread);
    let docker_bin = config.docker_binary.clone();
    let cname = container_name.clone();
    let cleanup_task = tokio::task::spawn_blocking(move || {
        let _ = child.kill();
        let _ = child.wait();
        let _ = StdCommand::new(&docker_bin).args(["rm", "-f", &cname]).output();
    });
    let _ = tokio::time::timeout(Duration::from_secs(5), cleanup_task).await;
    if cleanup {
        let _ = fs::remove_dir_all(&session_root).await;
    }
}

fn forward_process_output<R: Read>(mut reader: R, sender: tokio::sync::mpsc::Sender<Message>) {
    let mut buffer = [0_u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(length) => {
                if sender
                    .blocking_send(Message::Binary(buffer[..length].to_vec().into()))
                    .is_err()
                {
                    break;
                }
            }
        }
    }
}

fn docker_binary() -> String {
    if cfg!(windows) {
        return env::var("ProgramFiles")
            .map(|program_files| {
                PathBuf::from(program_files)
                    .join("Docker")
                    .join("Docker")
                    .join("resources")
                    .join("bin")
                    .join("docker.exe")
                    .to_string_lossy()
                    .into_owned()
            })
            .unwrap_or_else(|_| "docker".to_owned());
    } else {
        return "docker".to_owned();
    }
}

#[derive(Debug, Deserialize)]
pub struct FileQuery {
    pub path: Option<String>,
    pub session_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: u64,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FileResponse {
    Directory {
        path: String,
        entries: Vec<FileEntry>,
    },
    File {
        name: String,
        path: String,
        content: String,
        size: u64,
    },
}

fn resolve_safe_path(workspace: &Path, user_path: Option<&str>) -> Result<PathBuf, GatewayError> {
    let path_str = user_path.unwrap_or("").trim();
    let normalized = path_str.trim_start_matches('/').trim_start_matches('\\');
    if normalized.is_empty() || normalized == "." {
        return Ok(workspace.to_path_buf());
    }
    let relative = Path::new(normalized);
    for component in relative.components() {
        match component {
            std::path::Component::ParentDir => {
                return Err(GatewayError::bad_request("path traversal using '..' is forbidden"));
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return Err(GatewayError::bad_request("absolute paths are forbidden"));
            }
            std::path::Component::Normal(_) | std::path::Component::CurDir => {}
        }
    }
    let full = workspace.join(relative);
    if !full.starts_with(workspace) {
        return Err(GatewayError::bad_request("path escapes workspace boundary"));
    }
    Ok(full)
}

async fn get_files(
    State(config): State<GatewayConfig>,
    headers: HeaderMap,
    Query(query): Query<FileQuery>,
) -> Result<Json<FileResponse>, GatewayError> {
    let session_header = headers.get("x-tarvos-session-id");
    let query_header = query.session_id.as_deref().map(HeaderValue::from_str);
    let header = session_header.or_else(|| match &query_header {
        Some(Ok(v)) => Some(v),
        _ => None,
    });
    let (session_root, _) = session_job(&config, header)?;
    let workspace = session_root.join("workspace");
    if !workspace.is_dir() {
        fs::create_dir_all(&workspace)
            .await
            .map_err(GatewayError::internal)?;
    }
    let target = resolve_safe_path(&workspace, query.path.as_deref())?;
    if !target.exists() {
        return Err(GatewayError::new(StatusCode::NOT_FOUND, "path not found"));
    }

    let relative_path = target
        .strip_prefix(&workspace)
        .unwrap_or(Path::new(""))
        .to_string_lossy()
        .replace('\\', "/");

    let metadata = fs::metadata(&target)
        .await
        .map_err(GatewayError::internal)?;
    if metadata.is_dir() {
        let mut read_dir = fs::read_dir(&target)
            .await
            .map_err(GatewayError::internal)?;
        let mut entries = Vec::new();
        while let Some(entry) = read_dir.next_entry().await.map_err(GatewayError::internal)? {
            let entry_path = entry.path();
            let rel = entry_path
                .strip_prefix(&workspace)
                .unwrap_or(&entry_path)
                .to_string_lossy()
                .replace('\\', "/");
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let entry_meta = entry.metadata().await.map_err(GatewayError::internal)?;
            let modified = entry_meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            entries.push(FileEntry {
                name: file_name,
                path: rel,
                is_dir: entry_meta.is_dir(),
                size: entry_meta.len(),
                modified,
            });
        }
        entries.sort_by(|a, b| {
            if a.is_dir == b.is_dir {
                a.name.to_lowercase().cmp(&b.name.to_lowercase())
            } else if a.is_dir {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            }
        });
        Ok(Json(FileResponse::Directory {
            path: relative_path,
            entries,
        }))
    } else {
        if metadata.len() > 10 * 1024 * 1024 {
            return Err(GatewayError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "file too large to preview in editor",
            ));
        }
        let content = match fs::read(&target).await {
            Ok(bytes) => {
                // Check if file is valid UTF-8 text or binary
                match String::from_utf8(bytes) {
                    Ok(text) => text,
                    Err(e) => {
                        let lossy = String::from_utf8_lossy(e.as_bytes()).to_string();
                        // If it contains lots of null bytes, treat as binary
                        if lossy.contains('\0') {
                            format!(
                                "// [Tarvos Binary File Viewer]\n// File '{}' is a binary artifact (size: {} bytes).\n// Binary files cannot be edited as raw text in the source editor.\n",
                                target.file_name().unwrap_or_default().to_string_lossy(),
                                metadata.len()
                            )
                        } else {
                            lossy
                        }
                    }
                }
            }
            Err(e) => return Err(GatewayError::internal(e)),
        };
        let name = target
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        Ok(Json(FileResponse::File {
            name,
            path: relative_path,
            content,
            size: metadata.len(),
        }))
    }
}

/// Serve a file as raw bytes with the correct MIME type.
/// Used by the frontend to display images, audio, and video in the editor pane.
async fn get_raw_file(
    State(config): State<GatewayConfig>,
    headers: HeaderMap,
    Query(query): Query<FileQuery>,
) -> Result<Response, GatewayError> {
    let session_header = headers.get("x-tarvos-session-id");
    let query_header = query.session_id.as_deref().map(HeaderValue::from_str);
    let header = session_header.or_else(|| match &query_header {
        Some(Ok(v)) => Some(v),
        _ => None,
    });
    let (session_root, _) = session_job(&config, header)?;
    let workspace = session_root.join("workspace");
    let target = resolve_safe_path(&workspace, query.path.as_deref())?;
    if !target.exists() {
        return Err(GatewayError::new(StatusCode::NOT_FOUND, "path not found"));
    }
    let metadata = fs::metadata(&target).await.map_err(GatewayError::internal)?;
    if metadata.is_dir() {
        return Err(GatewayError::bad_request("path is a directory"));
    }
    if metadata.len() > 100 * 1024 * 1024 {
        return Err(GatewayError::new(StatusCode::PAYLOAD_TOO_LARGE, "file too large to serve"));
    }

    let bytes = fs::read(&target).await.map_err(GatewayError::internal)?;
    let ext = target
        .extension()
        .and_then(OsStr::to_str)
        .unwrap_or("")
        .to_lowercase();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "aac" => "audio/aac",
        "m4a" => "audio/mp4",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        "mov" => "video/quicktime",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    };

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CONTENT_LENGTH, bytes.len().to_string())
        .header(header::CACHE_CONTROL, "no-cache")
        .header("Access-Control-Allow-Origin", "*")
        .body(Body::from(bytes))
        .unwrap())
}

async fn put_file(
    State(config): State<GatewayConfig>,
    headers: HeaderMap,
    Query(query): Query<FileQuery>,
    body: axum::body::Bytes,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let session_header = headers.get("x-tarvos-session-id");
    let query_header = query.session_id.as_deref().map(HeaderValue::from_str);
    let header = session_header.or_else(|| match &query_header {
        Some(Ok(v)) => Some(v),
        _ => None,
    });
    let (session_root, _) = session_job(&config, header)?;
    let workspace = session_root.join("workspace");
    fs::create_dir_all(&workspace)
        .await
        .map_err(GatewayError::internal)?;
    let path_arg = query
        .path
        .as_deref()
        .ok_or_else(|| GatewayError::bad_request("missing 'path' query parameter"))?;
    if path_arg.trim().is_empty() || path_arg == "." {
        return Err(GatewayError::bad_request(
            "cannot write to root workspace directly as a file",
        ));
    }
    let target = resolve_safe_path(&workspace, Some(path_arg))?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(GatewayError::internal)?;
    }
    let content_bytes = if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&body) {
        if let Some(text) = val.get("content").and_then(|v| v.as_str()) {
            text.as_bytes().to_vec()
        } else {
            body.to_vec()
        }
    } else {
        body.to_vec()
    };
    fs::write(&target, &content_bytes)
        .await
        .map_err(GatewayError::internal)?;
    let rel = target
        .strip_prefix(&workspace)
        .unwrap_or(&target)
        .to_string_lossy()
        .replace('\\', "/");
    Ok(Json(serde_json::json!({
        "status": "ok",
        "path": rel,
        "size": content_bytes.len(),
    })))
}

async fn delete_file(
    State(config): State<GatewayConfig>,
    headers: HeaderMap,
    Query(query): Query<FileQuery>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let session_header = headers.get("x-tarvos-session-id");
    let query_header = query.session_id.as_deref().map(HeaderValue::from_str);
    let header = session_header.or_else(|| match &query_header {
        Some(Ok(v)) => Some(v),
        _ => None,
    });
    let (session_root, _) = session_job(&config, header)?;
    let workspace = session_root.join("workspace");
    let path_arg = query
        .path
        .as_deref()
        .ok_or_else(|| GatewayError::bad_request("missing 'path' query parameter"))?;
    let target = resolve_safe_path(&workspace, Some(path_arg))?;
    if target == workspace {
        return Err(GatewayError::bad_request("cannot delete root workspace"));
    }
    if !target.exists() {
        return Err(GatewayError::new(
            StatusCode::NOT_FOUND,
            "file or directory does not exist",
        ));
    }
    let meta = fs::metadata(&target)
        .await
        .map_err(GatewayError::internal)?;
    if meta.is_dir() {
        fs::remove_dir_all(&target)
            .await
            .map_err(GatewayError::internal)?;
    } else {
        fs::remove_file(&target)
            .await
            .map_err(GatewayError::internal)?;
    }
    Ok(Json(serde_json::json!({ "status": "ok" })))
}

async fn mkdir(
    State(config): State<GatewayConfig>,
    headers: HeaderMap,
    Query(query): Query<FileQuery>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let session_header = headers.get("x-tarvos-session-id");
    let query_header = query.session_id.as_deref().map(HeaderValue::from_str);
    let header = session_header.or_else(|| match &query_header {
        Some(Ok(v)) => Some(v),
        _ => None,
    });
    let (session_root, _) = session_job(&config, header)?;
    let workspace = session_root.join("workspace");
    let path_arg = query
        .path
        .as_deref()
        .ok_or_else(|| GatewayError::bad_request("missing 'path' query parameter"))?;
    let target = resolve_safe_path(&workspace, Some(path_arg))?;
    fs::create_dir_all(&target)
        .await
        .map_err(GatewayError::internal)?;
    Ok(Json(serde_json::json!({ "status": "ok" })))
}


#[derive(Debug, Deserialize)]
struct RenameQuery {
    from: String,
    to: String,
}

async fn rename_file(
    State(config): State<GatewayConfig>,
    headers: HeaderMap,
    Query(query): Query<RenameQuery>,
) -> Result<impl IntoResponse, GatewayError> {
    let header = headers.get("x-tarvos-session-id");
    let (session_root, _) = session_job(&config, header)?;
    let workspace = session_root.join("workspace");
    let src = resolve_safe_path(&workspace, Some(&query.from))?;
    let dst = resolve_safe_path(&workspace, Some(&query.to))?;
    if !src.exists() {
        return Err(GatewayError::bad_request("source path does not exist"));
    }
    if dst.exists() {
        return Err(GatewayError::bad_request("destination path already exists"));
    }
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).await.map_err(GatewayError::internal)?;
    }
    fs::rename(&src, &dst).await.map_err(GatewayError::internal)?;
    Ok((StatusCode::OK, Json(serde_json::json!({ "renamed": true }))))
}

fn session_job(
    config: &GatewayConfig,
    session_header: Option<&HeaderValue>,
) -> Result<(PathBuf, bool), GatewayError> {
    let Some(header) = session_header else {
        return Err(GatewayError::bad_request(
            "a server-issued Tarvos session id is required",
        ));
    };
    let session_id = header
        .to_str()
        .map_err(|_| GatewayError::bad_request("session id must be ASCII"))?;
    if !valid_session_id(session_id) {
        return Err(GatewayError::bad_request("session id is invalid"));
    }
    let session_root = config.workspace_root.join("sessions").join(session_id);
    if !session_root.is_dir() {
        return Err(GatewayError::bad_request(
            "session id is unknown or has expired; create a new Tarvos session",
        ));
    }
    Ok((session_root, false))
}

fn valid_session_id(value: &str) -> bool {
    (16..=128).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn cache_root() -> PathBuf {
    let home = env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir);
    home.join(".tarvos").join("cache").join("v1.0.0")
}

fn workspace_root() -> PathBuf {
    let home = env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir);
    home.join(".tarvos").join("workspaces").join("v1.0.0")
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
