mod ast_bridge;
#[path = "codegen.rs"]
mod tarvos_codegen;
mod types;

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

use ast_bridge::parse_python;
use tarvos_codegen::generate;

const CACHE_FORMAT_VERSION: &[u8] = b"tarvos-ruff-native-v5-runtime-warnings";

fn main() -> ExitCode {
    match dispatch() {
        Ok(status) => ExitCode::from(status as u8),
        Err(error) => {
            eprintln!("tarvos: {error}");
            ExitCode::FAILURE
        }
    }
}

fn dispatch() -> Result<i32, String> {
    let mut args = env::args_os();
    let _program = args.next();
    let command = args.next().ok_or_else(usage)?;
    let input = PathBuf::from(args.next().ok_or_else(usage)?);

    if args.next().is_some() {
        return Err(usage());
    }

    let executable = build_artifact(&input)?;
    match command.to_string_lossy().as_ref() {
        "run" => execute(&executable),
        "build" => {
            println!("{}", executable.display());
            Ok(0)
        }
        _ => Err(usage()),
    }
}

fn build_artifact(input: &Path) -> Result<PathBuf, String> {
    if input.extension().and_then(|value| value.to_str()) != Some("py") {
        return Err("input must be a .py file".into());
    }

    let source = fs::read_to_string(input)
        .map_err(|error| format!("cannot read {}: {error}", input.display()))?;
    let cache = input
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".tarvos")
        .join("cache");
    fs::create_dir_all(&cache)
        .map_err(|error| format!("cannot create {}: {error}", cache.display()))?;

    let key = format!("{:016x}", cache_key(&source));
    let generated = cache.join(format!("{key}.rs"));
    let executable = cache.join(format!("{key}.exe"));

    if executable.is_file() {
        return Ok(executable);
    }

    let module = parse_python(&source).map_err(|error| format!("Ruff parse failure: {error}"))?;
    if !module.diagnostics.is_empty() {
        return Err(module
            .diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect::<Vec<_>>()
            .join("\n"));
    }

    let rust = generate(&module).map_err(|error| error.message)?;
    fs::write(&generated, rust)
        .map_err(|error| format!("cannot write {}: {error}", generated.display()))?;

    let status = Command::new("rustc")
        .arg(&generated)
        .arg("--edition=2021")
        .args([
            "-C",
            "opt-level=z",
            "-C",
            "lto=fat",
            "-C",
            "codegen-units=1",
        ])
        .args(["-C", "panic=abort", "-C", "strip=symbols"])
        .arg("-o")
        .arg(&executable)
        .status()
        .map_err(|error| format!("failed to start rustc: {error}"))?;

    if !status.success() {
        return Err("rustc rejected generated source".into());
    }

    Ok(executable)
}

fn execute(executable: &Path) -> Result<i32, String> {
    let status = Command::new(executable)
        .status()
        .map_err(|error| format!("cannot execute {}: {error}", executable.display()))?;
    Ok(status.code().unwrap_or(1))
}

fn cache_key(source: &str) -> u64 {
    let mut state = 0xcbf29ce484222325_u64;
    for byte in CACHE_FORMAT_VERSION
        .iter()
        .chain(std::env::consts::ARCH.as_bytes())
        .chain(std::env::consts::OS.as_bytes())
        .chain(source.as_bytes())
    {
        state = (state ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
    }
    state
}

fn usage() -> String {
    "usage: tarvos <run|build> <script.py>".into()
}
