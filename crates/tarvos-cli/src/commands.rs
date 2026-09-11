use anyhow::{Context, Result};
use colored::*;
use std::{env, path::PathBuf, process::Command};

use super::{analyze_mode, benchmark_mode, clean_mode, doctor_mode, export_mode, init_mode};

pub fn install_command(_args: &[String]) -> Result<()> {
    println!("{}", "=== Tarvos Global Installer ===".cyan().bold());

    // 1. Check if cargo is available
    let cargo_status = Command::new("cargo").arg("--version").output();
    if cargo_status.is_err() || !cargo_status.as_ref().unwrap().status.success() {
        eprintln!(
            "{}",
            "Error: 'cargo' is not found in your PATH. Please install Rust via https://rustup.rs"
                .red()
                .bold()
        );
        return Err(anyhow::anyhow!("Cargo not found"));
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(&manifest_dir);

    println!(
        "{} Building and installing tarvos binary...",
        "==>".green().bold()
    );
    let status = Command::new("cargo")
        .arg("install")
        .arg("--path")
        .arg(repo_root.join("crates").join("tarvos-cli"))
        .arg("--force")
        .current_dir(repo_root)
        .status()
        .with_context(|| "Failed to execute 'cargo install'")?;

    if status.success() {
        println!(
            "{}",
            "✓ Successfully installed Tarvos to ~/.cargo/bin/tarvos!"
                .green()
                .bold()
        );
        println!("Ensure {} is in your system PATH.", "~/.cargo/bin".yellow());
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "Failed to install Tarvos via cargo install"
        ))
    }
}

pub fn clean_command(args: &[String]) -> Result<()> {
    clean_mode(args)
}

pub fn export_command(args: &[String]) -> Result<()> {
    export_mode(args)
}

pub fn doctor_command(args: &[String]) -> Result<()> {
    doctor_mode(args)
}

pub fn analyze_command(args: &[String]) -> Result<()> {
    analyze_mode(args)
}

pub fn benchmark_command(args: &[String]) -> Result<()> {
    benchmark_mode(args)
}

pub fn init_command(args: &[String]) -> Result<()> {
    init_mode(args)
}

pub fn validate_command(_args: &[String]) -> Result<()> {
    println!("{}", "=== Tarvos Validation Suite ===".cyan().bold());
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(&manifest_dir);
    let validation_script = repo_root.join("validation").join("run_validation.py");

    if !validation_script.exists() {
        println!(
            "{} No validation/run_validation.py found. Running internal cargo tests...",
            "[i]".blue().bold()
        );
        let status = Command::new("cargo")
            .arg("test")
            .arg("--all")
            .current_dir(repo_root)
            .status()
            .with_context(|| "Failed to run cargo test --all")?;
        if status.success() {
            println!("{}", "✓ All internal test suites passed!".green().bold());
            return Ok(());
        } else {
            return Err(anyhow::anyhow!("Validation tests failed"));
        }
    }

    let status = Command::new("python")
        .arg(&validation_script)
        .current_dir(repo_root)
        .status()
        .with_context(|| "Failed to execute validation script")?;

    if status.success() {
        println!("{}", "✓ Validation Suite: 100% Passed!".green().bold());
        Ok(())
    } else {
        Err(anyhow::anyhow!("Validation suite encountered errors"))
    }
}
