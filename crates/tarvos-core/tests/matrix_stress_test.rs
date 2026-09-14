//! Tarvos v1.5.0 integration capability matrix.
//!
//! This is intentionally a diagnostic test rather than a compatibility claim:
//! Tarvos currently compiles a statically analyzable Python subset. Cases that
//! require an import, dynamic object model, or external runtime are reported as
//! fallback boundaries and must never panic the compiler.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use tarvos_core::{find_python_command, CompilePipeline};

#[derive(Debug)]
struct Case {
    name: &'static str,
    source: &'static str,
    expected_native: bool,
    boundary: &'static str,
}

#[test]
fn v1_5_capability_matrix_and_distribution_smoke() {
    let root = unique_temp_dir("tarvos-matrix");
    fs::create_dir_all(root.join("utils")).expect("create utils directory");
    fs::create_dir_all(root.join("core").join("analytics")).expect("create analytics directory");

    let files = [
        (
            PathBuf::from("main.py"),
            "from utils.math_kernel import reduce_sum\nfrom core.analytics.processing import classify\n\nprint(classify(reduce_sum(10)))\n",
        ),
        (
            PathBuf::from("utils").join("math_kernel.py"),
            "def reduce_sum(limit):\n    total = 0\n    for value in range(limit):\n        total += value\n    return total\n",
        ),
        (
            PathBuf::from("core").join("analytics").join("processing.py"),
            "def classify(value):\n    if value > 10:\n        return 1\n    return 0\n",
        ),
    ];
    for (relative, source) in &files {
        fs::write(root.join(relative), source).expect("write virtual project file");
    }

    let discovered = discover_python_files(&root);
    assert_eq!(
        discovered.len(),
        3,
        "recursive project discovery lost a Python node"
    );
    println!(
        "[PASS] nested project discovery: {} Python files",
        discovered.len()
    );

    let layout = render_module_layout(&discovered, &root);
    assert!(layout.contains("pub mod utils;"));
    assert!(layout.contains("pub mod core;"));
    assert!(layout.contains("pub mod analytics;"));
    println!("[INFO] recursive Cargo module layout:\n{layout}");

    let cases = [
        Case {
            name: "native arithmetic kernel",
            source: "total = 0\nfor value in range(100):\n    total += value\nprint(total)\n",
            expected_native: true,
            boundary: "primitive arithmetic and control flow",
        },
        Case {
            name: "NumPy arange/zeros",
            source: "import numpy as np\nvalues = np.arange(10)\nzeros = np.zeros(10)\nprint(values[0] + zeros[0])\n",
            expected_native: false,
            boundary: "external array runtime; no NumPy bridge",
        },
        Case {
            name: "SciPy numerical kernel",
            source: "from scipy import signal\nprint(signal.convolve([1], [1]))\n",
            expected_native: false,
            boundary: "external SciPy runtime; no native bridge",
        },
        Case {
            name: "Pandas record mutation",
            source: "import pandas as pd\nrecords = pd.DataFrame({'x': [1, 2]})\nrecords['x'] = records['x'] + 1\nprint(records)\n",
            expected_native: false,
            boundary: "dynamic dataframe/object model",
        },
        Case {
            name: "scikit-learn validation",
            source: "from sklearn.model_selection import train_test_split\nprint(train_test_split([1, 2, 3]))\n",
            expected_native: false,
            boundary: "third-party estimator/runtime objects",
        },
        Case {
            name: "PyTorch tensor slicing",
            source: "import torch\nmatrix = torch.zeros((2, 2))\nprint(matrix[:, 0])\n",
            expected_native: false,
            boundary: "dynamic tensor runtime",
        },
        Case {
            name: "JSON serialization",
            source: "import json\nprint(json.dumps({'value': 3}))\n",
            expected_native: false,
            boundary: "stdlib import/runtime bridge not implemented",
        },
        Case {
            name: "CSV iteration",
            source: "import csv\nprint(list(csv.reader(['a,b'])))\n",
            expected_native: false,
            boundary: "stdlib import/runtime bridge not implemented",
        },
        Case {
            name: "regular expressions",
            source: "import re\nprint(re.match('a', 'abc'))\n",
            expected_native: false,
            boundary: "dynamic regex object/runtime bridge",
        },
        Case {
            name: "datetime comparison",
            source: "import datetime\nprint(datetime.datetime.now())\n",
            expected_native: false,
            boundary: "datetime runtime object and clock access",
        },
        Case {
            name: "Tkinter/audio/thread boundary",
            source: "import tkinter\nimport threading\nimport simpleaudio\nprint('dynamic desktop runtime')\n",
            expected_native: false,
            boundary: "GUI, audio, and threads require CPython fallback",
        },
    ];

    let python = find_python_command().expect("Python is required for the AST exporter");
    let mut native_success = false;
    for case in cases {
        let filename: String = case
            .name
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character
                } else {
                    '_'
                }
            })
            .collect();
        let source_path = root.join(format!("{filename}.py"));
        fs::write(&source_path, case.source).expect("write matrix source");
        let py_compile = Command::new(&python)
            .args(["-m", "py_compile"])
            .arg(&source_path)
            .status()
            .expect("run Python syntax validation");
        assert!(py_compile.success(), "{} is not valid Python", case.name);

        let result = std::panic::catch_unwind(|| CompilePipeline::transpile_file(&source_path));
        let outcome = match result {
            Ok(Ok(rust_source)) => {
                println!(
                    "[PASS] {:32} native candidate | boundary: {}",
                    case.name, case.boundary
                );
                if !case.expected_native {
                    println!("[INFO] {:32} compiler accepted syntax; runtime support still requires review", case.name);
                }
                if case.expected_native {
                    native_success = run_native_parity(&root, &rust_source, &python);
                }
                "native"
            }
            Ok(Err(error)) => {
                assert!(
                    !case.expected_native,
                    "{} unexpectedly failed: {error:#}",
                    case.name
                );
                println!(
                    "[FALLBACK] {:27} CPython boundary | {}",
                    case.name, case.boundary
                );
                "fallback"
            }
            Err(_) => panic!("{} caused a compiler panic", case.name),
        };
        assert!(!outcome.is_empty());
    }
    assert!(
        native_success,
        "native smoke case did not produce parity-verified output"
    );

    let _ = fs::remove_dir_all(&root);
}

fn run_native_parity(root: &Path, rust_source: &str, python: &str) -> bool {
    let python_source = root.join("native_parity.py");
    fs::write(
        &python_source,
        "total = 0\nfor value in range(100):\n    total += value\nprint(total)\n",
    )
    .expect("write parity source");
    let expected = Command::new(python)
        .arg(&python_source)
        .output()
        .expect("run CPython parity reference");

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("resolve repository root");
    let dist = repo_root.join("target").join("dist");
    fs::create_dir_all(&dist).expect("create target/dist");
    let binary = dist.join(if cfg!(windows) {
        "tarvos-matrix-smoke.exe"
    } else {
        "tarvos-matrix-smoke"
    });
    let rust_file = root.join("native_parity.rs");
    fs::write(&rust_file, rust_source).expect("write generated Rust");

    let rustc = Command::new("rustc")
        .args([
            "--edition",
            "2021",
            "-C",
            "opt-level=3",
            "-C",
            "strip=symbols",
        ])
        .arg(&rust_file)
        .arg("-o")
        .arg(&binary)
        .status()
        .expect("run rustc for distribution smoke test");
    assert!(rustc.success(), "generated Rust did not compile");

    let actual = Command::new(&binary)
        .output()
        .expect("run native smoke binary");
    assert!(actual.status.success(), "native smoke binary failed");
    let expected_text = String::from_utf8_lossy(&expected.stdout)
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let actual_text = String::from_utf8_lossy(&actual.stdout)
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    assert_eq!(
        actual_text, expected_text,
        "CPython/native output parity mismatch"
    );

    let profile_size = fs::metadata(&binary)
        .expect("read native binary metadata")
        .len();
    println!(
        "[PASS] native parity | output={:?} | binary_size={} bytes | flags=-C opt-level=3 -C strip=symbols",
        actual_text.trim(),
        profile_size
    );
    true
}

fn discover_python_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).expect("read project directory") {
            let path = entry.expect("read directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "py") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn render_module_layout(files: &[PathBuf], root: &Path) -> String {
    let mut modules = String::new();
    for file in files {
        let relative = file.strip_prefix(root).expect("strip project root");
        let components: Vec<_> = relative.components().collect();
        for component in components.iter().take(components.len().saturating_sub(1)) {
            let name = component.as_os_str().to_string_lossy();
            let declaration = format!("pub mod {name};");
            if !modules.contains(&declaration) {
                modules.push_str(&declaration);
                modules.push('\n');
            }
        }
    }
    modules
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{}-{timestamp}", std::process::id()))
}
