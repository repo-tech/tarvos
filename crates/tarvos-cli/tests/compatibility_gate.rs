use std::{
    fs,
    path::{Path, PathBuf},
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

fn write_program(project: &Path, source: &str) -> PathBuf {
    let input = project.join("main.py");
    fs::write(&input, source).expect("write Python compatibility test");
    input
}

fn run_tarvos(project: &Path, args: &[&str]) -> std::process::Output {
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

/// Assert a program ran natively (no CPython fallback) and printed `expected`.
///
/// The fallback warning is the signal that a construct regressed out of the
/// native subset, so it is checked alongside the program's own output.
fn assert_native_output(project: &Path, source: &str, expected: &[&str]) {
    write_program(project, source);
    let output = run_tarvos(project, &["run", "main.py"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "native run failed:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("local Python runtime"),
        "expected native execution but Tarvos fell back to CPython:\n{stderr}"
    );
    for line in expected {
        assert!(
            stdout.lines().any(|actual| actual == *line),
            "expected output line {line:?} was missing:\n{stdout}"
        );
    }
}

/// Whether a run fell back instead of compiling natively.
///
/// A fallback announces itself on stderr, so that is what these tests read. They
/// cannot assert stderr is *empty*: a Linux or macOS CI runner has no managed
/// toolchain installed, and the resolver says so in exactly this channel before
/// finding the pinned one under the runner image. That warning is the toolchain
/// working correctly, and treating any stderr as a fallback was failing these
/// gates on every non-Windows runner.
///
/// Matching the fallback markers rather than emptiness means a fallback message
/// added later still fails the gate, while an unrelated diagnostic no longer
/// hides the real reason a program was not native.
fn fell_back(stderr: &str) -> bool {
    const MARKERS: &[&str] = &[
        "local Python runtime",
        "compatibility runtime",
        "failed to generate Rust",
    ];
    MARKERS.iter().any(|marker| stderr.contains(marker))
}

#[test]
fn range_with_positive_step_uses_native_step_by() {
    // `step_by` takes a `usize`, so a three-argument `range` must convert. A
    // positive literal keeps the loop allocation-free.
    let project = temporary_project("range-positive-step");
    assert_native_output(
        &project,
        "def total(limit, stride):\n\
         \x20   acc = 0\n\
         \x20   for i in range(0, limit - stride, stride):\n\
         \x20       acc += i\n\
         \x20   return acc\n\
         \x20\n\
         print(total(100, 7))\n\
         print(total(50, 5))\n",
        &["637", "180"],
    );
    cleanup(project);
}

#[test]
fn range_with_negative_and_dynamic_step_matches_python() {
    // A negative step counts *down* in Python. Casting it to `usize` would make
    // the range empty rather than descending, so these must take the exact
    // runtime path. The dynamic-step cases are not provably positive at compile
    // time and must behave identically.
    let project = temporary_project("range-negative-step");
    assert_native_output(
        &project,
        "def collect(start, stop, step):\n\
         \x20   out = []\n\
         \x20   for i in range(start, stop, step):\n\
         \x20       out.append(i)\n\
         \x20   return out\n\
         \x20\n\
         print(collect(20, 0, -7))\n\
         print(collect(10, -10, -4))\n\
         print(collect(0, 5, 10))\n\
         print(collect(-5, 5, 4))\n\
         print(collect(5, 0, 1))\n\
         print(collect(0, 10, 3))\n",
        &[
            "[20, 13, 6]",
            "[10, 6, 2, -2, -6]",
            "[0]",
            "[-5, -1, 3]",
            "[]",
            "[0, 3, 6, 9]",
        ],
    );
    cleanup(project);
}

#[test]
fn identical_local_names_in_two_functions_bind_independently() {
    // Each function body is its own scope. Sharing one "already declared" set
    // across functions suppressed the second `let`, so the later function
    // referenced a name that was not in scope.
    let project = temporary_project("per-function-scope");
    assert_native_output(
        &project,
        "def first(limit):\n\
         \x20   total = 0\n\
         \x20   for i in range(limit):\n\
         \x20       total += i\n\
         \x20   return total\n\
         \x20\n\
         def second(limit):\n\
         \x20   total = 1\n\
         \x20   for i in range(limit):\n\
         \x20       total *= 2\n\
         \x20   return total\n\
         \x20\n\
         print(first(5))\n\
         print(second(5))\n",
        &["10", "32"],
    );
    cleanup(project);
}

#[test]
fn empty_list_built_in_a_loop_infers_its_element_type() {
    // `out = []` has no element type of its own; the only evidence is the
    // `append` inside the loop, and the function's return type depends on it.
    let project = temporary_project("empty-list-in-loop");
    assert_native_output(
        &project,
        "def squares(limit):\n\
         \x20   out = []\n\
         \x20   for i in range(limit):\n\
         \x20       out.append(i * i)\n\
         \x20   return out\n\
         \x20\n\
         print(squares(5))\n",
        &["[0, 1, 4, 9, 16]"],
    );
    cleanup(project);
}

#[test]
fn chained_comparison_runs_natively() {
    let project = temporary_project("chained-comparison");
    assert_native_output(
        &project,
        "grade = 'B'\n\
         print('a' <= grade <= 'z')\n\
         print('0' <= grade <= '9')\n\
         print(0 < 5 < 10)\n",
        &["True", "False", "True"],
    );
    cleanup(project);
}

#[test]
fn string_methods_run_natively() {
    let project = temporary_project("string-methods");
    assert_native_output(
        &project,
        "text = '  Hello World  '\n\
         print(text.strip().lower())\n\
         print('a-b-c'.split('-'))\n\
         print(','.join(['x', 'y', 'z']))\n\
         print('abcabc'.count('bc'))\n\
         print('abc'.startswith('ab'))\n\
         print('42'.zfill(5))\n",
        &[
            "hello world",
            "['a', 'b', 'c']",
            "x,y,z",
            "2",
            "True",
            "00042",
        ],
    );
    cleanup(project);
}

#[test]
fn list_methods_run_natively() {
    let project = temporary_project("list-methods");
    assert_native_output(
        &project,
        "values = [3, 1, 2]\n\
         values.append(0)\n\
         values.sort()\n\
         print(values)\n\
         values.reverse()\n\
         print(values)\n\
         print(values.index(2))\n\
         print(values.pop())\n",
        &["[0, 1, 2, 3]", "[3, 2, 1, 0]", "1", "0"],
    );
    cleanup(project);
}

#[test]
fn list_and_sorted_builtins_run_natively() {
    // `list()` materializes each supported iterable (lazy `range()`, `str` by
    // character, an existing list by clone, a homogeneous tuple literal) and
    // `sorted()` materializes then sorts with the element-typed helper. The
    // `dict` case uses `sorted()` because `list(d)` would depend on HashMap
    // iteration order, which differs from Python insertion order.
    let project = temporary_project("list-builtin");
    assert_native_output(
        &project,
        "print(list(range(3)))\n\
         print(list('ab'))\n\
         xs = [3, 1, 2]\n\
         print(list(xs))\n\
         print(sorted(xs))\n\
         print(sorted((2.0, 0.5, 1.5)))\n\
         print(sorted('cba'))\n\
         print(sorted({'b': 1, 'a': 2}))\n\
         print(list((1, 2, 3)))\n\
         print(list())\n",
        &[
            "[0, 1, 2]",
            "['a', 'b']",
            "[3, 1, 2]",
            "[1, 2, 3]",
            "[0.5, 1.5, 2.0]",
            "['a', 'b', 'c']",
            "['a', 'b']",
            "[1, 2, 3]",
            "[]",
        ],
    );
    cleanup(project);
}

#[test]
fn list_over_dict_falls_back_explicitly() {
    // `list(d)` is insertion-ordered in Python but HashMap iteration is not,
    // so it must refuse native compilation with an actionable diagnostic and
    // run through the compatibility runtime instead of mis-ordering keys.
    let project = temporary_project("list-dict-fallback");
    write_program(&project, "d = {'b': 1, 'a': 2}\nprint(list(d))\n");
    let output = run_tarvos(&project, &["run", "main.py"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "compatibility run failed:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("local Python runtime"),
        "list(dict) must use the compatibility runtime:\n{stderr}"
    );
    assert!(
        stdout.lines().any(|line| line == "['b', 'a']"),
        "compatibility output must preserve dict insertion order:\n{stdout}"
    );
    cleanup(project);
}

#[test]
fn element_swap_through_subscripts_runs_natively() {
    let project = temporary_project("subscript-swap");
    assert_native_output(
        &project,
        "values = [1, 2, 3]\n\
         i = 0\n\
         j = 2\n\
         values[i], values[j] = values[j], values[i]\n\
         print(values)\n",
        &["[3, 2, 1]"],
    );
    cleanup(project);
}

#[test]
fn unannotated_parameter_types_are_inferred_from_call_sites() {
    let project = temporary_project("parameter-inference");
    assert_native_output(
        &project,
        "def shout(value):\n\
         \x20   return value.upper()\n\
         \x20\n\
         def total(values):\n\
         \x20   return len(values)\n\
         \x20\n\
         print(shout('tarvos'))\n\
         print(total([1, 2, 3]))\n",
        &["TARVOS", "3"],
    );
    cleanup(project);
}

#[test]
fn empty_list_element_type_comes_from_first_append() {
    let project = temporary_project("empty-list-inference");
    assert_native_output(
        &project,
        "words = []\n\
         for index in range(3):\n\
         \x20   words.append('w' + str(index))\n\
         print(words)\n\
         print(''.join(words))\n",
        &["['w0', 'w1', 'w2']", "w0w1w2"],
    );
    cleanup(project);
}

#[test]
fn bubble_sort_over_string_characters_runs_natively() {
    // The pattern that motivated native string support: a comprehension over a
    // lowered string, a chained-comparison filter, a subscript swap and a join.
    let project = temporary_project("bubble-sort");
    assert_native_output(
        &project,
        "def clean(text):\n\
         \x20   chars = [c for c in text.lower() if 'a' <= c <= 'z']\n\
         \x20   size = len(chars)\n\
         \x20   for i in range(size):\n\
         \x20       for j in range(0, size - i - 1):\n\
         \x20           if chars[j] > chars[j + 1]:\n\
         \x20               chars[j], chars[j + 1] = chars[j + 1], chars[j]\n\
         \x20   return ''.join(chars)\n\
         \x20\n\
         print(clean('Tarvos Compiler'))\n",
        &["aceilmooprrstv"],
    );
    cleanup(project);
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
fn tuple_assignment_runs_natively() {
    let project = temporary_project("tuple-assignment");
    write_program(
        &project,
        "a, b = 1, 2\nprint(a, b)\na, b = b, a\nprint(a, b)\n",
    );

    let output = run_tarvos(&project, &["run", "main.py"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "tuple assignment failed:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.lines().any(|line| line == "1 2") && stdout.lines().any(|line| line == "2 1"),
        "tuple assignment output was incorrect:\n{stdout}"
    );
    cleanup(project);
}

#[test]
fn selected_os_operations_run_natively() {
    let project = temporary_project("os-subset");
    write_program(
        &project,
        "import os\nos.makedirs('nested/child')\nprint(os.path.isdir('nested/child'))\n",
    );

    let output = run_tarvos(&project, &["run", "main.py"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "selected os subset failed:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.lines().any(|line| line == "True"),
        "os output did not use Python boolean spelling:\n{stdout}"
    );
    assert!(
        project.join("nested").join("child").is_dir(),
        "os.makedirs() did not create the nested directory"
    );
    cleanup(project);
}

#[test]
fn list_and_string_repetition_run_natively() {
    let project = temporary_project("repetition");
    write_program(
        &project,
        "values = [1, 2] * 3\nprint(values)\ntext = 'ab' * 2\nprint(text)\n",
    );

    let output = run_tarvos(&project, &["run", "main.py"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "sequence repetition failed:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.lines().any(|line| line == "[1, 2, 1, 2, 1, 2]")
            && stdout.lines().any(|line| line == "abab"),
        "sequence repetition output was incorrect:\n{stdout}"
    );
    cleanup(project);
}

#[test]
fn bitwise_and_shift_program_runs_natively() {
    let project = temporary_project("bitwise");
    write_program(
        &project,
        "state = 123456789\n\
         counter = 0\n\
         for i in range(32):\n\
         \x20   state = (state ^ (i + 1)) * 1103515245 + 12345\n\
         \x20   state = state & 0xFFFFFFFF\n\
         \x20   if state % 2 == 0:\n\
         \x20       counter += 1\n\
         print(counter)\n\
         print(state >> 8)\n\
         print(-7 // 2)\n",
    );

    let output = run_tarvos(&project, &["run", "main.py"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "bitwise run failed:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        !fell_back(&stderr),
        "bitwise program must not fall back to the compatibility runtime:\n{stderr}"
    );
    assert!(
        stdout.lines().any(|line| line == "-4"),
        "Python floor division must be preserved natively:\n{stdout}"
    );
    cleanup(project);
}

#[test]
fn matrix_like_program_runs_natively_without_python() {
    let project = temporary_project("matrix");
    write_program(
        &project,
        "def total(size):\n\
         \x20   grid = [[i + j for j in range(size)] for i in range(size)]\n\
         \x20   for row in range(size):\n\
         \x20       for column in range(size):\n\
         \x20           grid[row][column] += row * column\n\
         \x20   checksum = 0\n\
         \x20   for row in range(size):\n\
         \x20       for column in range(size):\n\
         \x20           checksum += grid[row][column]\n\
         \x20   return checksum\n\
         print(total(10))\n",
    );

    let output = run_tarvos(&project, &["run", "main.py"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "matrix run failed:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        !fell_back(&stderr),
        "matrix program must not fall back to the compatibility runtime:\n{stderr}"
    );
    assert!(
        stdout.lines().any(|line| line == "2925"),
        "matrix checksum was incorrect:\n{stdout}"
    );
    cleanup(project);
}

#[test]
fn unsupported_program_uses_automatic_compatibility_runtime() {
    let project = temporary_project("native-error");
    write_program(&project, "value = 1\nvalue = 'dynamic'\nprint(value)\n");

    let output = run_tarvos(&project, &["run", "main.py"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "compatibility run failed:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.lines().any(|line| line == "dynamic"));
    cleanup(project);
}

/// A program that resolves something relative to its own file must still find it
/// under the fallback.
///
/// The compatibility runtime used to run a *copy* of the source out of
/// `~/.tarvos/cache`, so `__file__` pointed into the cache and the lookup failed
/// while naming the cache path Ã¢â‚¬â€ a local model folder, a config file, or a
/// sibling import all broke the same way. `tarvos run` now hands the interpreter
/// the original file.
#[test]
fn fallback_finds_data_beside_the_original_source() {
    let project = temporary_project("source-relative-data");
    write_program(
        &project,
        "import os\n\
         here = os.path.dirname(os.path.abspath(__file__))\n\
         with open(os.path.join(here, 'data.txt')) as handle:\n\
         \x20   print('data:', handle.read().strip())\n",
    );
    fs::write(project.join("data.txt"), "beside-the-source\n").unwrap();

    let output = run_tarvos(&project, &["run", "main.py"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "fallback run failed:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.lines().any(|line| line == "data: beside-the-source"),
        "the fallback must resolve paths against the original source directory:\n\
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    cleanup(project);
}

/// The fallback must not quietly turn a program's own exit status into a
/// Tarvos failure. A script that calls `sys.exit(3)` has exited 3.
#[test]
fn fallback_passes_through_the_program_exit_status() {
    let project = temporary_project("fallback-exit-status");
    write_program(&project, "import sys\nprint('failing')\nsys.exit(3)\n");

    let output = run_tarvos(&project, &["run", "main.py"]);
    assert_eq!(
        output.status.code(),
        Some(3),
        "the program's own exit code must survive the fallback:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
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
    // This run asks for the fallback explicitly, so falling back is the
    // point rather than a failure. What must not appear is an error on top of
    // that: the toolchain resolving is a warning, but a failed run is not.
    assert!(
        !stderr.contains("Error:") && !stderr.contains("panicked at"),
        "explicit compatibility mode should not also report an error:\n{stderr}"
    );
    cleanup(project);
}
// ---------------------------------------------------------------------------
// Artifact contract: what `tarvos build` produces and what it claims.
// ---------------------------------------------------------------------------

fn build_tarvos(project: &Path, args: &[&str]) -> std::process::Output {
    run_tarvos(project, args)
}

fn stdout_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn artifact_suffix() -> &'static str {
    if cfg!(windows) {
        ".exe"
    } else {
        ""
    }
}

/// A built executable, and the manifest `tarvos build` must leave beside it.
fn built_artifact(project: &Path, stem: &str) -> (PathBuf, serde_json::Value) {
    let artifact = project.join(format!("{stem}{}", artifact_suffix()));
    assert!(artifact.is_file(), "no artifact at {}", artifact.display());
    let manifest = project.join(format!("{stem}{}.tarvos-manifest.json", artifact_suffix()));
    assert!(
        manifest.is_file(),
        "a build must leave a manifest; {} is missing",
        manifest.display()
    );
    let text = fs::read_to_string(&manifest).expect("read artifact manifest");
    let value: serde_json::Value = serde_json::from_str(&text).expect("manifest must be JSON");
    (artifact, value)
}

/// `print("hello")` is the smallest genuinely standalone program.
#[test]
fn a_hello_program_builds_a_native_standalone_executable() {
    let project = temporary_project("artifact-hello");
    write_program(&project, "print('hello')\n");
    let output = build_tarvos(&project, &["build", "main.py"]);
    assert!(
        output.status.success(),
        "build failed:\n{}\n{}",
        stdout_of(&output),
        stderr_of(&output)
    );

    let (artifact, manifest) = built_artifact(&project, "main");
    assert_eq!(manifest["native"], serde_json::json!(true));
    assert_eq!(
        manifest["python_runtime_required"],
        serde_json::json!(false),
        "a native build must not claim it needs Python:\n{manifest}"
    );
    assert_eq!(
        manifest["temporary_python_source_required"],
        serde_json::json!(false),
        "a native build must not reconstruct source at run time:\n{manifest}"
    );
    assert_eq!(
        manifest["external_python_packages"],
        serde_json::json!([]),
        "nothing external may be listed for a program with no imports:\n{manifest}"
    );
    assert_eq!(manifest["rust_runtime_required"], serde_json::json!(false));

    // The header has to match this platform. A file named `.exe` that is not a
    // PE image is the exact artifact this check exists to catch.
    let bytes = fs::read(&artifact).expect("read built artifact");
    if cfg!(windows) {
        assert!(
            bytes.starts_with(b"MZ"),
            "a Windows build must produce a PE executable"
        );
    } else if cfg!(target_os = "macos") {
        assert_eq!(&bytes[..4], &[0xfe, 0xed, 0xfa, 0xcf]);
    } else {
        assert!(
            bytes.starts_with(b"\x7fELF"),
            "a Linux build must produce an ELF executable"
        );
    }

    cleanup(project);
}

/// Running the built program must not leave Python source behind, however many
/// times it runs.
#[test]
fn repeated_execution_never_generates_python_source() {
    let project = temporary_project("artifact-repeat");
    write_program(&project, "x = 2\nprint(x * 21)\n");
    let build = build_tarvos(&project, &["build", "main.py"]);
    assert!(
        build.status.success(),
        "build failed:\n{}\n{}",
        stdout_of(&build),
        stderr_of(&build)
    );
    let (artifact, _) = built_artifact(&project, "main");

    for _ in 0..10 {
        let output = Command::new(&artifact)
            .current_dir(&project)
            .output()
            .expect("run the built executable");
        assert!(
            output.status.success(),
            "the built executable must keep working on repeat runs"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "42");
    }

    let generated: Vec<String> = fs::read_dir(&project)
        .expect("read project directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".py") && name != "main.py")
        .collect();
    assert!(
        generated.is_empty(),
        "native execution must not write Python source; found {generated:?}"
    );

    cleanup(project);
}

/// A package Tarvos cannot lower must stop the build. Producing an executable
/// that quietly needs `flask` installed is the failure this prevents.
#[test]
fn an_unbuildable_dependency_stops_the_build_and_names_the_package() {
    let project = temporary_project("artifact-blocked");
    write_program(&project, "import flask\nprint('hi')\n");
    let output = build_tarvos(&project, &["build", "main.py"]);
    let combined = format!("{}{}", stdout_of(&output), stderr_of(&output));

    assert!(
        !output.status.success(),
        "a build that needs an uninstalled package must not succeed:\n{combined}"
    );
    assert!(
        combined.contains("TARVOS NATIVE COMPILATION BLOCKED"),
        "the refusal must be explicit:\n{combined}"
    );
    assert!(
        combined.contains("flask"),
        "the refusal must name the package to install:\n{combined}"
    );
    assert!(
        !project.join(format!("main{}", artifact_suffix())).exists(),
        "no executable may be produced for a blocked build"
    );

    cleanup(project);
}

/// An unknown name is not an installable package and must not be reported as one.
#[test]
fn an_unknown_import_is_reported_as_unsupported_not_as_installable() {
    let project = temporary_project("artifact-unknown");
    write_program(&project, "import some_unknown_package\nprint(1)\n");
    let output = build_tarvos(&project, &["build", "main.py"]);
    let combined = format!("{}{}", stdout_of(&output), stderr_of(&output));
    assert!(
        combined.contains("UNSUPPORTED"),
        "an unknown module must be classified UNSUPPORTED:\n{combined}"
    );
    assert!(
        !combined.contains("EXTERNAL_RUNTIME"),
        "an unknown module must not be presented as installable:\n{combined}"
    );
    cleanup(project);
}

/// The compatibility launcher stays available, but only by name, and labels
/// itself in both the build output and the manifest.
#[test]
fn the_compatibility_launcher_requires_opt_in_and_declares_itself() {
    let project = temporary_project("artifact-launcher");
    write_program(&project, "import flask\nprint('hi')\n");

    let refused = build_tarvos(&project, &["build", "main.py"]);
    assert!(
        !refused.status.success(),
        "without --compat-launcher nothing may be built"
    );

    let output = build_tarvos(&project, &["build", "main.py", "--compat-launcher"]);
    assert!(
        output.status.success(),
        "--compat-launcher must build:\n{}\n{}",
        stdout_of(&output),
        stderr_of(&output)
    );
    let (_, manifest) = built_artifact(&project, "main");
    assert_eq!(
        manifest["native"],
        serde_json::json!(false),
        "a launcher is not a native binary:\n{manifest}"
    );
    assert_eq!(manifest["python_runtime_required"], serde_json::json!(true));
    assert_eq!(
        manifest["external_python_packages"],
        serde_json::json!(["flask"]),
        "the launcher must record what the target needs:\n{manifest}"
    );

    cleanup(project);
}

/// `tarvos validate-artifact` has to work as a gate in a script.
#[test]
fn validate_artifact_gates_on_whether_python_is_required() {
    let project = temporary_project("artifact-validate");
    write_program(&project, "print('gated')\n");
    let build = build_tarvos(&project, &["build", "main.py"]);
    assert!(build.status.success());
    let (artifact, _) = built_artifact(&project, "main");

    let validated = run_tarvos(&project, &["validate-artifact", artifact.to_str().unwrap()]);
    let text = format!("{}{}", stdout_of(&validated), stderr_of(&validated));
    assert!(
        validated.status.success(),
        "a native artifact must validate:\n{text}"
    );
    for expected in [
        "Native:            YES",
        "Python required:   NO",
        "Temporary .py:     NO",
        "External packages: NONE",
        "Status:            PASS",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in:\n{text}");
    }

    // An artifact with no manifest cannot be vouched for. UNVERIFIED is the
    // honest answer; PASS would be a guess.
    let orphan = project.join("orphan.exe");
    fs::write(&orphan, b"MZ not really a program").expect("write orphan artifact");
    let report = run_tarvos(&project, &["validate-artifact", orphan.to_str().unwrap()]);
    let text = format!("{}{}", stdout_of(&report), stderr_of(&report));
    assert!(report.status.success(), "reported, not failed:\n{text}");
    assert!(
        text.contains("Status:            UNVERIFIED"),
        "an artifact with no manifest must never be reported PASS:\n{text}"
    );

    cleanup(project);
}

/// The default output name follows the input's name and the target's
/// convention, so builds stop overwriting each other and Linux stops being
/// handed a `.exe`.
#[test]
fn the_default_output_name_is_derived_from_the_input() {
    let project = temporary_project("artifact-name");
    fs::write(project.join("greeting.py"), "print('named')\n").expect("write source");
    let output = build_tarvos(&project, &["build", "greeting.py"]);
    assert!(
        output.status.success(),
        "build failed:\n{}\n{}",
        stdout_of(&output),
        stderr_of(&output)
    );
    assert!(
        project
            .join(format!("greeting{}", artifact_suffix()))
            .is_file(),
        "expected greeting{} beside the source",
        artifact_suffix()
    );
    cleanup(project);
}
