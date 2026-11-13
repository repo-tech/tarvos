# Showcase

Every example below was executed on this repository. The command, the expected
output, and the actual output are recorded together, and the actual output is
what the machine really printed. Nothing here is illustrative.

Reproduce any of them with a release build:

```bash
cargo build --release -p tarvos-cli
```

On Windows the binary is `target/release/tarvos.exe`; elsewhere it is
`target/release/tarvos`. The commands below are written for the bare `tarvos`.

---

## 1. Hello world

`hello.py`
```python
print("hello from tarvos")
```

```bash
tarvos build hello.py -o hello.exe
./hello.exe
```

```
hello from tarvos
```

The result is a standalone native executable. It does not need Python or the
Rust toolchain at run time.

---

## 2. Python semantics that Rust does not share

This is the interesting case. Each of these was a defect, and each now matches
CPython.

`semantics.py`
```python
print(-7 % 2)      # Python: 1   (floored; Rust's % gives -1)
print(7 % -2)      # Python: -1
print(7 / 2)       # Python: 3.5 (true division, not 3)
print(10 / 5)      # Python: 2.0
print(-7 // 2)     # Python: -4  (floors; Rust's / truncates toward zero)
print(1 == 1.0)    # Python: True (compares across int and float)
```

```bash
tarvos build semantics.py -o semantics.exe
./semantics.exe
```

```
1
-1
3.5
2.0
-4
True
```

Python's `%` is floored, so the result takes the sign of the divisor. Rust's is
truncated. That difference is the whole reason a Python-to-Rust compiler cannot
simply reuse the host operator.

---

## 3. Control flow: the `elif` bug

`grades.py`
```python
def grade(score):
    if score >= 90:
        return "A"
    elif score >= 80:
        return "B"
    elif score >= 70:
        return "C"
    else:
        return "F"


for s in [95, 85, 75, 50]:
    print(grade(s))
```

```
A
B
C
F
```

Before the fix in `f52a73a`, the compiler emitted each `elif` as an independent
branch followed by an unconditional `else`. The native binary printed **more
than one line per call**. The generated Rust is now a correctly nested chain.

---

## 4. Collections and negative indexing

`lists.py`
```python
xs = [10, 20, 30, 40]
print(xs[0])
print(xs[-1])      # last element
print(xs[-4])      # first element
xs[-1] = 99        # write through a negative index
print(xs)

for v in xs:
    print(v)
```

```
10
40
10
[10, 20, 30, 99]
10
20
30
99
```

`xs[-1]` used to compile to `xs[(-1 as usize)]`, which wraps to `usize::MAX` and
aborts the process. Negative indices now resolve against the length and raise
`IndexError` when genuinely out of range.

---

## 5. A real multi-module project

```
sample-project/
├── main.py
├── helpers.py
└── package/
    ├── __init__.py
    └── math_utils.py
```

`main.py`
```python
from package.math_utils import sum_all, average
from package import math_utils
from helpers import describe


def main():
    values = [3, 1, 4, 1, 5]
    total = sum_all(values)
    print(describe(total))
    print(average(values))
    # `from package import math_utils` binds a module, so this call is qualified.
    print(math_utils.sum_all(values))


main()
```

```bash
tarvos package ./sample-project -o ./sample-rust --entry main.py
./sample-rust/dist/sample_rust.exe
```

```
total=14
2.8
14
```

Both import forms work: the direct one and the qualified `module.f()` one. The
project is compiled through the same parser, lowering, optimizer, and codegen as
a single file; the local modules are inlined before lowering.

This is `benchmarks/project_acceptance.py`, which builds the fixture, compiles
it, runs the artifact, and compares against CPython on every CI run.

---

## 6. Export a reusable Cargo project

```bash
tarvos export ./sample-project ./my-rust-project
cd my-rust-project
cargo build --release
```

The output is a real Cargo project that builds on its own, without Tarvos.

---

## 7. Explicitly not supported

Shown because knowing the boundary matters more than the successes.

```python
import importlib
m = importlib.import_module("os")
```

```
Error: import 'importlib' is not supported by the native backend yet;
supported native modules: math, time, os, os.path, json
```

A dynamic import cannot be resolved statically, so it is reported rather than
guessed at. The full list of boundaries is in
[`COMPATIBILITY.md`](COMPATIBILITY.md); the machine-readable form is
[`compatibility.json`](compatibility.json), which CI verifies against the
differential corpus.

---

## What is verified, and how

| Check | Command | Result |
|---|---|---|
| Compiler gates | `cargo test --workspace` | 116 passed, 0 failed |
| CPython vs native | `python benchmarks/difftest.py` | 11/11 |
| Every CLI command | `python benchmarks/cli_audit.py` | 20/20 |
| Project end to end | `python benchmarks/project_acceptance.py` | 6/6 |
| Build time and size | `python benchmarks/size_benchmark.py` | see [BENCHMARKS.md](BENCHMARKS.md) |

A compiler rejection is recorded as a skip with its reason, never as a pass.
