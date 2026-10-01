use anyhow::{Context, Result};
use clap::{ArgAction, CommandFactory, Parser, Subcommand};
use std::{
    env, fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

mod ai_probe;
mod commands;
mod toolchain;
use commands::{
    analyze_command, benchmark_command, clean_command, doctor_command, export_command,
    init_command, install_command, validate_command,
};
use tarvos_analysis::analyze_module;
use tarvos_analysis::native_detector::{ModuleReport, NativePlan, NativeSubsetDetector};
use tarvos_analysis::native_specialization::{specialize_module, SpecializedLoop};
use tarvos_core::{
    export_python_ast as core_export_python_ast, find_python_command as core_find_python_command,
    CompilePipeline,
};
use tarvos_parser::parse_python_ast;

/// Long description shown by `tarvos --help`.
const LONG_ABOUT: &str = "\
Tarvos transpiles a statically analyzable subset of Python into optimized Rust
and compiles it into a single native executable. The produced binary is a
standalone program: it does not embed CPython and does not require pyo3 or a
Python installation at run time.

Tarvos aims at a measurable subset of Python, not full CPython compatibility.
Run `tarvos doctor` to verify your toolchain, and `tarvos analyze <file.py>` to
see which functions a given module can accelerate before you compile it.

Quick start:
  tarvos doctor                       Check Python, Rust, Cargo, and linker
  tarvos build hello.py -o hello      Transpile and build a native executable
  tarvos run hello.py arg1 arg2       Build, then run it in one step
  tarvos benchmark kernel.py          Measure Python vs native with fairness
                                      checks
  tarvos validate                     Run the reference output test suite

Docs and examples: https://github.com/repo-tech/tarvos-engine
Benchmarks and compatibility notes are published with every engine release.";

/// Trailing section shown after the option list in `tarvos --help`.
const AFTER_HELP: &str = "\
Environment:
  RUSTC                    Path to the rustc used by `build` and `compile`
  TARVOS_STRICT_RUST_PIN   Set to 1 to fail instead of using a loose system Rust
  TARVOS_OLLAMA_ENDPOINT   Loopback endpoint read by `tarvos ai-status`
                           (default http://127.0.0.1:11434)
  TARVOS_SANDBOX           Restricts commands to a sandboxed working area

Exit status:
  0  success
  1  compilation, toolchain, or runtime failure
  2  invalid command-line usage

Examples:
  tarvos compile main.py --format exe -o app.exe
  tarvos build main.py -o app --system-rust
  tarvos run main.py -- --verbose input.csv
  tarvos run main.py --python-fallback
  tarvos analyze src/ --hot-functions
  tarvos package ./myproject --entry main.py -o ./dist
  tarvos export main.py ./tarvos-export
  tarvos init my-app";

const COMPILE_HELP: &str = "\
Compile Python code to optimized Rust source or binary.

Runs the full front end: Python AST export, parsing, type checking and IR
lowering, optimization, then Rust code generation.

With `--format rust` (the default) Tarvos writes Rust source and stops there,
so no Rust toolchain is required. With `--format exe` it invokes rustc
afterwards to produce a native executable.

When a module uses dynamic constructs outside the native subset, Tarvos reports
why and emits a compatibility launcher that delegates to Python at run time
instead of failing the build.

Examples:
  tarvos compile app.py
  tarvos compile app.py -o src/main.rs
  tarvos compile app.py --format exe -o app.exe
  tarvos compile app.py --target embedded";

const BUILD_HELP: &str = "\
Build a native binary executable directly from Python.

Transpiles the module and links it into a standalone executable in one step. The
resulting binary embeds no Python interpreter, so it can be shipped to a machine
with no Python installed.

Tarvos uses the managed toolchain in ~/.tarvos/toolchain by default. Use
`--system-rust` to opt into an already validated system Rust.

If the module uses behaviour the native backend cannot express, this command
fails rather than emitting a launcher. Such a launcher would re-run the original
.py file through the system `python` using a path baked in at build time, so it
would only work on the machine that built it. Use `tarvos run` to execute the
program on this machine, or pass `--compat-launcher` to force the launcher.

Examples:
  tarvos build app.py
  tarvos build app.py -o dist/app
  tarvos build app.py --system-rust
  tarvos build app.py --compat-launcher
  tarvos build app.py --source-only";

const RUN_HELP: &str = "\
Transpile, build, and run a Python file in one seamless step.

Attempts a native build first and executes the resulting binary. If the module
relies on dynamic behaviour that the native subset cannot express, Tarvos reports
the reason and transparently falls back to the local Python runtime so the
program still runs.

Use `--python-fallback` to skip the native attempt entirely, which helps isolate
whether a slowdown comes from the transpiler or from the original code.
Arguments after the script name are forwarded to the program; use `--` to stop
Tarvos option parsing.

Examples:
  tarvos run app.py
  tarvos run app.py input.csv
  tarvos run app.py -- --verbose input.csv
  tarvos run app.py --python-fallback";

const PYTHON_HELP: &str = "\
Execute any Python program through the local Python runtime.

Always uses CPython and never invokes the transpiler, which makes this the
reference path for checking Tarvos output against the original program.

Examples:
  tarvos python app.py
  tarvos python app.py --verbose data.json";

const DOCTOR_HELP: &str = "\
Run environment diagnostics and check toolchain dependencies.

Reports the resolved path and version of every component Tarvos depends on, and
explains what to install or fix when something is missing. Run this first
whenever a build fails.

Example:
  tarvos doctor";

const ANALYZE_HELP: &str = "\
Static analysis and complexity profiling of a Python module.

Reports per-function complexity and the constructs Tarvos can lower to native
code. With `--hot-functions` it ranks the functions worth accelerating first.

Accepts either a single file or a project directory.

Examples:
  tarvos analyze app.py
  tarvos analyze ./src --hot-functions";

const SCAN_HELP: &str = "\
Scan a Python project for library-backed hot loops and native plans.

Walks a project directory, finds loops that call into supported libraries, and
proposes a native plan for each one. This is the fastest way to find out whether
a project is a good candidate for Tarvos.

Example:
  tarvos scan ./myproject";

const BENCHMARK_HELP: &str = "\
Benchmark Python vs Tarvos (Native Rust) with fairness metrics.

Compiles the workload both ways, compares their output for equality, then reports
median and per-iteration timings. Workloads are run without constant folding so
the comparison reflects real work.

Pass a reference .rs file to benchmark hand-written Rust alongside Python and
Tarvos.

Examples:
  tarvos benchmark kernel.py
  tarvos benchmark kernel.py reference.rs";

const INIT_HELP: &str = "\
Initialize a new Tarvos project template.

Creates a ready to build project skeleton in a new directory.

Example:
  tarvos init my-app";

const EXPORT_HELP: &str = "\
Export Python code as a complete standalone Cargo Rust project.

Writes a self-contained Cargo project you can open, edit, and build with plain
cargo. Useful when you want to inspect or hand-tune the generated Rust before
shipping a binary.

Examples:
  tarvos export app.py
  tarvos export app.py ./my-export";

const PACKAGE_HELP: &str = "\
Convert a Python file or project folder into a Cargo release project and binary.

Like `export`, but also runs a release cargo build, so the command ends with a
ready to distribute optimized binary. When the input is a directory, point at the
entry point with `--entry`.

Examples:
  tarvos package app.py
  tarvos package ./myproject --entry main.py
  tarvos package app.py -o ./dist";

const CLEAN_HELP: &str = "\
Clean build artifacts, temporary cache files, and intermediate outputs.

Removes generated Rust, staged Cargo projects, and Tarvos cache directories.
Source files are never touched.

Example:
  tarvos clean";

const TOOLCHAIN_HELP: &str = "\
Manage the Tarvos-owned Rust toolchain (status, install, verify).

Tarvos pins its own Rust channel under ~/.tarvos/toolchain so native builds are
reproducible regardless of the system Rust version.

Examples:
  tarvos toolchain --status
  tarvos toolchain --install
  tarvos toolchain --verify";

const INSTALL_HELP: &str = "\
Install Tarvos system-wide into your PATH.

Copies the tarvos executable into a directory already on your PATH and tells you
if a shell restart is needed.

Example:
  tarvos install";

const VALIDATE_HELP: &str = "\
Validate Tarvos output against reference test suite.

Compiles every workload in the reference suite and diffs the program output
against the recorded expected values, so a regression shows up before it reaches
a release.

Example:
  tarvos validate";

/// Version reported by `--version` and by the clap help footer.
///
/// The compiler and the public distribution are released on independent version
/// lines: the compiler ships many release candidates, while the packaged engine
/// is cut when there is something worth publishing. A build can therefore pin
/// the user-facing line with `TARVOS_PRODUCT_VERSION`; without it the crate
/// version is used, so a plain `cargo build` still reports the compiler version
/// and the version-consistency gate keeps passing.
const PRODUCT_VERSION: &str = match option_env!("TARVOS_PRODUCT_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

/// Tarvos — Ultra-fast Python to Native Rust Transpiler & Compiler
#[derive(Parser, Debug)]
#[command(name = "tarvos", disable_version_flag = true)]
#[command(author = "Himanshu & Repo-Tech Team")]
#[command(version = PRODUCT_VERSION)]
#[command(about = "Transpiles and compiles Python code to native high-performance Rust executables", long_about = LONG_ABOUT, after_help = AFTER_HELP, after_long_help = AFTER_HELP)]
struct Cli {
    /// Print the Tarvos version and exit
    #[arg(short = 'v', long = "version", action = ArgAction::SetTrue)]
    version: bool,

    #[command(subcommand)]
    command: Option<Commands>,

    /// Direct input Python file when no subcommand is specified.
    /// Shorthand for `tarvos compile <FILE.py>`.
    #[arg(value_name = "INPUT.py")]
    input: Option<PathBuf>,

    /// Output path when running in direct mode (defaults to output.rs)
    #[arg(short, long, value_name = "OUTPUT")]
    output: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Compile Python code to optimized Rust source or binary
    #[command(long_about = COMPILE_HELP)]
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

        /// Code generation target: native (default) or embedded (strict no_std subset)
        #[arg(long, default_value = "native")]
        target: String,
    },

    /// Build a native binary executable directly from Python
    #[command(long_about = BUILD_HELP)]
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

        /// Use an explicitly validated system Rust instead of the managed one
        #[arg(long = "system-rust")]
        system_rust: bool,

        /// Allow a launcher that requires Python at run time, for code outside
        /// the native subset. The result is not portable.
        #[arg(long = "compat-launcher")]
        compat_launcher: bool,
    },

    /// Transpile, build, and run a Python file in one seamless step
    #[command(long_about = RUN_HELP)]
    Run {
        /// Input Python file (.py)
        #[arg(value_name = "FILE.py")]
        input: PathBuf,

        /// Force the compatibility runtime instead of attempting native compilation first.
        #[arg(long = "python-fallback", alias = "compat-runtime")]
        python_fallback: bool,

        /// Arguments to pass to the executed binary
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Execute any Python program through the local Python runtime
    #[command(long_about = PYTHON_HELP)]
    Python {
        /// Input Python file (.py)
        #[arg(value_name = "FILE.py")]
        input: PathBuf,

        /// Arguments to pass to the Python program
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Run environment diagnostics and check toolchain dependencies (Python, Rust, Cargo, Linker)
    #[command(long_about = DOCTOR_HELP)]
    Doctor,

    /// Report the optional local AI capability (Ollama).
    ///
    /// Read-only: never starts, stops, downloads, or reconfigures anything, and
    /// only ever contacts a loopback endpoint. Records ownership so a later
    /// repair or uninstall step knows what Tarvos is allowed to touch.
    AiStatus {
        /// Emit machine-readable JSON instead of the human-readable report
        #[arg(long)]
        json: bool,
    },

    /// Static analysis and complexity profiling of a Python module
    #[command(long_about = ANALYZE_HELP)]
    Analyze {
        /// Input Python file (.py) or a project directory
        #[arg(value_name = "INPUT")]
        input: PathBuf,

        /// Report functions that are candidates for native hot-path acceleration
        #[arg(long)]
        hot_functions: bool,
    },

    /// Scan a Python project for library-backed hot loops and native plans
    #[command(long_about = SCAN_HELP)]
    Scan {
        /// Project directory or Python file to inspect
        input: PathBuf,
    },

    /// Benchmark Python vs Tarvos (Native Rust) with fairness metrics
    #[command(long_about = BENCHMARK_HELP)]
    Benchmark {
        /// Input Python file (.py)
        #[arg(value_name = "FILE.py")]
        input: PathBuf,

        /// Optional reference Rust implementation
        #[arg(value_name = "REF.rs")]
        reference: Option<PathBuf>,
    },

    /// Initialize a new Tarvos project template
    #[command(long_about = INIT_HELP)]
    Init {
        /// Project directory name
        #[arg(default_value = "tarvos-app")]
        name: String,
    },

    /// Export Python code as a complete standalone Cargo Rust project
    #[command(long_about = EXPORT_HELP)]
    Export {
        /// Input Python file (.py)
        #[arg(value_name = "FILE.py")]
        input: PathBuf,

        /// Destination directory for the exported Cargo project
        #[arg(value_name = "DIR", default_value = "tarvos-export")]
        output_dir: PathBuf,
    },

    /// Convert a Python file or project folder into a Cargo release project and binary
    #[command(long_about = PACKAGE_HELP)]
    Package {
        /// Python file or project directory
        input: PathBuf,

        /// Entry Python file when input is a directory (defaults to main.py)
        #[arg(long, value_name = "FILE.py")]
        entry: Option<PathBuf>,

        /// Destination Cargo project directory
        #[arg(short, long, value_name = "DIR")]
        output_dir: Option<PathBuf>,
    },

    /// Clean build artifacts, temporary cache files, and intermediate outputs
    #[command(long_about = CLEAN_HELP)]
    Clean,

    /// Manage the Tarvos-owned Rust toolchain (status, install, verify)
    #[command(long_about = TOOLCHAIN_HELP)]
    Toolchain {
        /// Show resolved toolchain, layout, and per-stage validation
        #[arg(long)]
        status: bool,

        /// Fetch the pinned channel into ~/.tarvos/toolchain
        #[arg(long)]
        install: bool,

        /// Re-run validation on the managed toolchain and report each check
        #[arg(long)]
        verify: bool,
    },

    /// Install Tarvos system-wide into your PATH
    #[command(long_about = INSTALL_HELP)]
    Install,

    /// Validate Tarvos output against reference test suite
    #[command(long_about = VALIDATE_HELP)]
    Validate,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.version {
        println!("tarvos {PRODUCT_VERSION}");
        return Ok(());
    }

    match cli.command {
        Some(Commands::Compile {
            input,
            output,
            format,
            source_only,
            target,
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
            if target != "native" {
                args.push("--target".to_string());
                args.push(target);
            }
            compile_mode(&args)
        }
        Some(Commands::Build {
            input,
            output,
            source_only,
            system_rust,
            compat_launcher,
        }) => {
            let mut args = vec![input.to_string_lossy().to_string()];
            if let Some(o) = output {
                args.push("--output".to_string());
                args.push(o.to_string_lossy().to_string());
            }
            if source_only {
                args.push("--source-only".to_string());
            }
            if system_rust {
                args.push("--system-rust".to_string());
            }
            if compat_launcher {
                args.push("--compat-launcher".to_string());
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
                python_mode(&args)
            } else {
                hybrid_run_mode(&args)
            }
        }
        Some(Commands::Python { input, args: extra }) => {
            let mut args = vec![input.to_string_lossy().to_string()];
            args.extend(extra);
            python_mode(&args)
        }
        Some(Commands::Doctor) => doctor_command(&[]),
        Some(Commands::AiStatus { json }) => ai_status_mode(json),
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
        Some(Commands::Package {
            input,
            entry,
            output_dir,
        }) => package_project_mode(&input, entry.as_deref(), output_dir.as_deref()),
        Some(Commands::Clean) => clean_command(&[]),
        Some(Commands::Toolchain {
            status,
            install,
            verify,
        }) => toolchain::toolchain_command(status, install, verify),
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
    let mut target = "native".to_string();
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
            "--target" => {
                target = iter.next().cloned().unwrap_or_else(|| "native".to_string());
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
    let (rust_source, compatibility_launcher) = match target.as_str() {
        "native" => match transpile_python_to_rust(&input_path) {
            Ok(source) => (source, false),
            Err(error) if is_dynamic_native_error(&error) => {
                warn_native_fallback(&error);
                (compatibility_launcher_source(&input_path)?, true)
            }
            Err(error) => return Err(error),
        },
        "embedded" => (
            CompilePipeline::transpile_file_embedded(&input_path)?,
            false,
        ),
        other => {
            return Err(anyhow::anyhow!(
                "unsupported target `{other}`; choose `native` or `embedded`"
            ))
        }
    };

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
    if compatibility_launcher {
        println!("Mode: compatibility launcher (requires Python at execution time)");
    } else {
        println!("Mode: native Rust pipeline (Python runtime not required)");
    }

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
    let mut prefer_system = false;
    let mut allow_compat_launcher = false;
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
            "--system-rust" => {
                prefer_system = true;
            }
            "--compat-launcher" => {
                allow_compat_launcher = true;
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
    let (rust_source, compatibility_launcher) = match transpile_python_to_rust(&input_path) {
        Ok(source) => (source, false),
        Err(error) if is_dynamic_native_error(&error) => {
            // `build` produces an artifact meant to be shipped to another machine,
            // so a launcher that needs the original .py file, a Python install and
            // the build machine's absolute path is not a build the caller asked
            // for. It used to be produced anyway, and the only trace was one line
            // of output; the resulting executable then failed on any machine
            // without the same paths, which is exactly where it was supposed to
            // work. Refusing here is the only failure that happens early enough to
            // be cheap.
            if !allow_compat_launcher {
                return Err(anyhow::anyhow!(
                    "{error}\n\n\
                     `tarvos build` did not produce a native binary. The program uses Python \
                     behaviour the native backend cannot express, so the only thing that could \
                     be built here is a launcher that re-runs the original .py file through the \
                     system `python`. Such an executable is not portable: it needs that file at \
                     its original path, plus a Python installation, and it fails on every \
                     machine that does not have both.\n\n\
                     To run it on this machine instead of shipping it, use `tarvos run`.\n\
                     To force the launcher anyway, pass --compat-launcher."
                ));
            }
            warn_native_fallback(&error);
            (compatibility_launcher_source(&input_path)?, true)
        }
        Err(error) => return Err(error),
    };
    let rust_output = output_path.with_extension("rs");

    if source_only {
        write_rust_output(&rust_output, &rust_source)?;
        println!("Source-only build complete: {}", rust_output.display());
        println!("Rust is not required for source generation. Use `tarvos build ... --source-only` to emit Rust without native EXE compilation.");
        return Ok(());
    }

    match compile_rust_binary_toolchain(&output_path, &rust_source, false, prefer_system) {
        Ok(()) => {
            if compatibility_launcher {
                println!(
                    "Compatibility launcher built: {} (requires Python at execution time)",
                    output_path.display()
                );
            } else {
                println!("Build complete: {}", output_path.display());
            }
            Ok(())
        }
        Err(err) => {
            eprintln!(
                "Rust native build unavailable; generated Rust remains in the user cache for this invocation."
            );
            // This message used to say "Install Rust", which is wrong twice
            // over: the toolchain had already resolved successfully by the
            // time codegen could fail, so telling the user to install Rust
            // points at a problem they do not have. Name the toolchain that
            // actually ran and the real reason instead.
            eprintln!("The toolchain above did compile; the generated Rust did not.");
            eprintln!("Use `tarvos compile <file.py> --source-only` to emit Rust without native compilation.");
            Err(err)
        }
    }
}

fn package_project_mode(
    input: &Path,
    requested_entry: Option<&Path>,
    requested_output: Option<&Path>,
) -> Result<()> {
    let cwd = env::current_dir()?;
    let input = canonicalize_existing_path(input, "package input")?;
    let (project_root, entry) = if input.is_dir() {
        let entry = requested_entry
            .map(PathBuf::from)
            .unwrap_or_else(|| input.join("main.py"));
        let entry = if entry.is_absolute() {
            entry
        } else {
            input.join(entry)
        };
        (
            input.clone(),
            canonicalize_existing_path(&entry, "package entrypoint")?,
        )
    } else {
        let entry = input.clone();
        (
            input.parent().map(Path::to_path_buf).unwrap_or(cwd.clone()),
            entry,
        )
    };
    if !entry.is_file() {
        return Err(anyhow::anyhow!(
            "package entrypoint is not a file: {}",
            entry.display()
        ));
    }
    let output = requested_output.map(PathBuf::from).unwrap_or_else(|| {
        cwd.join(format!(
            "{}-tarvos-dist",
            project_root
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        ))
    });
    let output = if output.is_absolute() {
        output
    } else {
        cwd.join(output)
    };
    if output.starts_with(&project_root) {
        return Err(anyhow::anyhow!(
            "package output must not be inside the input project: {}",
            output.display()
        ));
    }
    if output.exists() {
        return Err(anyhow::anyhow!(
            "package output already exists; choose an empty destination: {}",
            output.display()
        ));
    }

    let src_dir = output.join("src");
    let dist_dir = output.join("dist");
    fs::create_dir_all(&src_dir)?;
    fs::create_dir_all(&dist_dir)?;
    let project_name = sanitize_package_name(
        output
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .as_ref(),
    );
    let rust_source = transpile_python_to_rust(&entry)?;
    fs::write(src_dir.join("main.rs"), rust_source)?;
    fs::write(
        output.join("Cargo.toml"),
        format!(
            "[workspace]\n\n[package]\nname = \"{}\"\nversion = \"1.1.0-rc.6\"\nedition = \"2021\"\n\n[profile.release]\nopt-level = \"z\"         # Optimize aggressively for strict minimum size\nlto = true              # Enable whole-program Link-Time Optimization\ncodegen-units = 1       # Reduce parallel blocks to maximize single-binary optimization\npanic = \"abort\"         # Completely terminate stack unwinding code tables\nstrip = true            # Guarantee complete binary stripping of metadata and symbols\n",
            project_name
        ),
    )?;
    fs::write(
        output.join("README.md"),
        "# Tarvos packaged project\n\nBuild output is available in `dist/`.\n",
    )?;
    copy_project_assets(&project_root, &output.join("python"), &entry)?;

    let cargo = which_simple("cargo")?
        .ok_or_else(|| anyhow::anyhow!("Cargo is required for `tarvos package`"))?;
    let status = Command::new(cargo)
        .args(["build", "--release"])
        .current_dir(&output)
        .status()
        .with_context(|| format!("failed to build packaged project {}", output.display()))?;
    if !status.success() {
        return Err(anyhow::anyhow!(
            "packaged Rust project failed to build; inspect {}",
            // A literal backslash would be one path element named
            // `src\main.rs` on Unix, so the separator has to come from `join`.
            output.join("src").join("main.rs").display()
        ));
    }
    let built = output
        .join("target")
        .join("release")
        .join(if cfg!(windows) {
            format!("{}.exe", project_name)
        } else {
            project_name.clone()
        });
    let dist_binary = dist_dir.join(
        built
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("packaged binary has no file name"))?,
    );
    fs::copy(&built, &dist_binary).with_context(|| {
        format!(
            "failed to copy packaged binary to {}",
            dist_binary.display()
        )
    })?;
    println!("Packaged Rust project: {}", output.display());
    println!("Release executable: {}", dist_binary.display());
    Ok(())
}

fn copy_project_assets(root: &Path, destination: &Path, entry: &Path) -> Result<()> {
    fn visit(source: &Path, root: &Path, destination: &Path, entry: &Path) -> Result<()> {
        for item in fs::read_dir(source)? {
            let item = item?;
            let path = item.path();
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name == "__pycache__"
                || name == ".venv"
                || name == ".git"
                || name == "target"
                || name == ".build-tmp"
                || name == ".tarvos_cache"
            {
                continue;
            }
            if path.is_file()
                && matches!(
                    path.extension().and_then(|extension| extension.to_str()),
                    Some("exe" | "dll" | "pdb" | "o" | "rlib" | "rmeta")
                )
            {
                continue;
            }
            let relative = path.strip_prefix(root).unwrap_or(&path);
            let target = destination.join(relative);
            if path == entry {
                continue;
            }
            if path.is_dir() {
                fs::create_dir_all(&target)?;
                visit(&path, root, destination, entry)?;
            } else {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&path, target)?;
            }
        }
        Ok(())
    }
    fs::create_dir_all(destination)?;
    visit(root, root, destination, entry)
}

fn prepare_native_run(args: &[String]) -> Result<(PathBuf, Vec<String>)> {
    let input_file = args
        .first()
        .ok_or_else(|| anyhow::anyhow!("usage: tarvos run <file.py> [args...]"))?;
    let working_dir = env::current_dir()?;
    let input_path = secure_input_path(input_file, &working_dir)?;
    let rust_source = transpile_python_to_rust(&input_path)?;
    let temp_dir = tarvos_cache_dir()?.join("runs");
    fs::create_dir_all(&temp_dir).with_context(|| {
        format!(
            "failed to create user cache directory {}",
            temp_dir.display()
        )
    })?;
    let mut source_hasher = std::collections::hash_map::DefaultHasher::new();
    "tarvos-run-cache-v2-fast".hash(&mut source_hasher);
    rust_source.hash(&mut source_hasher);
    let exe_path = temp_dir.join(format!(
        "{}-{:016x}{}",
        input_path.file_stem().unwrap_or_default().to_string_lossy(),
        source_hasher.finish(),
        if cfg!(windows) { ".exe" } else { "" }
    ));
    if exe_path.is_file() {
        println!("Using cached native executable: {}", exe_path.display());
    } else {
        println!("Compiling native executable for rapid execution...");
        compile_rust_binary_opt(&exe_path, &rust_source, true)?;
    }
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
    // The scan target is named explicitly by the user, so it is resolved the
    // same way as every other input: canonicalized, but not pinned to the
    // current directory. Scanning `~/projects/app` from an unrelated folder is
    // a normal request; refusing it was the same usability bug as
    // `secure_input_path`, and it also blocked the CLI audit, which runs in a
    // temporary directory.
    let input = canonicalize_existing_path(input, "scan path")?;
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
    let mut scan_errors = 0;
    println!("=== Tarvos Native Subset & Library Loop Scan ===");
    println!("Project: {}", input.display());
    for file in &files {
        let source = match fs::read_to_string(file) {
            Ok(source) => source,
            Err(error) => {
                println!("\nFile: {}\n  Scan error: {}", file.display(), error);
                scan_errors += 1;
                continue;
            }
        };
        let ast_json = match export_python_ast(&source) {
            Ok(ast_json) => ast_json,
            Err(error) => {
                println!("\nFile: {}\n  Scan error: {}", file.display(), error);
                scan_errors += 1;
                continue;
            }
        };
        let module = match parse_python_ast(&ast_json) {
            Ok(module) => module,
            Err(error) => {
                println!("\nFile: {}\n  Scan error: {}", file.display(), error);
                scan_errors += 1;
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
    if scan_errors > 0 {
        return Err(anyhow::anyhow!(
            "scan completed with {} unsupported or unreadable file(s); review the diagnostics above",
            scan_errors
        ));
    }
    Ok(())
}

fn collect_python_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let entries =
            fs::read_dir(&path).map_err(|error| path_io_error("read directory", &path, error))?;
        for entry in entries {
            let entry =
                entry.map_err(|error| path_io_error("inspect directory entry", &path, error))?;
            let child = entry.path();
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

fn canonicalize_existing_path(input: &Path, label: &str) -> Result<PathBuf> {
    let canonical = fs::canonicalize(input).map_err(|error| path_io_error(label, input, error))?;
    Ok(normalize_windows_path(canonical))
}

fn path_io_error(operation: &str, path: &Path, error: std::io::Error) -> anyhow::Error {
    match error.kind() {
        std::io::ErrorKind::PermissionDenied => anyhow::anyhow!(
            "[Tarvos Doctor] Access Denied. Please ensure your terminal environment is running with administrative elevation (Run as Administrator)."
        ),
        std::io::ErrorKind::NotFound => anyhow::anyhow!(
            "[Tarvos Doctor] Path Not Found while attempting to {}: {}. Verify the exact absolute path and directory structure.",
            operation,
            path.display()
        ),
        _ => anyhow::anyhow!(
            "failed to {} '{}': {}",
            operation,
            path.display(),
            error
        ),
    }
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
            eprintln!(
                "Running with the local Python runtime ({})",
                summarize_native_error(&native_error)
            );
            let result = prepare_compatibility_run(args)?;
            return execute_native_run(&result.0, &result.1);
        }
    };
    execute_native_run(&exe_path, &runtime_args)
}

fn prepare_compatibility_run(args: &[String]) -> Result<(PathBuf, Vec<String>)> {
    let input_file = args
        .first()
        .ok_or_else(|| anyhow::anyhow!("usage: tarvos run <file.py> [args...]"))?;
    let working_dir = env::current_dir()?;
    let input_path = secure_input_path(input_file, &working_dir)?;
    let source = fs::read(&input_path)
        .with_context(|| format!("failed to read {}", input_path.display()))?;
    let cache_dir = tarvos_cache_dir()?.join("runs");
    fs::create_dir_all(&cache_dir)
        .with_context(|| format!("failed to create {}", cache_dir.display()))?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "tarvos-compat-run-v1".hash(&mut hasher);
    input_path
        .canonicalize()
        .unwrap_or_else(|_| input_path.clone())
        .to_string_lossy()
        .hash(&mut hasher);
    source.hash(&mut hasher);
    let exe_path = cache_dir.join(format!(
        "{}-compat-{:016x}{}",
        input_path.file_stem().unwrap_or_default().to_string_lossy(),
        hasher.finish(),
        if cfg!(windows) { ".exe" } else { "" }
    ));
    if exe_path.is_file() {
    } else {
        let rust_source = compatibility_launcher_source(&input_path)?;
        compile_rust_binary_opt(&exe_path, &rust_source, true)?;
    }
    Ok((exe_path, args[1..].to_vec()))
}

fn warn_native_fallback(error: &anyhow::Error) {
    eprintln!(
        "Native subset unavailable ({}); emitting a compatibility launcher that requires Python at execution time.",
        summarize_native_error(error)
    );
    eprintln!("Native diagnostic: {error:#}");
}

fn summarize_native_error(error: &anyhow::Error) -> String {
    let text = error.to_string();
    if let Some((_, details)) = text.split_once("source requires unsupported native Python syntax:")
    {
        let count = details
            .lines()
            .filter(|line| line.starts_with("unsupported "))
            .count();
        return if count == 0 {
            "dynamic Python features detected".to_string()
        } else {
            format!("{count} dynamic Python construct(s) detected")
        };
    }
    text.lines()
        .next()
        .unwrap_or("native compilation failed")
        .to_string()
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
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1")
        .arg(&input_path)
        .args(&args[1..])
        .current_dir(&working_dir)
        .status()
        .with_context(|| format!("failed to execute Python program {}", input_path.display()))?;
    // Forward the program's own exit status. Returning a generic error made
    // every non-zero exit look like a Tarvos failure, so a caller could not
    // tell a program that returned 3 from a program that would not start.
    if let Some(code) = status.code() {
        std::process::exit(code);
    }
    if !status.success() {
        return Err(anyhow::anyhow!(
            "Python compatibility runtime was terminated by a signal"
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

    match toolchain::resolve(false) {
        Ok(found) => {
            println!(
                "  [✓] Toolchain (managed): {} {} ({})",
                found.mode.label(),
                found.version,
                found.target
            );
            println!("      source: {}", found.source);
        }
        Err(error) => {
            println!("  [!] Toolchain (managed): unavailable");
            println!("      {error}");
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
                "  [✓] Rust Compiler (legacy system discovery): {} ({})",
                cmd.display(),
                ver_str
            );
        }
        None => {
            println!("  [!] Rust Compiler (legacy system discovery): Not detected (source-only mode available, auto-bootstrap via rustup supported)");
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

    // The local AI capability is optional, so it is reported and never required.
    // `doctor` stays read-only: probing must not start or stop a service.
    print_ai_capability_status();

    println!("\nDiagnostics complete: All core capabilities verified.\n");
    Ok(())
}

/// Directory holding Tarvos state that must outlive the build cache.
///
/// Follows the same resolution as [`tarvos_cache_dir`] — `USERPROFILE` on
/// Windows, `HOME` elsewhere — so state is per-user and never needs elevation.
fn ai_state_dir() -> Result<PathBuf> {
    let home = if cfg!(windows) {
        env::var_os("USERPROFILE")
    } else {
        env::var_os("HOME")
    }
    .map(PathBuf::from)
    .ok_or_else(|| anyhow::anyhow!("could not determine the current user's home directory"))?;
    Ok(home.join(".tarvos"))
}

fn ai_state_path() -> Result<PathBuf> {
    Ok(ai_state_dir()?.join("ai-capability.json"))
}

/// Render the capability report.
///
/// A missing capability is stated plainly rather than dressed up as a failure:
/// the compiler is fully functional without it, and a user should not go
/// looking for a broken install that is working exactly as intended.
fn ai_status_lines(capability: &ai_probe::AiCapability) -> Vec<String> {
    let mut lines = Vec::new();
    if capability.is_usable() {
        lines.push("  [✓] Local AI (Ollama): available (optional)".to_string());
    } else {
        lines.push("  [-] Local AI (Ollama): not available (optional; not required)".to_string());
    }
    lines.push(format!("      endpoint: {}", capability.endpoint));
    if let Some(path) = &capability.binary_path {
        lines.push(format!("      binary: {}", path.display()));
    }
    lines.push(format!(
        "      ownership: {}",
        ai_probe::describe_ownership(capability.ownership)
    ));
    if capability.models.is_empty() {
        lines.push("      models: none reported".to_string());
    } else {
        lines.push(format!("      models: {}", capability.models.join(", ")));
    }
    if let Some(note) = &capability.note {
        lines.push(format!("      note: {note}"));
    }
    lines
}

/// Probe the capability and refresh the recorded state, reporting nothing when
/// state cannot be written.
fn print_ai_capability_status() {
    let Ok(path) = ai_state_path() else {
        return;
    };
    let previous = ai_probe::AiState::read(&path);
    let capability = ai_probe::probe();
    for line in ai_status_lines(&capability) {
        println!("{line}");
    }
    if let Err(error) = ai_probe::AiState::from_probe(&capability, previous.as_ref()).write(&path) {
        eprintln!(
            "      [!] could not record capability state at {}: {error}",
            path.display()
        );
    }
}

/// `tarvos ai-status` — report the optional local AI capability.
///
/// Exits successfully whether or not the capability exists: it is optional, and
/// the installer calls this to record state, so a non-zero status here would
/// wrongly signal a failed installation.
pub(crate) fn ai_status_mode(json: bool) -> Result<()> {
    let path = ai_state_path()?;
    let previous = ai_probe::AiState::read(&path);
    let capability = ai_probe::probe();
    let state = ai_probe::AiState::from_probe(&capability, previous.as_ref());

    if json {
        let report = serde_json::json!({
            "schema": state.schema,
            "ownership": state.ownership,
            "usable": capability.is_usable(),
            "binary_path": state.binary_path,
            "endpoint": state.endpoint,
            "service_reachable": state.service_reachable,
            "models": state.models,
            "note": capability.note,
            "checked_at_unix": state.checked_at_unix,
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("=== Tarvos Local AI Capability ===\n");
        for line in ai_status_lines(&capability) {
            println!("{line}");
        }
        println!("\n  This capability is optional. Tarvos compiles without it.");
    }

    // A read-only probe must not fail the command just because the state file
    // could not be written (read-only home, locked-down environment).
    if let Err(error) = state.write(&path) {
        eprintln!(
            "  [!] could not record capability state at {}: {error}",
            path.display()
        );
    }
    Ok(())
}

fn transpile_python_to_rust(input_path: &Path) -> Result<String> {
    let cache_dir = tarvos_cache_dir()?;
    fs::create_dir_all(&cache_dir)
        .with_context(|| format!("failed to create cache directory {}", cache_dir.display()))?;

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    // The epoch ties every cached verdict (including "unsupported") to the build that
    // produced it. Without it, a dynamic-python verdict written before a compiler fix
    // would keep forcing the CPython fallback long after the gap was closed.
    native_cache_epoch().hash(&mut hasher);
    input_path
        .canonicalize()
        .unwrap_or_else(|_| input_path.to_path_buf())
        .to_string_lossy()
        .hash(&mut hasher);
    hash_project_sources(
        input_path.parent().unwrap_or_else(|| Path::new(".")),
        &mut hasher,
    )?;
    let cache_path = cache_dir.join(format!("{:016x}.rs", hasher.finish()));
    let unsupported_path = cache_path.with_extension("unsupported");

    if cache_path.is_file() {
        println!("Using cached native translation: {}", cache_path.display());
        return fs::read_to_string(&cache_path).with_context(|| {
            format!("failed to read cached translation {}", cache_path.display())
        });
    }
    if unsupported_path.is_file() {
        return Err(anyhow::anyhow!(
            "cached native translation unavailable: dynamic Python features detected"
        ));
    }

    let rust_source = match CompilePipeline::transpile_file(input_path) {
        Ok(source) => source,
        Err(error) => {
            fs::write(
                &unsupported_path,
                format!("unsupported by Tarvos build {}", native_cache_epoch()),
            )
            .with_context(|| {
                format!(
                    "failed to write compatibility cache {}",
                    unsupported_path.display()
                )
            })?;
            return Err(error);
        }
    };
    fs::write(&cache_path, &rust_source).with_context(|| {
        format!(
            "failed to write cached translation {}",
            cache_path.display()
        )
    })?;
    println!("Cached native translation: {}", cache_path.display());
    Ok(rust_source)
}

fn is_dynamic_native_error(error: &anyhow::Error) -> bool {
    let text = error.to_string();
    text.contains("source requires unsupported native Python syntax")
        || text.contains("cached native translation unavailable")
        || text.contains("dynamic type in native")
        || text.contains("unsupported feature")
}

fn compatibility_launcher_source(input_path: &Path) -> Result<String> {
    let source_path = input_path
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", input_path.display()))?;
    let mut source_text = source_path.to_string_lossy().into_owned();
    if source_text.starts_with(r"\\?\") {
        source_text = source_text[4..].to_string();
    }
    let source_literal = serde_json::to_string(&source_text.replace('\\', "/"))
        .context("failed to encode compatibility source path")?;
    Ok(format!(
        r#"use std::process::Command;

fn main() {{
    let source = {source_literal};
    let mut command = Command::new("python");
    command.arg(source).args(std::env::args().skip(1));
    command.env("PYTHONIOENCODING", "utf-8").env("PYTHONUTF8", "1");
    let status = command.status().expect("failed to start Python compatibility runtime");
    std::process::exit(status.code().unwrap_or(1));
}}
"#
    ))
}

/// Identity of this Tarvos build for translation-cache keys.
///
/// Hashing the version plus the running executable's size, modification time,
/// *and contents* means every rebuild invalidates cached translations and
/// cached "unsupported" verdicts, so a newly supported construct is retried
/// instead of reusing the old answer. Contents matter because a fast relink
/// can preserve size and leave `mtime` granularity unchanged, which previously
/// reused a stale translation that predated the fix.
fn native_cache_epoch() -> String {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    env!("CARGO_PKG_VERSION").hash(&mut hasher);
    if let Ok(executable) = std::env::current_exe() {
        if let Ok(metadata) = fs::metadata(&executable) {
            metadata.len().hash(&mut hasher);
            if let Ok(modified) = metadata.modified() {
                modified.hash(&mut hasher);
            }
        }
        // Content hash: bounded read keeps startup fast while guaranteeing a
        // rebuilt binary never reuses the previous build's cached verdicts.
        if let Ok(bytes) = fs::read(&executable) {
            const EPOCH_SAMPLE: usize = 1 << 20;
            let head = bytes.len().min(EPOCH_SAMPLE);
            bytes[..head].hash(&mut hasher);
            if bytes.len() > EPOCH_SAMPLE {
                bytes[bytes.len() - EPOCH_SAMPLE..].hash(&mut hasher);
            }
        }
    }
    format!("{:016x}", hasher.finish())
}

fn tarvos_cache_dir() -> Result<PathBuf> {
    let home = if cfg!(windows) {
        env::var_os("USERPROFILE")
    } else {
        env::var_os("HOME")
    }
    .map(PathBuf::from)
    .ok_or_else(|| anyhow::anyhow!("could not determine the current user's home directory"))?;
    Ok(home
        .join(".tarvos")
        .join("cache")
        .join("tarvos-cache-v1.0-r12"))
}

fn hash_project_sources(root: &Path, hasher: &mut impl Hasher) -> Result<()> {
    let mut files = Vec::new();
    collect_python_sources(root, &mut files)?;
    files.sort();
    for path in files {
        path.to_string_lossy().hash(hasher);
        let content = fs::read(&path)
            .with_context(|| format!("failed to read project source {}", path.display()))?;
        content.hash(hasher);
    }
    Ok(())
}

fn collect_python_sources(root: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if !root.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(root)
        .with_context(|| format!("failed to read project directory {}", root.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if path.is_dir()
            && !matches!(
                name,
                ".git" | ".venv" | "target" | "__pycache__" | "node_modules"
            )
        {
            collect_python_sources(&path, files)?;
        } else if path.is_file() && path.extension().and_then(|value| value.to_str()) == Some("py")
        {
            files.push(path);
        }
    }
    Ok(())
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
    compile_rust_binary_opt(output_path, rust_source, false)
}

fn compile_rust_binary_opt(output_path: &Path, rust_source: &str, fast_dev: bool) -> Result<()> {
    compile_rust_binary_toolchain(output_path, rust_source, fast_dev, false)
}

/// Compile with an explicitly resolved toolchain.
///
/// `prefer_system` is only true for `--system-rust`. The managed path never
/// consults PATH, and the system path never silently receives a managed
/// compiler: the two resolutions share no code that could mix them up.
fn compile_rust_binary_toolchain(
    output_path: &Path,
    rust_source: &str,
    fast_dev: bool,
    prefer_system: bool,
) -> Result<()> {
    let resolved = toolchain::resolve(prefer_system).or_else(|managed_error| {
        if prefer_system {
            // An explicit mode never degrades into another one.
            Err(managed_error)
        } else {
            // Reaching here means the managed toolchain is absent, so PATH is
            // consulted. That contradicts the contract stated above and used to
            // happen without a word: the build then continued against whatever
            // `rustc` happened to be installed, which on an old machine surfaced
            // as `unexpected argument '-C' found` from rustup — an error naming
            // a program the user never asked for. The fallback is kept because a
            // developer with a working system Rust should still be able to build,
            // but it is announced, and a compiler too old to accept the flags
            // this command passes is rejected before it is used.
            match ensure_rust_toolchain().map(toolchain::legacy_system_toolchain) {
                Ok(legacy) => {
                    eprintln!(
                        "Warning: no managed toolchain; falling back to {} ({}).",
                        legacy.source, legacy.version
                    );
                    eprintln!(
                        "Warning: this is NOT the pinned toolchain. Run \
                         `tarvos toolchain --install` for a reproducible build."
                    );
                    Ok(legacy)
                }
                Err(legacy_error) => Err(anyhow::anyhow!(
                    "{managed_error}\nSystem Rust could not be used either: {legacy_error}\nRun \
                     `tarvos toolchain --install` once for the managed compiler."
                )),
            }
        }
    })?;
    let rustc = resolved.rustc;
    println!(
        "Toolchain: {} ({} {})",
        resolved.mode.label(),
        resolved.version,
        resolved.target
    );
    let cache_dir = tarvos_cache_dir()?.join("rustc");
    fs::create_dir_all(&cache_dir).with_context(|| {
        format!(
            "failed to create user cache directory {}",
            cache_dir.display()
        )
    })?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    rust_source.hash(&mut hasher);
    output_path.to_string_lossy().hash(&mut hasher);
    let rust_file = cache_dir.join(format!("{:016x}.rs", hasher.finish()));
    fs::write(&rust_file, rust_source)
        .with_context(|| format!("failed to write {}", rust_file.display()))?;

    let mut cmd = Command::new(&rustc);
    if fast_dev {
        cmd.arg("-C")
            .arg("opt-level=3")
            .arg("-C")
            .arg("lto=thin")
            .arg("-C")
            .arg("codegen-units=1")
            .arg("-C")
            .arg("strip=symbols");
    } else {
        cmd.arg("-C")
            .arg("opt-level=3")
            .arg("-C")
            .arg("lto=fat")
            .arg("-C")
            .arg("codegen-units=1")
            .arg("-C")
            .arg("strip=symbols");
        // `target-cpu=native` used to be passed here. It tunes the generated code
        // for whichever CPU happened to run the build, so a binary produced on a
        // recent processor died with SIGILL on an older one. A build tool cannot
        // know the machine a program will run on, so the baseline is used and the
        // result stays portable across processors of the same architecture. The
        // release workflow already took this position for the same reason; a
        // `tarvos build` output was therefore *less* portable than the release
        // binary of the same commit.
    }

    let status = cmd
        .arg("-C")
        .arg(if rust_source.contains("catch_unwind") {
            "panic=unwind"
        } else {
            "panic=abort"
        })
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

/// The channel this project asks for.
///
/// A hard-coded `stable` ignores `rust-toolchain.toml` and `RUSTUP_TOOLCHAIN`,
/// so the CLI would try to install a different compiler than the one the project
/// is written and tested against. It also fails outright wherever rustup cannot
/// self-update, such as a CI runner that ships a pinned toolchain.
fn project_toolchain_channel() -> Option<String> {
    if let Ok(channel) = env::var("RUSTUP_TOOLCHAIN") {
        let channel = channel.trim().to_string();
        if !channel.is_empty() {
            return Some(channel);
        }
    }
    // `rustup show active-toolchain` already honours rust-toolchain.toml and the
    // RUSTUP_TOOLCHAIN override, so it is the single source of truth.
    for tool in ["rustup", "rustup.exe"] {
        let path = which_simple(tool)
            .ok()
            .flatten()
            .unwrap_or_else(|| PathBuf::from(tool));
        if !path.exists() {
            continue;
        }
        let Ok(output) = Command::new(&path)
            .args(["show", "active-toolchain"])
            .output()
        else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        let first = String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .unwrap_or_default()
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string();
        if !first.is_empty() {
            return Some(first);
        }
    }
    None
}

fn ensure_rust_toolchain() -> Result<PathBuf> {
    if let Some(path) = find_rustc_command() {
        return Ok(path);
    }

    // Install the channel the project actually wants rather than assuming
    // `stable`, so a pinned toolchain is not silently replaced by another.
    let channel = project_toolchain_channel().unwrap_or_else(|| "stable".to_string());
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
            .args(["toolchain", "install", &channel, "--profile", "minimal"])
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
        "Rust is not installed and the '{channel}' toolchain could not be \
         bootstrapped. Install Rust, or point RUSTC at a compiler. Source-only \
         mode is still available via `tarvos compile <file.py> output.rs`."
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
    let cache_dir = tarvos_cache_dir()?;
    let cache_root = cache_dir
        .parent()
        .ok_or_else(|| anyhow::anyhow!("could not determine Tarvos cache root"))?;
    if cache_root.exists() {
        for entry in fs::read_dir(cache_root)
            .with_context(|| format!("failed to read user cache root {}", cache_root.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            let is_tarvos_cache = path.is_dir()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("tarvos-cache-"));
            if is_tarvos_cache {
                fs::remove_dir_all(&path)
                    .with_context(|| format!("failed to remove user cache {}", path.display()))?;
                println!("Removed {}", path.display());
            }
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

    let is_dir = input_path.is_dir();
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

    if is_dir {
        // Multi-file Python project export: find entrypoint and transpile all Python files
        let entrypoint = if input_path.join("main.py").exists() {
            input_path.join("main.py")
        } else if input_path.join("app.py").exists() {
            input_path.join("app.py")
        } else {
            // Find first .py file
            let mut found = None;
            for entry in fs::read_dir(&input_path)? {
                let p = entry?.path();
                if p.extension().and_then(|s| s.to_str()) == Some("py") {
                    found = Some(p);
                    break;
                }
            }
            found.ok_or_else(|| {
                anyhow::anyhow!("No Python files found in {}", input_path.display())
            })?
        };

        // Transpile main entry
        let main_rust = transpile_python_to_rust(&entrypoint)?;
        fs::write(src_dir.join("main.rs"), &main_rust)?;

        // Transpile all other .py files as modules or library files
        for entry in fs::read_dir(&input_path)? {
            let p = entry?.path();
            if p.is_file()
                && p.extension().and_then(|s| s.to_str()) == Some("py")
                && p != entrypoint
            {
                let stem = p.file_stem().unwrap_or_default().to_string_lossy();
                let mod_name = sanitize_package_name(&stem);
                if let Ok(mod_rust) = transpile_python_to_rust(&p) {
                    fs::write(src_dir.join(format!("{}.rs", mod_name)), &mod_rust)?;
                }
            }
        }
        // Also copy non-code project assets
        let _ = copy_project_assets(&input_path, &output_path.join("assets"), &entrypoint);
    } else {
        let rust_source = transpile_python_to_rust(&input_path)?;
        fs::write(src_dir.join("main.rs"), &rust_source)
            .with_context(|| format!("failed to write Rust source in {}", src_dir.display()))?;
    }

    let cargo_toml = format!(
        "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[profile.release]\nopt-level = 3\nlto = true\ncodegen-units = 1\npanic = \"abort\"\n\n[dependencies]\n",
        project_name
    );
    fs::write(output_path.join("Cargo.toml"), cargo_toml)
        .with_context(|| format!("failed to write Cargo.toml in {}", output_path.display()))?;
    fs::write(
        output_path.join("README.md"),
        format!("# Exported Tarvos Project: {}\n\nThis is a complete, native Rust cargo project generated by `tarvos export`.\n\n### Build and Run:\n```bash\ncargo build --release\ncargo run --release\n```\n", project_name),
    )
    .with_context(|| format!("failed to write README in {}", output_path.display()))?;

    println!(
        "Exported complete Rust project to: {}",
        output_path.display()
    );
    println!("Cargo structure: Cargo.toml, src/main.rs, modules, README.md");
    println!(
        "Build with: cd {} && cargo build --release",
        output_path.display()
    );
    Ok(())
}

pub(crate) fn analyze_mode(args: &[String]) -> Result<()> {
    let input = args
        .first()
        .ok_or_else(|| anyhow::anyhow!("usage: tarvos analyze <file.py|project-dir>"))?;
    let path = PathBuf::from(input);

    // A directory is analyzed module by module. Reading a directory as a file
    // failed with an opaque "Access is denied", so `tarvos analyze ./project`
    // did not work even though scanning and packaging a project both do.
    if path.is_dir() {
        let files = collect_python_files(&path)?;
        if files.is_empty() {
            return Err(anyhow::anyhow!(
                "no Python files found under {}",
                path.display()
            ));
        }
        println!("Project: {}", path.display());
        println!("Python files: {}", files.len());
        for file in &files {
            analyze_single(file)?;
        }
        return Ok(());
    }

    if !path.is_file() {
        return Err(anyhow::anyhow!(
            "analyze path is neither a Python file nor a directory: {}",
            path.display()
        ));
    }
    analyze_single(&path)
}

/// Analyze one Python file and print its statistics.
fn analyze_single(path: &Path) -> Result<()> {
    let source =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let ast_json = export_python_ast(&source)?;
    let module = parse_python_ast(&ast_json)?;
    let stats = analyze_module(&module);

    println!();
    println!("Input: {}", path.display());
    println!("{stats}");
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

    let caller_dir = env::current_dir()?;
    let repo_root = if env::var_os("TARVOS_SANDBOX").is_some() {
        caller_dir.clone()
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
    };
    let python_script = repo_root.join("benchmarks").join("run_benchmarks.py");
    if !python_script.exists() {
        return Err(anyhow::anyhow!(
            "could not find benchmarks/run_benchmarks.py"
        ));
    }

    let input_path = resolve_readable_path(input_file, &caller_dir)
        .with_context(|| format!("benchmark input: {}", input_file))?;
    let reference_path = if args.get(1).is_some() {
        resolve_readable_path(&reference_file, &caller_dir)
            .with_context(|| format!("benchmark reference: {}", reference_file))?
    } else {
        secure_input_path(&reference_file, &repo_root)?
    };

    // Keep staged inputs inside the repository-safe build root because the
    // compiler intentionally rejects paths outside the current project.
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

/// Print the full help, identical to `tarvos --help`.
///
/// Rendered from the same clap definition as the flag, so the bare invocation
/// can never drift from the documented one.
fn print_help() {
    eprint!("{}", Cli::command().render_long_help());
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

/// Resolve a user-supplied input path.
///
/// The path is canonicalized so relative paths, `.` segments, and symlinks
/// resolve to one unambiguous location.
///
/// This deliberately does NOT confine the result to the current directory.
/// The argument is typed by the user, so refusing a path they explicitly named
/// ("tarvos compile /opt/app/main.py" from anywhere else) is a usability bug,
/// not a security control: there is no untrusted input here. The boundary that
/// does matter — a compiled module escaping its project with `..` — is enforced
/// where the input is actually untrusted, in `tarvos_core::resolve_local_module`.
fn secure_input_path(input: &str, root: &Path) -> Result<PathBuf> {
    let candidate = PathBuf::from(input);
    let absolute = if candidate.is_absolute() {
        candidate
    } else {
        root.join(candidate)
    };
    canonicalize_existing_path(&absolute, "resolve input path")
}

/// Resolve a user-supplied output path and create its parent directory.
///
/// As with `secure_input_path`, the destination is the user's own choice and is
/// not confined to the current directory. The parent is created when missing so
/// `tarvos build app.py -o dist/app.exe` works on a fresh checkout.
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

    #[test]
    fn supported_python_snippet_transpiles_to_valid_rust() {
        let snippet = "def add(a: int, b: int) -> int:\n    return a + b\n\nprint(add(2, 3))\n";
        let ast_json = export_python_ast(snippet).expect("ast export should succeed");
        let module = parse_python_ast(&ast_json).expect("ast parsing should succeed");
        let ir = lower_module(&module).expect("lowering should succeed");
        let ir = Optimizer::optimize(&ir).expect("optimizer should succeed");
        let rust = RustCodegen::generate(&ir).expect("codegen should succeed");

        assert!(rust.contains("fn add(a: i64, b: i64) -> i64"));
        assert!(rust.contains("println!(\"{}\", add(2_i64, 3_i64));"));
    }

    #[test]
    fn unsupported_import_produces_explicit_actionable_error() {
        let snippet = "import unknown_dynamic_lib\nprint(1)\n";
        let ast_json = export_python_ast(snippet).expect("ast export should succeed");
        let module = parse_python_ast(&ast_json).expect("ast parsing should succeed");
        let res = lower_module(&module);
        assert!(
            res.is_err(),
            "unsupported import must produce an explicit diagnostic"
        );
        let err_msg = res.unwrap_err().to_string();
        assert!(
            err_msg.contains("unsupported") || err_msg.contains("import"),
            "expected diagnostic about unsupported imports, got: {}",
            err_msg
        );
    }

    #[test]
    fn incompatible_type_reassignment_fails_fast_without_success_shape() {
        let snippet = "x = 42\nx = 'now_a_string'\nprint(x)\n";
        let ast_json = export_python_ast(snippet).expect("ast export should succeed");
        let module = parse_python_ast(&ast_json).expect("ast parsing should succeed");
        let ir = lower_module(&module).expect("lowering produces typed IR");
        let ir = Optimizer::optimize(&ir).expect("optimizer succeeds");
        let res = RustCodegen::generate(&ir);
        assert!(
            res.is_err(),
            "incompatible reassignment must be rejected natively by codegen"
        );
        let err_msg = res.unwrap_err().to_string();
        assert!(
            err_msg.contains("fallback")
                || err_msg.contains("incompatible")
                || err_msg.contains("changes from"),
            "expected actionable type error, got: {}",
            err_msg
        );
    }
}
