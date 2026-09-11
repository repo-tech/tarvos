use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::{
    env, fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

mod commands;
use commands::{
    analyze_command, benchmark_command, clean_command, doctor_command, export_command,
    init_command, install_command, validate_command,
};
use tarvos_analysis::analyze_module;
use tarvos_analysis::native_detector::{ModuleReport, NativePlan, NativeSubsetDetector};
use tarvos_analysis::native_specialization::{
    specialize_module, wire_specialization_runtime, SpecializedLoop,
};
use tarvos_core::{
    export_python_ast as core_export_python_ast, find_python_command as core_find_python_command,
    CompilePipeline,
};
use tarvos_parser::parse_python_ast;

/// Tarvos — Ultra-fast Python to Native Rust Transpiler & Compiler
#[derive(Parser, Debug)]
#[command(name = "tarvos")]
#[command(author = "Himanshu & Repo-Tech Team")]
#[command(version = "1.2.0")]
#[command(about = "Transpiles and compiles Python code to native high-performance Rust executables", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Direct input Python file when no subcommand is specified
    #[arg(value_name = "INPUT.py")]
    input: Option<PathBuf>,

    /// Output path when running in direct mode
    #[arg(short, long, value_name = "OUTPUT")]
    output: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Compile Python code to optimized Rust source or binary
    Compile {
        /// Input Python file (.py)
        #[arg(value_name = "FILE.py")]
        input: PathBuf,

        /// Output file path (defaults to output.rs or output.exe)
        #[arg(short, long, value_name = "PATH")]
        output: Option<PathBuf>,

        /// Target format: 'rust' (source) or 'exe' (native binary)
        #[arg(short, long, default_value = "rust")]
        format: String,

        /// Generate only Rust source without invoking native rustc compiler
        #[arg(long)]
        source_only: bool,
    },

    /// Build a native binary executable directly from Python
    Build {
        /// Input Python file (.py)
        #[arg(value_name = "FILE.py")]
        input: PathBuf,

        /// Output executable path (defaults to tarvos_app.exe)
        #[arg(short, long, value_name = "PATH")]
        output: Option<PathBuf>,

        /// Emit only Rust source without building executable
        #[arg(long)]
        source_only: bool,
    },

    /// Transpile, build, and run a Python file in one seamless step
    Run {
        /// Input Python file (.py)
        #[arg(value_name = "FILE.py")]
        input: PathBuf,

        /// Fall back to the CPython runtime if native compilation is unsupported
        #[arg(long)]
        python_fallback: bool,

        /// Arguments to pass to the executed binary
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Execute any Python program through the local Python runtime
    Python {
        /// Input Python file (.py)
        #[arg(value_name = "FILE.py")]
        input: PathBuf,

        /// Arguments to pass to the Python program
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Run environment diagnostics and check toolchain dependencies (Python, Rust, Cargo, Linker)
    Doctor,

    /// Static analysis and complexity profiling of a Python module
    Analyze {
        /// Input Python file (.py)
        #[arg(value_name = "FILE.py")]
        input: PathBuf,

        /// Report functions that are candidates for native hot-path acceleration
        #[arg(long)]
        hot_functions: bool,
    },

    /// Scan a Python project for library-backed hot loops and native plans
    Scan {
        /// Project directory or Python file to inspect
        input: PathBuf,
    },

    /// Benchmark Python vs Tarvos (Native Rust) with fairness metrics
    Benchmark {
        /// Input Python file (.py)
        #[arg(value_name = "FILE.py")]
        input: PathBuf,

        /// Optional reference Rust implementation
        #[arg(value_name = "REF.rs")]
        reference: Option<PathBuf>,
    },

    /// Initialize a new Tarvos project template
    Init {
        /// Project directory name
        #[arg(default_value = "tarvos-app")]
        name: String,
    },

    /// Export Python code as a complete standalone Cargo Rust project
    Export {
        /// Input Python file (.py)
        #[arg(value_name = "FILE.py")]
        input: PathBuf,

        /// Destination directory for the exported Cargo project
        #[arg(value_name = "DIR", default_value = "tarvos-export")]
        output_dir: PathBuf,
    },

    /// Clean build artifacts, temporary cache files, and intermediate outputs
    Clean,

    /// Install Tarvos system-wide into your PATH
    Install,

    /// Validate Tarvos output against reference test suite
    Validate,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Compile {
            input,
            output,
            format,
            source_only,
        }) => {
            let mut args = vec![input.to_string_lossy().to_string()];
            if format != "rust" {
                args.push("--format".to_string());
                args.push(format);
            }
            if let Some(o) = output {
                args.push("--output".to_string());
                args.push(o.to_string_lossy().to_string());
            }
            if source_only {
                args.push("--source-only".to_string());
            }
            compile_mode(&args)
        }
        Some(Commands::Build {
            input,
            output,
            source_only,
        }) => {
            let mut args = vec![input.to_string_lossy().to_string()];
            if let Some(o) = output {
                args.push("--output".to_string());
                args.push(o.to_string_lossy().to_string());
            }
            if source_only {
                args.push("--source-only".to_string());
            }
            build_mode(&args)
        }
        Some(Commands::Run {
            input,
            python_fallback,
            args: extra,
        }) => {
            let mut args = vec![input.to_string_lossy().to_string()];
            args.extend(extra);
            if python_fallback {
                hybrid_run_mode(&args)
            } else {
                run_mode(&args)
            }
        }
        Some(Commands::Python { input, args: extra }) => {
            let mut args = vec![input.to_string_lossy().to_string()];
            args.extend(extra);
            python_mode(&args)
        }
        Some(Commands::Doctor) => doctor_command(&[]),
        Some(Commands::Analyze {
            input,
            hot_functions,
        }) => {
            let args = vec![input.to_string_lossy().to_string()];
            if hot_functions {
                analyze_hot_functions(&args)
            } else {
                analyze_command(&args)
            }
        }
        Some(Commands::Scan { input }) => scan_project_mode(&input),
        Some(Commands::Benchmark { input, reference }) => {
            let mut args = vec![input.to_string_lossy().to_string()];
            if let Some(ref_path) = reference {
                args.push(ref_path.to_string_lossy().to_string());
            }
            benchmark_command(&args)
        }
        Some(Commands::Init { name }) => init_command(&[name]),
        Some(Commands::Export { input, output_dir }) => {
            let args = vec![
                input.to_string_lossy().to_string(),
                output_dir.to_string_lossy().to_string(),
            ];
            export_command(&args)
        }
        Some(Commands::Clean) => clean_command(&[]),
        Some(Commands::Install) => install_command(&[]),
        Some(Commands::Validate) => validate_command(&[]),
        None => {
            if let Some(input) = cli.input {
                let mut args = vec![input.to_string_lossy().to_string()];
                if let Some(o) = cli.output {
                    args.push(o.to_string_lossy().to_string());
                }
                compile_mode(&args)
            } else {
                print_help();
                Ok(())
            }
        }
    }
}

pub(crate) fn compile_mode(args: &[String]) -> Result<()> {
    let mut output_file = "output.rs".to_string();
    let mut format = "rust".to_string();
    let mut source_only = false;
    let mut iter = args.iter();
    let input_file = iter
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: tarvos <file.py> [output.rs]"))?;

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--format" | "-f" => {
                format = iter.next().cloned().unwrap_or_else(|| "rust".to_string());
            }
            "--output" | "-o" => {
                output_file = iter.next().cloned().unwrap_or_else(|| output_file.clone());
            }
            "--source-only" => {
                source_only = true;
            }
            _ => {
                if output_file == "output.rs" {
                    output_file = arg.clone();
                }
            }
        }
    }

    let working_dir = env::current_dir()?;
    let input_path = secure_input_path(input_file, &working_dir)?;
    let output_path = secure_output_path(&output_file, &working_dir)?;
    let rust_source = transpile_python_to_rust(&input_path)?;

    println!("=== Tarvos Compiler ===\n");
    println!("Input: {}", input_path.display());
    println!("  AST Visitor: re-enabled via ast.NodeVisitor");
    println!("  [1/6] Exporting Python AST...");
    println!("  [2/6] Parsing AST...");
    println!("  [3/6] Type checking and lowering to IR...");
    println!("  [4/6] Running optimizations...");
    println!("  [5/6] Generating Rust code...");
    println!("  [6/6] Writing output...");

    write_rust_output(&output_path, &rust_source)?;

    println!("\n=== Compilation Successful ===\n");
    println!("Output: {}", output_path.display());
    println!("Mode: source-only pipeline (Rust toolchain not required)");

    if !source_only
        && (format == "exe" || output_path.extension().and_then(|s| s.to_str()) == Some("exe"))
    {
        compile_rust_binary(&output_path, &rust_source)?;
    } else {
        println!("\n=== Generated Rust Code ===\n{}", rust_source);
    }

    Ok(())
}

pub(crate) fn build_mode(args: &[String]) -> Result<()> {
    let mut output_file = "tarvos_app.exe".to_string();
    let mut source_only = false;
    let mut iter = args.iter();
    let input_file = iter
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: tarvos build <file.py> [output.exe]"))?;
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--output" | "-o" => {
                output_file = iter.next().cloned().unwrap_or_else(|| output_file.clone());
            }
            "--source-only" => {
                source_only = true;
            }
            _ => {
                if output_file == "tarvos_app.exe" {
                    output_file = arg.clone();
                }
            }
        }
    }

    let working_dir = env::current_dir()?;
    let input_path = secure_input_path(input_file, &working_dir)?;
    let output_path = secure_output_path(&output_file, &working_dir)?;
    let rust_source = transpile_python_to_rust(&input_path)?;
    let rust_output = output_path.with_extension("rs");

    write_rust_output(&rust_output, &rust_source)?;

    if source_only {
        println!("Source-only build complete: {}", rust_output.display());
        println!("Rust is not required for source generation. Use `tarvos build ... --source-only` to emit Rust without native EXE compilation.");
        return Ok(());
    }

    match compile_rust_binary(&output_path, &rust_source) {
        Ok(()) => {
            println!("Build complete: {}", output_path.display());
            Ok(())
        }
        Err(err) => {
            eprintln!(
                "Rust native build unavailable; source generation succeeded at {}",
                rust_output.display()
            );
            eprintln!("Install Rust or use `tarvos compile <file.py> --source-only` to keep source mode working without a Rust toolchain.");
            Err(err)
        }
    }
}

pub(crate) fn run_mode(args: &[String]) -> Result<()> {
    let (exe_path, runtime_args) = prepare_native_run(args)?;
    execute_native_run(&exe_path, &runtime_args)
}

fn prepare_native_run(args: &[String]) -> Result<(PathBuf, Vec<String>)> {
    let input_file = args
        .first()
        .ok_or_else(|| anyhow::anyhow!("usage: tarvos run <file.py> [args...]"))?;
    let working_dir = env::current_dir()?;
    let input_path = secure_input_path(input_file, &working_dir)?;
    let rust_source = transpile_python_to_rust(&input_path)?;
    let temp_dir = working_dir.join(".build-tmp");
    fs::create_dir_all(&temp_dir).ok();
    let exe_path = temp_dir.join(format!(
        "{}{}",
        input_path.file_stem().unwrap_or_default().to_string_lossy(),
        if cfg!(windows) { ".exe" } else { "" }
    ));
    fs::write(temp_dir.join("runtime_run.rs"), &rust_source)?;
    compile_rust_binary(&exe_path, &rust_source)?;
    Ok((exe_path, args[1..].to_vec()))
}

fn execute_native_run(exe_path: &Path, args: &[String]) -> Result<()> {
    let status = Command::new(exe_path)
        .args(args)
        .status()
        .with_context(|| format!("failed to execute {}", exe_path.display()))?;
    if !status.success() {
        return Err(anyhow::anyhow!(
            "native program exited with status {}",
            status
        ));
    }
    Ok(())
}

fn scan_project_mode(input: &Path) -> Result<()> {
    let root = env::current_dir()?;
    let requested = if input.is_absolute() {
        input.to_path_buf()
    } else {
        root.join(input)
    };
    let input = requested
        .canonicalize()
        .with_context(|| format!("scan path does not exist: {}", requested.display()))?;
    let canonical_root = root.canonicalize()?;
    if !input.starts_with(&canonical_root) {
        return Err(anyhow::anyhow!(
            "scan path must remain inside the current project root: {}",
            canonical_root.display()
        ));
    }
    if !input.is_file() && !input.is_dir() {
        return Err(anyhow::anyhow!(
            "scan path is not a file or directory: {}",
            input.display()
        ));
    }
    let files = if input.is_dir() {
        collect_python_files(&input)?
    } else {
        vec![input.clone()]
    };
    if files.is_empty() {
        return Err(anyhow::anyhow!(
            "no Python files found under {}",
            input.display()
        ));
    }
    let detector = NativeSubsetDetector::default();
    let mut total_loops = 0;
    println!("=== Tarvos Native Subset & Library Loop Scan ===");
    println!("Project: {}", input.display());
    for file in &files {
        let source = match fs::read_to_string(file) {
            Ok(source) => source,
            Err(error) => {
                println!("\nFile: {}\n  Scan error: {}", file.display(), error);
                continue;
            }
        };
        let ast_json = match export_python_ast(&source) {
            Ok(ast_json) => ast_json,
            Err(error) => {
                println!("\nFile: {}\n  Scan error: {}", file.display(), error);
                continue;
            }
        };
        let module = match parse_python_ast(&ast_json) {
            Ok(module) => module,
            Err(error) => {
                println!("\nFile: {}\n  Scan error: {}", file.display(), error);
                continue;
            }
        };
        let report = detector.analyze(&module);
        total_loops += report.loops.len();
        print_scan_report(file, &report);
        print_specialization_report(&module, &report);
    }

    fn print_specialization_report(module: &tarvos_ast::Module, report: &ModuleReport) {
        let specialized = specialize_module(module, report);
        if specialized.buffers.is_empty() && specialized.loops.is_empty() {
            return;
        }
        println!("  Runtime specialization:");
        for buffer in specialized.buffers {
            println!(
                "    Buffer {}: Vec<{}>, capacity={}, source={}",
                buffer.name,
                buffer.dtype.rust_type(),
                buffer.capacity_expression,
                buffer.source
            );
        }
        for loop_plan in specialized.loops {
            match loop_plan {
                SpecializedLoop::Numpy { ordinal, rust, .. } => {
                    println!(
                        "    NumPy loop #{} native rewrite:\n{}",
                        ordinal,
                        indent_block(&rust)
                    );
                }
                SpecializedLoop::Pandas {
                    ordinal,
                    columns,
                    rust,
                } => {
                    println!(
                        "    Pandas loop #{} schema={:?} native rewrite:\n{}",
                        ordinal,
                        columns,
                        indent_block(&rust)
                    );
                }
                SpecializedLoop::Fallback { ordinal, reason } => {
                    println!("    Loop #{} fallback boundary: {}", ordinal, reason);
                }
            }
        }
    }

    fn indent_block(block: &str) -> String {
        block
            .lines()
            .map(|line| format!("      {}", line))
            .collect::<Vec<_>>()
            .join("\n")
    }
    println!(
        "Scanned {} file(s), detected {} loop(s).",
        files.len(),
        total_loops
    );
    Ok(())
}

fn collect_python_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(&path)
            .with_context(|| format!("failed to read directory {}", path.display()))?
        {
            let child = entry?.path();
            if child.is_dir() {
                if child.file_name().and_then(|n| n.to_str()) != Some("__pycache__") {
                    pending.push(child);
                }
            } else if child.extension().and_then(|ext| ext.to_str()) == Some("py") {
                files.push(child);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn print_scan_report(file: &Path, report: &ModuleReport) {
    println!("\nFile: {}", file.display());
    println!("  NumPy bindings: {:?}", report.imports.numpy_aliases);
    println!("  Pandas bindings: {:?}", report.imports.pandas_aliases);
    if report.loops.is_empty() {
        println!("  Hot loops: none");
        return;
    }
    for loop_info in &report.loops {
        let library = loop_info
            .library
            .map(|kind| format!("{:?}", kind))
            .unwrap_or_else(|| "none".into());
        let plan = match &loop_info.plan {
            NativePlan::VecF64 { reason } => format!("Vec<f64>: {}", reason),
            NativePlan::NdArray { reason } => format!("ndarray/contiguous: {}", reason),
            NativePlan::Iterator {
                parallelizable,
                reason,
            } => format!("iterator (parallelizable={}): {}", parallelizable, reason),
            NativePlan::PythonFallback { reason } => format!("CPython fallback: {}", reason),
        };
        println!(
            "  Loop #{} ({}, library={}):",
            loop_info.ordinal, loop_info.kind, library
        );
        println!(
            "    Signals: {}",
            if loop_info.signals.is_empty() {
                "none".into()
            } else {
                loop_info.signals.join(", ")
            }
        );
        println!("    Plan: {}", plan);
    }
}

pub(crate) fn hybrid_run_mode(args: &[String]) -> Result<()> {
    let (exe_path, runtime_args) = match prepare_native_run(args) {
        Ok(result) => result,
        Err(native_error) => {
            eprintln!("Native subset compilation unavailable: {}", native_error);
            eprintln!("Falling back to CPython compatibility runtime (--python-fallback).");
            return python_mode(args);
        }
    };
    execute_native_run(&exe_path, &runtime_args)
}

fn analyze_hot_functions(args: &[String]) -> Result<()> {
    let input_file = args
        .first()
        .ok_or_else(|| anyhow::anyhow!("usage: tarvos analyze <file.py> --hot-functions"))?;
    let working_dir = env::current_dir()?;
    let input_path = secure_input_path(input_file, &working_dir)?;
    let source = fs::read_to_string(&input_path)
        .with_context(|| format!("failed to read {}", input_path.display()))?;
    let ast_json = export_python_ast(&source)?;
    let module = parse_python_ast(&ast_json)?;
    let stats = analyze_module(&module);

    println!("=== Tarvos Hot Function Analysis ===");
    println!("Input: {}", input_path.display());
    if stats.functions == 0 {
        println!("Candidates: none (no user-defined functions found)");
        return Ok(());
    }

    if stats.loops == 0 && stats.binary_ops == 0 {
        println!("Candidates: functions found, but no loop/arithmetic hotspot detected");
        println!("Recommendation: keep CPython execution until profiling shows a hot path.");
    } else {
        println!("Candidates: {} user-defined function(s)", stats.functions);
        println!("Estimated module cost: {}", stats.estimated_cost);
        println!("Recommendation: inspect loop-heavy typed functions for native extraction.");
        println!(
            "Native extraction status: analysis-only foundation; function ABI bridge is next."
        );
    }
    Ok(())
}

pub(crate) fn python_mode(args: &[String]) -> Result<()> {
    let input_file = args
        .first()
        .ok_or_else(|| anyhow::anyhow!("usage: tarvos python <file.py> [args...]"))?;
    let working_dir = env::current_dir()?;
    let input_path = secure_input_path(input_file, &working_dir)?;
    let python = find_python_command()?;
    let status = Command::new(&python)
        .arg(&input_path)
        .args(&args[1..])
        .current_dir(&working_dir)
        .status()
        .with_context(|| format!("failed to execute Python program {}", input_path.display()))?;
    if !status.success() {
        return Err(anyhow::anyhow!(
            "Python compatibility runtime exited with status {}",
            status
        ));
    }
    Ok(())
}

pub(crate) fn doctor_mode(_args: &[String]) -> Result<()> {
    println!("=== Tarvos System Diagnostics & Health Check ===\n");

    let python_cmd = find_python_command();
    match &python_cmd {
        Ok(cmd) => {
            let ver = Command::new(cmd).arg("--version").output();
            let ver_str = ver
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_else(|_| "Python 3.x".to_string());
            println!("  [✓] Python Interpreter: {} ({})", cmd, ver_str);
        }
        Err(_) => {
            println!("  [✗] Python Interpreter: Not found in PATH or environment");
        }
    }

    let rustc_cmd = find_rustc_command();
    match &rustc_cmd {
        Some(cmd) => {
            let ver = Command::new(cmd).arg("--version").output();
            let ver_str = ver
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_else(|_| "rustc stable".to_string());
            println!(
                "  [✓] Rust Compiler (rustc): {} ({})",
                cmd.display(),
                ver_str
            );
        }
        None => {
            println!("  [!] Rust Compiler (rustc): Not detected (source-only mode available, auto-bootstrap via rustup supported)");
        }
    }

    // Check cargo
    let cargo_status = Command::new("cargo").arg("--version").output();
    if let Ok(c) = cargo_status {
        if c.status.success() {
            println!(
                "  [✓] Cargo Package Manager: {}",
                String::from_utf8_lossy(&c.stdout).trim()
            );
        }
    }

    println!(
        "  [✓] Current Working Directory: {}",
        env::current_dir()?.display()
    );
    println!("  [✓] Transpiler Architecture: Native AST Visitor + IR Lowering + Dead-Code & Induction Optimizer");
    println!("\nDiagnostics complete: All core capabilities verified.\n");
    Ok(())
}

fn transpile_python_to_rust(input_path: &Path) -> Result<String> {
    let source = fs::read_to_string(input_path)
        .with_context(|| format!("failed to read {}", input_path.display()))?;
    let root = env::current_dir()?;
    let cache_dir = root.join(".tarvos_cache");
    fs::create_dir_all(&cache_dir)
        .with_context(|| format!("failed to create cache directory {}", cache_dir.display()))?;

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "tarvos-cache-v1.2".hash(&mut hasher);
    source.hash(&mut hasher);
    let cache_path = cache_dir.join(format!("{:016x}.rs", hasher.finish()));

    if cache_path.is_file() {
        println!("Using cached native translation: {}", cache_path.display());
        return fs::read_to_string(&cache_path).with_context(|| {
            format!("failed to read cached translation {}", cache_path.display())
        });
    }

    let rust_source = CompilePipeline::transpile_file(input_path)?;
    let source = fs::read_to_string(input_path)
        .with_context(|| format!("failed to read {}", input_path.display()))?;
    let ast_json = export_python_ast(&source)?;
    let module = parse_python_ast(&ast_json)?;
    let detected = NativeSubsetDetector::default().analyze(&module);
    let specialization = specialize_module(&module, &detected);
    let rust_source = wire_specialization_runtime(&rust_source, &specialization);
    fs::write(&cache_path, &rust_source).with_context(|| {
        format!(
            "failed to write cached translation {}",
            cache_path.display()
        )
    })?;
    println!("Cached native translation: {}", cache_path.display());
    Ok(rust_source)
}

fn write_rust_output(output_path: &Path, rust_source: &str) -> Result<()> {
    CompilePipeline::write_rust_output(output_path, rust_source)
}

fn export_python_ast(source: &str) -> Result<String> {
    core_export_python_ast(source)
}

fn find_python_command() -> Result<String> {
    core_find_python_command()
}

fn compile_rust_binary(output_path: &Path, rust_source: &str) -> Result<()> {
    let rustc = ensure_rust_toolchain()?;
    let rust_file = output_path.with_extension("rs");
    fs::write(&rust_file, rust_source)
        .with_context(|| format!("failed to write {}", rust_file.display()))?;
    let status = Command::new(&rustc)
        .arg("-C")
        .arg("opt-level=3")
        .arg("-C")
        .arg("target-cpu=native")
        .arg("-C")
        .arg("strip=symbols")
        .arg("-C")
        .arg("panic=abort")
        .arg("-o")
        .arg(output_path)
        .arg(&rust_file)
        .status()
        .with_context(|| format!("failed to compile Rust target {}", output_path.display()))?;
    if !status.success() {
        return Err(anyhow::anyhow!(
            "native Rust compilation failed for {}",
            output_path.display()
        ));
    }
    Ok(())
}

fn ensure_rust_toolchain() -> Result<PathBuf> {
    if let Some(path) = find_rustc_command() {
        return Ok(path);
    }

    for candidate in [PathBuf::from("rustup"), PathBuf::from("rustup.exe")] {
        let tool = if candidate.is_absolute() {
            candidate
        } else {
            which_simple(&candidate.to_string_lossy())?.unwrap_or(candidate)
        };
        if !tool.exists() {
            continue;
        }
        let install = Command::new(&tool)
            .args(["toolchain", "install", "stable", "--profile", "minimal"])
            .status()
            .with_context(|| {
                format!(
                    "failed to bootstrap Rust toolchain using {}",
                    tool.display()
                )
            })?;
        if !install.success() {
            continue;
        }
        if let Some(path) = find_rustc_command() {
            return Ok(path);
        }
    }

    Err(anyhow::anyhow!(
        "Rust is not installed and could not be auto-bootstrapped. Source-only mode is still available via `tarvos compile <file.py> output.rs`."
    ))
}

fn sanitize_package_name(name: &str) -> String {
    let mut sanitized: String = name
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' => c,
            '-' | '_' | '.' => '_',
            _ => '_',
        })
        .collect();

    while sanitized.starts_with('_') {
        sanitized.remove(0);
    }
    while sanitized.ends_with('_') {
        sanitized.pop();
    }
    if sanitized.is_empty() {
        return "tarvos_project".to_string();
    }
    sanitized
}

pub(crate) fn init_mode(args: &[String]) -> Result<()> {
    let project_name = args
        .first()
        .cloned()
        .unwrap_or_else(|| "tarvos-app".to_string());
    let project_dir = PathBuf::from(&project_name);
    if project_dir.exists() {
        return Err(anyhow::anyhow!(
            "project directory already exists: {}",
            project_dir.display()
        ));
    }
    let src_dir = project_dir.join("src");
    fs::create_dir_all(&src_dir)
        .with_context(|| format!("failed to create {}", src_dir.display()))?;

    let package_name = sanitize_package_name(&project_name);
    let cargo_toml = format!(
        "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
        package_name
    );
    fs::write(project_dir.join("Cargo.toml"), cargo_toml)
        .with_context(|| format!("failed to write Cargo.toml in {}", project_dir.display()))?;

    let sample = "print(\"Hello from Tarvos!\")\n";
    fs::write(src_dir.join("main.py"), sample)
        .with_context(|| format!("failed to write sample program in {}", src_dir.display()))?;

    fs::write(
        project_dir.join("README.md"),
        "# Tarvos project\n\nRun:\n\n```bash\ntarvos build src/main.py\n```\n",
    )
    .with_context(|| format!("failed to write README in {}", project_dir.display()))?;

    println!("Initialized Tarvos project at {}", project_dir.display());
    println!("Next steps:");
    println!("  tarvos build {}\\src\\main.py", project_dir.display());
    Ok(())
}

pub(crate) fn clean_mode(_args: &[String]) -> Result<()> {
    let root = env::current_dir()?;
    for rel in [
        ".build-tmp",
        ".tarvos_cache",
        "tmp_build",
        "out",
        "tarvos-export",
    ] {
        let target = root.join(rel);
        if target.exists() {
            if target.is_dir() {
                fs::remove_dir_all(&target)
                    .with_context(|| format!("failed to remove {}", target.display()))?;
            } else {
                fs::remove_file(&target)
                    .with_context(|| format!("failed to remove {}", target.display()))?;
            }
            println!("Removed {}", target.display());
        }
    }
    println!("Workspace cleaned.");
    Ok(())
}

pub(crate) fn export_mode(args: &[String]) -> Result<()> {
    let input_file = args
        .first()
        .ok_or_else(|| anyhow::anyhow!("usage: tarvos export <file.py> [output-dir]"))?;
    let output_dir = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "tarvos-export".to_string());
    let root = env::current_dir()?;
    let input_path = secure_input_path(input_file, &root)?;
    let output_path = secure_output_path(&output_dir, &root)?;

    let rust_source = transpile_python_to_rust(&input_path)?;
    let src_dir = output_path.join("src");
    fs::create_dir_all(&src_dir)
        .with_context(|| format!("failed to create {}", src_dir.display()))?;

    let project_name = sanitize_package_name(
        output_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .as_ref(),
    );
    let cargo_toml = format!(
        "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
        project_name
    );
    fs::write(output_path.join("Cargo.toml"), cargo_toml)
        .with_context(|| format!("failed to write Cargo.toml in {}", output_path.display()))?;
    fs::write(src_dir.join("main.rs"), &rust_source)
        .with_context(|| format!("failed to write Rust source in {}", src_dir.display()))?;
    fs::write(
        output_path.join("README.md"),
        "# Exported Tarvos project\n\nThis project was generated by `tarvos export`.\n",
    )
    .with_context(|| format!("failed to write README in {}", output_path.display()))?;

    println!("Exported Tarvos project to {}", output_path.display());
    println!("Build with: cargo build --release");
    Ok(())
}

pub(crate) fn analyze_mode(args: &[String]) -> Result<()> {
    let input_file = args
        .first()
        .ok_or_else(|| anyhow::anyhow!("usage: tarvos analyze <file.py>"))?;

    let source =
        fs::read_to_string(input_file).with_context(|| format!("failed to read {}", input_file))?;

    let ast_json = export_python_ast(&source)?;
    let module = parse_python_ast(&ast_json)?;
    let stats = analyze_module(&module);

    println!("Input: {}", input_file);
    println!("{}", stats);

    Ok(())
}

pub(crate) fn benchmark_mode(args: &[String]) -> Result<()> {
    let input_file = args
        .first()
        .ok_or_else(|| anyhow::anyhow!("usage: tarvos benchmark <file.py> [reference.rs]"))?;
    let reference_file = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "examples/simple.rs".to_string());

    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let python_script = repo_root.join("benchmarks").join("run_benchmarks.py");
    if !python_script.exists() {
        return Err(anyhow::anyhow!(
            "could not find benchmarks/run_benchmarks.py"
        ));
    }

    let caller_dir = env::current_dir()?;
    let input_path = resolve_readable_path(input_file, &caller_dir)
        .with_context(|| format!("benchmark input: {}", input_file))?;
    let reference_path = if args.get(1).is_some() {
        resolve_readable_path(&reference_file, &caller_dir)
            .with_context(|| format!("benchmark reference: {}", reference_file))?
    } else {
        secure_input_path(&reference_file, &repo_root)?
    };

    // Keep the benchmark runner inside the repository's controlled workspace.
    let stage_dir = repo_root.join(".build-tmp").join(format!(
        "cli-benchmark-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    fs::create_dir_all(&stage_dir).with_context(|| {
        format!(
            "failed to create benchmark staging directory {}",
            stage_dir.display()
        )
    })?;
    let staged_input = stage_dir.join(input_path.file_name().unwrap_or_default());
    let staged_reference = stage_dir.join(reference_path.file_name().unwrap_or_default());
    fs::copy(&input_path, &staged_input)
        .with_context(|| format!("failed to stage benchmark input {}", input_path.display()))?;
    fs::copy(&reference_path, &staged_reference).with_context(|| {
        format!(
            "failed to stage benchmark reference {}",
            reference_path.display()
        )
    })?;

    let _ = ensure_rust_toolchain().ok();
    let mut cmd = Command::new(find_python_command()?);
    cmd.arg(&python_script)
        .arg(&staged_input)
        .arg(&staged_reference)
        .arg(format!("Benchmark: {}", input_path.display()));

    let output = cmd
        .current_dir(&repo_root)
        .output()
        .with_context(|| format!("failed to run benchmark for {}", input_path.display()))?;
    let _ = fs::remove_dir_all(&stage_dir);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("{}", stderr);
        std::process::exit(output.status.code().unwrap_or(1));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    print!("{}", stdout);
    Ok(())
}

fn resolve_readable_path(input: &str, base_dir: &Path) -> Result<PathBuf> {
    let candidate = PathBuf::from(input);
    let absolute = if candidate.is_absolute() {
        candidate
    } else {
        base_dir.join(candidate)
    };
    let canonical = absolute
        .canonicalize()
        .with_context(|| format!("file does not exist: {}", absolute.display()))?;
    if !canonical.is_file() {
        return Err(anyhow::anyhow!(
            "benchmark path is not a file: {}",
            canonical.display()
        ));
    }
    Ok(canonical)
}

fn print_help() {
    eprintln!("Tarvos - Python to Rust compiler");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  tarvos <file.py> [output.rs]");
    eprintln!("  tarvos compile <file.py> [output.rs]");
    eprintln!("  tarvos build <file.py> [output.exe]");
    eprintln!("  tarvos run <file.py> [args...]");
    eprintln!("  tarvos run <file.py> --python-fallback [args...]");
    eprintln!("  tarvos python <file.py> [args...]");
    eprintln!("  tarvos analyze <file.py>");
    eprintln!("  tarvos benchmark <file.py> [reference.rs]");
    eprintln!("  tarvos doctor");
    eprintln!("  tarvos --help");
    eprintln!();
    eprintln!("Native output notes:");
    eprintln!(
        "  - `compile` emits Rust source and can optionally build an EXE via `--format exe`."
    );
    eprintln!("  - `build` directly emits a native executable when a Rust toolchain is available.");
    eprintln!("  - `run` transpiles, builds, and executes the program in one command.");
    eprintln!("  - `run --python-fallback` tries native execution, then explicitly falls back to CPython.");
    eprintln!("  - `python` executes any Python program through the local CPython runtime.");
    eprintln!("  - Rust remains optional for source emission; developers can still use `tarvos compile` without final native build.");
    eprintln!();
    eprintln!("Supported subset:");
    eprintln!("  - integers, floats, bools, strings, None");
    eprintln!("  - arithmetic and comparisons");
    eprintln!("  - print(), if/else, while, for range(...) loops");
    eprintln!("  - simple function definitions and typed annotations");
    eprintln!();
    eprintln!("Example:");
    eprintln!("  def add(a: int, b: int) -> int:");
    eprintln!("      return a + b");
    eprintln!();
    eprintln!("Note: Tarvos targets a statically analyzable subset of Python, not full Python compatibility.");
}

fn find_rustc_command() -> Option<PathBuf> {
    let candidates = [
        env::var("RUSTC").ok().map(PathBuf::from),
        Some(PathBuf::from("rustc")),
        Some(PathBuf::from("rustc.exe")),
        Some(PathBuf::from("rustup")),
        Some(PathBuf::from("rustup.exe")),
    ];
    for candidate in candidates.into_iter().flatten() {
        let path = if candidate.is_absolute() {
            candidate
        } else {
            which_simple(&candidate.to_string_lossy())
                .ok()
                .flatten()
                .unwrap_or(candidate)
        };
        if !path.exists() {
            continue;
        }
        let canonical = path.canonicalize().unwrap_or(path.clone());
        let file_name = canonical
            .file_name()
            .map(|s| s.to_string_lossy().to_ascii_lowercase());

        if matches!(file_name.as_deref(), Some("rustup.exe") | Some("rustup")) {
            if let Ok(output) = Command::new(&canonical).arg("which").arg("rustc").output() {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !stdout.is_empty() {
                        let resolved = PathBuf::from(stdout.lines().next().unwrap_or_default());
                        if resolved.exists() {
                            return Some(resolved.canonicalize().unwrap_or(resolved));
                        }
                    }
                }
            }
            let sibling = canonical.with_file_name("rustc.exe");
            if sibling.exists() {
                return Some(sibling.canonicalize().unwrap_or(sibling));
            }
        }

        if matches!(file_name.as_deref(), Some("rustc.exe") | Some("rustc")) {
            return Some(canonical);
        }
    }
    None
}

fn which_simple(name: &str) -> Result<Option<PathBuf>> {
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
    Ok(Some(PathBuf::from(first)))
}

fn secure_input_path(input: &str, root: &Path) -> Result<PathBuf> {
    let candidate = PathBuf::from(input);
    let absolute = if candidate.is_absolute() {
        candidate
    } else {
        root.join(candidate)
    };
    let canonical = normalize_windows_path(absolute.canonicalize().unwrap_or(absolute.clone()));
    let root_norm = normalize_windows_path(root.canonicalize().unwrap_or(root.to_path_buf()));
    if !canonical.starts_with(&root_norm) {
        return Err(anyhow::anyhow!(
            "input path is outside the safe project root: {}",
            canonical.display()
        ));
    }
    Ok(canonical)
}

fn secure_output_path(output: &str, root: &Path) -> Result<PathBuf> {
    let candidate = PathBuf::from(output);
    let absolute = if candidate.is_absolute() {
        candidate
    } else {
        root.join(candidate)
    };
    let parent = absolute.parent().unwrap_or(root);
    if !parent.exists() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create output directory {}", parent.display()))?;
    }
    let canonical_parent =
        normalize_windows_path(parent.canonicalize().unwrap_or(parent.to_path_buf()));
    let root_norm = normalize_windows_path(root.canonicalize().unwrap_or(root.to_path_buf()));
    if !canonical_parent.starts_with(&root_norm) {
        return Err(anyhow::anyhow!(
            "output directory is outside the safe project root: {}",
            canonical_parent.display()
        ));
    }
    Ok(absolute)
}

fn normalize_windows_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy();
        let stripped = text
            .strip_prefix("\\\\?\\UNC\\")
            .or_else(|| text.strip_prefix("\\\\?\\"))
            .unwrap_or(&text);
        PathBuf::from(stripped)
    }
    #[cfg(not(windows))]
    {
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tarvos_analysis::lower_module;
    use tarvos_codegen_rust::RustCodegen;
    use tarvos_optimizer::Optimizer;

    #[test]
    fn branching_workload_keeps_then_branch_assignment() {
        let source = fs::read_to_string("../../benchmarks/workloads/branching.py").unwrap();
        let ast_json = export_python_ast(&source).unwrap();
        let module = parse_python_ast(&ast_json).unwrap();
        let ir = lower_module(&module).unwrap();
        let ir = Optimizer::optimize(&ir).unwrap();
        let rust = RustCodegen::generate(&ir).unwrap();

        assert!(
            rust.contains("result = 1_i64;"),
            "missing then-branch assignment:\n{}",
            rust
        );
        assert!(
            rust.contains("result = 0_i64;"),
            "missing else-branch assignment:\n{}",
            rust
        );
    }
}
