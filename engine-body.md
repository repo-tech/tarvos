# Tarvos Engine 1.0.0

The first stable public release of Tarvos Engine.

This is not a rebrand of an early preview. It is the point at which the product
has a working compiler, a measured performance result, a documented
compatibility boundary — and, just as importantly, an honest account of what it
still cannot do. Windows and Linux binaries are attached; the macOS binary is
not, and that is stated below rather than glossed over.

## What Tarvos Engine is

A Python-to-native-Rust compiler and the distribution layer that ships it. You
install one thing, you point it at a `.py` file, and you get a standalone native
executable:

- **No Rust installation.** `tarvos toolchain install` fetches and verifies a
  pinned compiler for you. You are not asked to become a Rust developer to use
  a Python compiler.
- **No Python at run time.** The output is a native binary. The machine it runs
  on does not need an interpreter.
- **No administrator rights.** Everything lands under your user profile on
  Windows, `~/.tarvos` on Linux and macOS.
- **No silent fallbacks.** Code outside the supported subset produces a named
  diagnostic, never a quietly different program.

## At a glance

| | |
|---|---|
| **Version line** | `1.0.0`, independent of the `1.1.0-rc.6` compiler it was built from |
| **Platforms** | Windows and Linux binaries attached; macOS buildable but not attached to this release |
| **Feature classification** | 95 features: **52 supported**, 18 partial, 24 unsupported, 1 planned |
| **Measured speedup** | **127.9×** over CPython 3.13.13 on an integer kernel, byte-identical output |
| **Correctness gate** | 21 differential workloads vs CPython: 21 passed, 0 failed, 1 skipped |
| **Works with** | Functions, classes, `try`/`except`, loops, comprehensions, f-strings, `math`, `time`, `os.path`, `statistics`, builtins |
| **Does not work with** | Generators and `yield`, inheritance and `super()`, operator overloading, `dataclasses`, native `re`/`csv`/`datetime`, arbitrary-precision ints, `threading`/`asyncio`, `eval`/`exec` |
| **Not a replacement for** | CPython itself, or the NumPy / Pandas / TensorFlow / PyTorch stack |

If your program depends on a row from the bottom block, `tarvos scan` will tell
you so before you start the port rather than after.

## What is in this release

### Performance, measured

One deterministic integer kernel, 1 warm-up run plus 7 samples, medians
compared, and output byte-compared against CPython:

| Runtime | Median | Min | Max |
|---|---|---|---|
| CPython 3.13.13 | 1713.14 ms | 1445.00 ms | 3326.79 ms |
| **Tarvos** | **13.40 ms** | 11.54 ms | 14.94 ms |
| hand-written Rust 1.98.0 | 29.28 ms | 28.64 ms | 29.65 ms |

**127.9× on the median**, with byte-identical output. Compile time (12.71 s) is
reported separately and is not part of that ratio. The caveats — one workload,
one machine, a noisy CPython baseline, and the workload classes where this does
*not* apply — are in [BENCHMARKS.md](BENCHMARKS.md) and are part of the claim,
not an appendix to it.

### Correctness

Every release is gated on a differential suite that compiles each workload to a
native binary and compares its stdout against CPython. At this release:

| Gate | Result |
|---|---|
| Differential parity | 21 passed, 0 failed, 1 skipped |
| Workspace test suite | 0 failures |
| Capability matrix | 95 features: 52 supported, 18 partial, 24 unsupported, 1 planned |
| Formatting and version gates | pass |

The skipped case is a competitor-comparison harness that imports modules outside
the native subset. It is reported as skipped rather than dropped from the
denominator.

### Supported subset

95 features are classified, and the classification is published rather than
implied: 52 supported, 18 partial, 24 unsupported, 1 planned. The native
standard-library surface includes `math`, `time`, `os.path` (`join`, `basename`,
`dirname`, `exists`, `isfile`, `isdir`), `statistics`, and the Python exception
hierarchy. `tarvos scan ./your-project` tells you where a project stands before
you invest in a migration.

### Installation

| Platform | Method | Needs admin |
|---|---|---|
| Windows | `Tarvos-Setup-Windows-x86_64.exe`, or `install.ps1` | No |
| Linux | `install.sh` with `tarvos-linux-x86_64` | No |
| Any | `pip install .` (downloads and verifies the binary) | No |

> **macOS has no binary attached to this release.** The macOS build cannot be
> cross-compiled from Linux or Windows — it needs a macOS SDK and the Xcode
> toolchain — so no `tarvos-darwin-x86_64` asset is attached. The macOS row is
> missing from the table above rather than pointing at a file that is not
> there. Build it on a Mac and attach it, or enable CI, before telling a macOS
> user to run `install.sh`.

Every asset ships with a `.sha256` that the installers verify before writing
anything to disk.

### Documentation you can reach from the terminal

`tarvos --help` now explains itself. It carries a quick start, the environment
variables it reads, its exit codes, and worked examples, and all sixteen
subcommands have their own long help covering what they do, when to reach for
them, and how they fail:

```console
$ tarvos build --help
$ tarvos run --help
$ tarvos benchmark --help
```

Running `tarvos` with no arguments prints that same help, so the summary you get
by accident is never a stale copy of it. If you are not sure whether a module is
worth migrating, `tarvos analyze app.py --hot-functions` ranks the functions that
benefit first, and `tarvos scan ./your-project` reports where the project as a
whole stands.

## Three compiler bugs found while validating this release

All three were found by running the documentation examples and comparing their
output against CPython. They are recorded here because they are the reason to
trust the rest of this document.

- **A tuple assignment inside a loop returned a stale constant.** This loop:

  ```python
  def fib(n: int) -> int:
      a = 0
      b = 1
      for _ in range(n):
          a, b = b, a + b
      return a
  ```

  returned `0` instead of `832040`. The optimizer recorded `a = 0` as a known
  constant and never learned that the tuple assignment rebinds it, so
  `return a` compiled to `return 0_i64`. The binary built, linked, ran, and
  printed a plausible integer.

  It survived because writing the initial values on one line — `a, b = 0, 1` —
  does not trigger it, so the more idiomatic spelling happened to be the working
  one. Fixed, and the workload that reproduces it is now in the differential
  suite so it is checked against CPython rather than only by a unit assertion.

- **A `return` inside an `except` handler produced Rust that did not compile.**
  The `try` body lowers to a labelled block; the handler was still emitted
  against that block after it had closed, producing
  `error[E0426]: use of undeclared label`. Fixed.

- **A zero divisor aborted the process instead of raising a catchable error.**
  `a // b` with `b == 0` lowered to `.checked_div(..).expect("ZeroDivisionError")`,
  which kills the native process, so an enclosing `except ZeroDivisionError`
  was unreachable and a program written to recover died instead.

  Float `/` was worse than a crash. It emitted a bare `a / b`, so a zero divisor
  produced `inf` and the program carried on with a silently wrong number. Float
  `//` had no zero check at all and did the same.

  Division now goes through checking helpers that report failure as a
  `Result`, and inside a `try` the failure reaches the handler the same way a
  call to a fallible function does. A zero divisor now matches CPython for
  `//`, `/` on integers, `/` on floats, and `//` on floats.

  ```python
  def safe_div(a: int, b: int) -> int:
      try:
          return a // b
      except ZeroDivisionError:
          return -1

  print(safe_div(10, 2))   # 5
  print(safe_div(1, 0))    # -1
  ```

A parity guarantee is only as strong as the programs fed through it. "20 of 21
passing" says nothing about the shape you never thought to test, and the honest
lesson from this release is that our corpus had a hole in it. Two of these three
bugs were found by writing documentation, not by a test failing.

## Known limits, stated rather than hidden

Exception handling is native and the common cases are correct. What remains is
listed in full in [COMPATIBILITY.md](COMPATIBILITY.md):

- bare `raise` and re-raise inside a handler
- exception chaining, `raise X from Y`
- user-defined exception classes
- traceback formatting on an uncaught exception: the program prints
  `Class: message` and exits 1, because a native binary does not carry the
  source-level frames a Python traceback is made of

The first release of this product was written with these limits published
rather than discovered by users, which is the only reason the claims on this
page are worth reading.

## What this release does not claim

- Not a CPython replacement. Dynamic Python, reflection, metaprogramming, and
  arbitrary third-party imports are outside the native subset.
- Not a NumPy, Pandas, TensorFlow, or PyTorch replacement. If your numeric work
  lives in those libraries, this is the wrong tool.
- Not a speedup for I/O-bound programs. The interpreter was never the
  bottleneck there.
- No comparison against PyPy, Numba, Nuitka, or Codon is claimed: they were not
  installed on the measurement machine, and they are reported as unavailable
  rather than quietly omitted.

## Where this is going

Direction, not a schedule. Dates are deliberately absent: a compiler that
promises a date for a feature it cannot yet type-check fails the same way one
that silently produces a wrong answer.

Every feature below has to clear one bar — a differential test comparing the
compiled binary's output against CPython. It is not "supported" because it
compiles.

**Next, roughly in the order it is worth doing:**

- **Exception completeness** — bare `raise` and re-raise, `raise X from Y`
  chaining, user-defined exception classes, `str(KeyError)` repr quoting, and
  traceback metadata carried into the binary for uncaught exceptions.
- **Classes** — inheritance and `super()`, properties and descriptors,
  operator overloading, and dunder methods lowered onto real Rust traits.
  `dataclasses` is the single most requested gap for anyone porting real code.
- **Iterators and generators** — `yield`, generator expressions, and custom
  `__iter__` / `__next__` lowered to a Rust iterator.
- **Standard library** — `os` beyond `os.path`, and native `re`, `csv`,
  `datetime`, `collections`, and run-time `json.loads` / `json.dumps`. `re` is
  the large one, and a common reason a port stops.
- **Bigger integers** — CPython integers are arbitrary precision. A bignum
  runtime costs the common case to serve a rare one, so the likely answer is a
  compile-time switch rather than one behaviour.
- **Concurrency** — not started. A compute kernel already parallelised across
  processes gains nothing from a compiler that cannot describe concurrency yet.

**Not planned, so you do not wait for it:** a CPython-compatible runtime,
NumPy / Pandas / TensorFlow / PyTorch, GUI and networking toolkits, and runtime
introspection (`eval`, `exec`, `getattr` on unknown names). The reasoning for
each is in [ROADMAP.md](ROADMAP.md).

## Versioning

Tarvos Engine is versioned **independently** of the compiler. The compiler ships
many release candidates; the public distribution is cut when there is
worthwhile. A compiler release does not automatically become a product release
here, and this repository's `VERSION` file is the single source of truth for
the public version line.

The binaries in this release report that line, not the compiler line they were
built from. `tarvos --version` prints `tarvos 1.0.0` on the Windows and Linux
binaries attached here, so a user who installed this release is not told they
are running `1.1.0-rc.6` — a compiler candidate that was never published as a
product. The two lines are separated at build time rather than by a second
hardcoded string, so an ordinary `cargo build` of the compiler still reports
`1.1.0-rc.6` and its own release verification is unaffected.

## Earlier pre-releases

`v1.1.0-rc.1` and `v1.1.0-rc.2` were early pre-releases that tracked the
compiler's release candidates. They are superseded by this release, which
replaces their tag scheme with an independent one.

## Documentation

| Document | Covers |
|---|---|
| [COMPATIBILITY.md](COMPATIBILITY.md) | Product, subset, and the exact boundary of every feature |
| [ROADMAP.md](ROADMAP.md) | Product direction and what is deliberately out of scope |
| [BENCHMARKS.md](BENCHMARKS.md) | Method, measured matrix, how to read a result |
| [EXAMPLES.md](EXAMPLES.md) | Worked examples, each verified against CPython |
| [SHOWCASE.md](SHOWCASE.md) | One kernel, end to end |

This repository intentionally does not contain the compiler source.
