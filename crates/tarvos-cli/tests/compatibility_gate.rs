use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn temporary_project(name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after Unix epoch")
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("tarvos-cli-{name}-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&path).expect("create CLI compatibility test directory");
    path
}

fn write_program(project: &PathBuf, source: &str) -> PathBuf {
    let input = project.join("main.py");
    fs::write(&input, source).expect("write Python compatibility test");
    input
}

fn run_tarvos(project: &PathBuf, args: &[&str]) -> std::process::Output {
    let binary = env!("CARGO_BIN_EXE_tarvos");
    Command::new(binary)
        .current_dir(project)
        .args(args)
        .output()
        .expect("run Tarvos CLI")
}

fn cleanup(project: PathBuf) {
    let _ = fs::remove_dir_all(project);
}

#[test]
fn supported_program_runs_natively() {
    let project = temporary_project("native");
    write_program(&project, "print(2 + 3)\n");

    let output = run_tarvos(&project, &["run", "main.py", "native-argument"]);

    assert!(
        output.status.success(),
        "native run failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line == "5"),
        "native output did not contain the expected result:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    cleanup(project);
}

#[test]
fn unsupported_native_program_fails_without_success_shape() {
    let project = temporary_project("native-error");
    write_program(&project, "value = 1\nvalue = 'dynamic'\nprint(value)\n");

    let output = run_tarvos(&project, &["run", "main.py"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        !output.status.success(),
        "unsupported native run unexpectedly succeeded"
    );
    assert!(
        stderr.contains("changes from")
            || stderr.contains("fallback")
            || stderr.contains("incompatible"),
        "native failure was not actionable:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        !stdout.contains("Compilation Successful") && !stdout.contains("Build complete"),
        "failed native run emitted success-shaped output:\n{stdout}"
    );
    cleanup(project);
}

#[test]
fn fallback_executes_cpython_and_preserves_runtime_argument() {
    let project = temporary_project("fallback");
    write_program(&project, "import sys\nprint(sys.argv[1])\n");

    let output = run_tarvos(
        &project,
        &["run", "main.py", "--python-fallback", "argument-value"],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "fallback run failed:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.lines().any(|line| line == "argument-value"),
        "CPython did not receive the runtime argument:\n{stdout}"
    );
    assert!(
        stderr.contains("Falling back to CPython"),
        "fallback transition was not reported:\n{stderr}"
    );
    cleanup(project);
}
