use anyhow::{Context, Result};
use std::{
    fs,
    io::Write,
    path::Path,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use tarvos_analysis::lower_module;
use tarvos_codegen_rust::RustCodegen;
use tarvos_optimizer::Optimizer;
use tarvos_parser::parse_python_ast;

pub mod api;

pub struct CompilePipeline;

const EMBEDDED_AST_EXPORTER: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../python/ast_export.py"
));

impl CompilePipeline {
    pub fn transpile_file(input_path: &Path) -> Result<String> {
        let source = fs::read_to_string(input_path)
            .with_context(|| format!("failed to read {}", input_path.display()))?;
        let ast_json = export_python_ast(&source)?;
        let module = parse_python_ast(&ast_json)?;
        let ir = lower_module(&module)?;
        let optimized = Optimizer::optimize(&ir)?;
        RustCodegen::generate(&optimized)
            .with_context(|| format!("failed to generate Rust for {}", input_path.display()))
    }

    pub fn transpile_file_embedded(input_path: &Path) -> Result<String> {
        let source = fs::read_to_string(input_path)
            .with_context(|| format!("failed to read {}", input_path.display()))?;
        let ast_json = export_python_ast(&source)?;
        let module = parse_python_ast(&ast_json)?;
        let ir = lower_module(&module)?;
        let optimized = Optimizer::optimize(&ir)?;
        RustCodegen::generate_embedded(&optimized).with_context(|| {
            format!(
                "failed to generate embedded Rust for {}",
                input_path.display()
            )
        })
    }

    pub fn write_rust_output(output_path: &Path, rust_source: &str) -> Result<()> {
        fs::write(output_path, rust_source)
            .with_context(|| format!("failed to write {}", output_path.display()))?;
        Ok(())
    }
}

pub fn export_python_ast(source: &str) -> Result<String> {
    let script_path = std::env::temp_dir().join(format!(
        "tarvos-ast-export-{}-{}.py",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock is before Unix epoch")?
            .as_nanos()
    ));
    fs::write(&script_path, EMBEDDED_AST_EXPORTER).with_context(|| {
        format!(
            "failed to materialize embedded AST exporter {}",
            script_path.display()
        )
    })?;

    let mut child = Command::new(find_python_command()?)
        .arg(&script_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .with_context(|| {
            format!(
                "failed to start Python AST exporter {}",
                script_path.display()
            )
        })?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(source.as_bytes())
            .with_context(|| "failed to send Python source to AST exporter")?;
    }

    let output = child
        .wait_with_output()
        .context("failed to read Python AST exporter output")?;
    let cleanup_result = fs::remove_file(&script_path);
    if let Err(error) = cleanup_result {
        eprintln!(
            "Tarvos AST exporter cleanup failed for {}: {error}",
            script_path.display()
        );
    }

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!(
            "Python AST export failed: {}",
            stderr.trim()
        ));
    }

    let json =
        String::from_utf8(output.stdout).context("AST exporter returned non-UTF-8 output")?;

    Ok(json.trim().to_string())
}

pub fn find_python_command() -> Result<String> {
    let candidates = [
        std::env::var("TARVOS_PYTHON")
            .ok()
            .map(|s| Path::new(&s).to_path_buf()),
        std::env::var("PYTHON")
            .ok()
            .map(|s| Path::new(&s).to_path_buf()),
        std::env::var("PYTHON3")
            .ok()
            .map(|s| Path::new(&s).to_path_buf()),
        Some(Path::new("python3").to_path_buf()),
        Some(Path::new("python").to_path_buf()),
        Some(Path::new("py").to_path_buf()),
    ];

    for candidate in candidates.into_iter().flatten() {
        let path = if candidate.is_absolute() {
            candidate
        } else {
            which_simple(&candidate.to_string_lossy())?.unwrap_or(candidate)
        };

        if !path.exists() {
            continue;
        }

        let canonical = path.canonicalize().unwrap_or(path.clone());
        if is_safe_executable(&canonical) {
            return Ok(canonical.to_string_lossy().to_string());
        }
    }

    Err(anyhow::anyhow!(
        "could not locate a safe Python interpreter in PATH or environment"
    ))
}

fn which_simple(name: &str) -> Result<Option<std::path::PathBuf>> {
    let mut cmd = Command::new("where");
    if cfg!(unix) {
        cmd = Command::new("which");
    }
    let out = cmd.arg(name).output();
    let out = match out {
        Ok(output) => output,
        Err(_) => return Ok(None),
    };
    if !out.status.success() {
        return Ok(None);
    }
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if stdout.is_empty() {
        return Ok(None);
    }
    let first = stdout.lines().next().unwrap_or_default();
    if first.is_empty() {
        return Ok(None);
    }
    Ok(Some(std::path::PathBuf::from(first)))
}

fn is_safe_executable(path: &Path) -> bool {
    let output = Command::new(path).arg("--version").output();
    output.is_ok() && output.unwrap().status.success()
}
