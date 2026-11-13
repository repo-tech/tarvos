use anyhow::{Context, Result};
use colored::*;
use std::{env, fs, path::PathBuf, process::Command};

use super::{analyze_mode, benchmark_mode, clean_mode, doctor_mode, export_mode, init_mode};

pub fn install_command(_args: &[String]) -> Result<()> {
    println!("{}", "=== Tarvos Global Installer ===".cyan().bold());

    if env::var_os("TARVOS_SANDBOX").is_some() {
        println!(
            "{}",
            "Tarvos is already installed in this sandbox at /usr/local/bin/tarvos."
                .green()
                .bold()
        );
        return Ok(());
    }

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

    println!("{} Building Tarvos release binary...", "==>".green().bold());
    let status = Command::new("cargo")
        .args(["build", "--release", "--bin", "tarvos"])
        .current_dir(repo_root)
        .status()
        .with_context(|| "Failed to execute 'cargo build --release --bin tarvos'")?;

    if !status.success() {
        return Err(anyhow::anyhow!("Failed to build Tarvos release binary"));
    }

    let home = if cfg!(windows) {
        env::var_os("USERPROFILE")
    } else {
        env::var_os("HOME")
    }
    .map(PathBuf::from)
    .ok_or_else(|| anyhow::anyhow!("could not determine the current user's home directory"))?;
    let bin_dir = home.join(".tarvos").join("bin");
    let destination = bin_dir.join(format!("tarvos{}", std::env::consts::EXE_SUFFIX));
    let source = repo_root
        .join("target")
        .join("release")
        .join(format!("tarvos{}", std::env::consts::EXE_SUFFIX));
    fs::create_dir_all(&bin_dir)
        .with_context(|| format!("failed to create {}", bin_dir.display()))?;
    fs::copy(&source, &destination).with_context(|| {
        format!(
            "failed to install {} to {}. If Tarvos is running, close that process and retry.",
            source.display(),
            destination.display()
        )
    })?;

    println!(
        "{}",
        format!(
            "✓ Successfully installed Tarvos to {}!",
            destination.display()
        )
        .green()
        .bold()
    );
    println!(
        "Add {} to your user PATH, then open a new terminal.",
        bin_dir.display().to_string().yellow()
    );
    Ok(())
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
