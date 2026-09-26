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

/// Assert a program ran natively (no CPython fallback) and printed `expected`.
///
/// The fallback warning is the signal that a construct regressed out of the
/// native subset, so it is checked alongside the program's own output.
fn assert_native_output(project: &PathBuf, source: &str, expected: &[&str]) {
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
        stderr.is_empty(),
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
        stderr.is_empty(),
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
        stderr.is_empty(),
        "explicit compatibility mode should be quiet:\n{stderr}"
    );
    cleanup(project);
}
