use anyhow::{Context, Result};
use std::{
    env,
    fs,
    path::PathBuf,
    process::{Command, Stdio},
};

mod ast;
mod codegen;
mod ir;

use ast::parser::parse_python_ast;
use codegen::rust::RustCodegen;
use ir::lower::lower_module;

fn main() -> Result<()> {
    let input = env::args()
        .nth(1)
        .context("usage: tarvos <file.py>")?;

    let source = fs::read_to_string(&input)
        .with_context(|| format!("failed to read {input}"))?;

    let ast_json = export_ast(&source)?;
    let module = parse_python_ast(&ast_json)?;

    let ir = lower_module(&module)?;
    let rust_source = RustCodegen::generate(&ir)?;

    println!("=== Generated Rust ===");
    println!("{rust_source}");

    Ok(())
}

fn export_ast(source: &str) -> Result<String> {
    let exporter = PathBuf::from("python/ast_export.py");

    let mut child = Command::new("python3")
        .arg(exporter)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to start Python AST exporter")?;

    {
        use std::io::Write;

        child
            .stdin
            .as_mut()
            .context("failed to open Python stdin")?
            .write_all(source.as_bytes())?;
    }

    let output = child
        .wait_with_output()
        .context("Python AST exporter failed")?;

    if !output.status.success() {
        anyhow::bail!(
            "Python AST exporter error:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}