# Changelog

All notable changes to Tarvos are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow
[Semantic Versioning](https://semver.org/).

## [1.3.0] - 2026-10-03

First stable release. The release candidates accumulated a set of changes that
together make the compiler usable on ordinary Python rather than only on code
written to fit the subset.

### Added

- **Dynamic typing for names that change type.** `x = 0` followed by
  `x = "cecece"` now compiles natively instead of being handed to CPython. A name
  whose type genuinely changes is emitted as a tagged runtime value, so both
  assignments agree on one Rust type. Arithmetic, comparison and concatenation on
  such a name go through a runtime dispatcher that follows Python's numeric tower
  (`int + float` widens, `/` yields a float, dividing by zero raises
  `ZeroDivisionError`), so `x + 'b'` concatenates and `x + 1` raises the same
  `TypeError` CPython raises.

  Only names that actually change type are boxed. A program whose types are
  fixed keeps its native `i64`/`f64`/`String` representation and never pays for
  the tagged-value runtime, which is emitted only when one is reachable.

### Fixed

- **`tarvos run` no longer pays for link-time optimization.** The profile passed
  `lto=thin` and `codegen-units=1`, chosen for producing a small shipped artifact.
  But `run` compiles a program the caller is about to execute, so it optimizes for
  time to first run instead. LTO pays off when a program is dominated by calls
  between separately compiled units; a generated single file that mostly calls
  the standard library gives the linker nothing to fold. Measured on a
  3M-iteration loop with a cold cache, 4.7s to compile and run with LTO against
  4.1s without; the program itself then ran in 0.021s against 0.020s, so the
  runtime was a wash and the ~0.6s came back on the first run and every one after.
  Binary size was 128512 bytes either way. `tarvos build` keeps fat LTO, where
  the artifact is shipped and the compile is paid once for a permanent win.

### Changed

- **The Python package workflow now runs real tests.** It failed with pytest's
  exit code 5, which means "collected nothing": the package shipped a launcher
  that decides which release asset a platform gets and verifies a SHA-256 before
  running it, and neither decision was checked. Both are silent when wrong — a
  wrong asset name means `pip install tarvos` succeeds on every platform and the
  command fails only when first run, on the machine that needed it. Coverage now
  pins the asset mapping per platform, checksum verification, rejection and
  cleanup on mismatch, and that the CLI reports failure with a usable message
  instead of a traceback.

  Writing those tests found a real gap: **macOS arm64 is refused outright**,
  because no arm64 build is published. Serving an x86_64 binary to Apple silicon
  promises emulation that costs memory the user may not have and fails outright
  without Rosetta, so the refusal is now pinned by a test rather than left as
  incidental behaviour.

- **`setup-python` moved from the deprecated v3 to v5** in that workflow.

## [Unreleased]

### Added

- **`tarvos validate-artifact <path>`.** Reports what a built executable is and
  what it needs on the machine that runs it: the real format, read from the file's
  own header rather than its name, plus the manifest `tarvos build` writes beside
  the artifact. Exits non-zero when the artifact turns out to need a Python
  runtime, so a script or CI job can gate on it. An artifact with no manifest
  beside it is reported as `UNVERIFIED`, never as a pass.
- **A build manifest for every artifact.** `tarvos build` now writes
  `<artifact>.tarvos-manifest.json` next to what it produced, recording the format
  and size measured from the file, and whether the artifact is native, needs a
  Python runtime, or needs particular third-party packages. Every field comes from
  this build or from the file itself; none of it is asserted.
- **Dependency classification before compilation.** Every import a program makes
  is classified as natively lowered, partially lowered, externally provided, or
  unknown, and the classification is printed before anything is compiled. This is a
  property of the compiler, not of one machine's environment: nothing consults the
  developer's `site-packages`, so a build classifies a program the same way on any
  machine. `numpy` and `pandas` are classified *partially*, because only the
  recognized loop shapes compile, rather than being declared fully supported.

### Fixed

- **`tarvos build` no longer produces an executable that quietly needs Python.**
  When the native backend could not lower a dependency, the build emitted a
  launcher that unpacked the program to a temporary `.tarvos-*.py` file and ran it
  through the target machine's interpreter. The file was named `.exe`, printed
  "Build complete", and failed with `ModuleNotFoundError` on any machine that did
  not already have `flask`, `numpy`, or `pandas` installed — a failure a user
  could only diagnose by hand. A build now stops and names the dependency instead,
  and the launcher is produced only when `--compat-launcher` asks for it by name.
  Its manifest says plainly that it is not native and lists what the target
  machine needs.
- **A mislabelled artifact can no longer be published.** The format of a produced
  executable is read back from its header and checked against the target platform.
  A PE file named for Linux, or the reverse, is reported as a build error and the
  artifact is removed rather than written to the user's output path.
- **`tarvos build app.py` names its output after the input.** It used to default
  to `tarvos_app.exe` on every platform, so two builds in one directory
  overwrote each other and a Linux build was handed a `.exe` name. The default is
  now the input's stem plus the convention of the target format.
- **A compatibility launcher's manifest no longer claims the program needs
  nothing.** When the compiler could not parse a program, the manifest came out
  empty, because the imports were read from the parsed module. Imports are now
  also read from the source text when parsing fails, which is exactly the case
  that most needs them recorded.

### Fixed

- **The fallback broke any program that reads a file next to its own source.**
  When the native backend could not express a program, `tarvos run` did not run
  the caller's file. It compiled a compatibility launcher, cached it under
  `~/.tarvos/cache`, and executed that instead — so the program ran against a
  *copy* of the source and `__file__` pointed into the cache directory. A
  script loading a local model through `os.path.dirname(__file__)` failed with
  the model reported as missing and the cache path named as the place it had
  searched. A config file beside the script, or a sibling import, failed the
  same way. The fallback now hands the interpreter the original file, so
  `__file__` means what it means under `python main.py` and the working
  directory is the caller's own. As a side effect the program no longer pays a
  full `rustc` compile before it starts, and its own exit status is passed
  through instead of being reported as a failed Tarvos run.
  A standalone `tarvos build` executable is unchanged and still unpacks its
  source beside itself, which is what makes it portable.

- **Every command could hang for minutes on a real project.** Building the
  translation-cache key walked the input file's whole containing directory
  *recursively* and read and hashed every `.py` file it found, even though
  `tarvos run main.py` compiles exactly one file. A project that keeps a
  `transformers` checkout beside its script made each command process 19,705
  files before printing a single line: a 37-byte program took over 90 seconds
  and had still not finished. The key now covers the program and its
  same-directory siblings, which still catches an edited helper module, with a
  budget of 512 files and 8 MB so no single directory can dominate a command.
  Directory symlinks are skipped instead of followed, so a virtualenv's
  `lib64 -> lib` is no longer traversed twice. Measured on that project:
  `tarvos run main.py` went from "still running after 90 s" to 180 ms, and
  `tarvos compile --format exe` and `tarvos build` both settled at ~270 ms.

- **`tarvos build` and `tarvos compile --format exe` paid for `rustc` on every
  invocation.** The generated Rust was written to the cache and hashed, but the
  compiled binary was never kept or reused, so each call ran a full fat-LTO
  `rustc` even when the previous call had already produced exactly that program:
  5.6 s per invocation, every time. A program built once took several seconds
  to build again unchanged. The compiled artifact is now stored in
  `~/.tarvos/cache/tarvos-cache-*/rustc-bin` and copied straight to the output
  path on a repeat, which is ~190 ms. `tarvos run` already cached its
  executable and was unaffected.
  The cache is keyed on the generated source, the exact compiler and its
  version, the host target, the optimisation profile, and this Tarvos build, so
  a changed program, a different toolchain, or a Tarvos rebuild all still
  compile. Each entry is certified by a stamp written only after the binary is
  fully stored, and the recorded size is re-checked on read, so an interrupted
  build is recompiled rather than handed to the caller. Renaming the output
  reuses the cached binary, since symbols are stripped and the name does not
  reach the program.
- **A failed native build could leave a partial executable behind.** `rustc`
  wrote directly to the output path, so a build that died mid-link left a
  truncated file where the caller expects a whole program or nothing. The
  binary is now linked inside the cache and copied to the destination only
  after it succeeds.
- **Two compatibility launchers run at the same time deleted each other's
  script.** Every launcher wrote its embedded program beside the executable
  under the *original* file name, so every project in the world collided on
  `main.py`. Starting two of them together made the first one to exit remove
  the file the second was still running, which surfaced as
  `python3: can't open file '.../main.py': [Errno 2]`. The unpacked script is
  now named after the program plus the process id, so concurrent runs can
  never share a path.

- **The compatibility launcher is now a single file that carries its own
  source.** It previously embedded the build machine's absolute path and asked
  Python for that exact file, so a copy run anywhere else failed with `can't
  open file 'C:\Users\<someone else>\...\main_app.py': [Errno 2]`. The Python
  source is now embedded in the executable and unpacked beside it at run time,
  so the executable can be copied on its own and carries its own program. The
  working directory is set to the executable's own directory so a program that
  loads data by relative path finds it where the user put the file, and the
  temporary script is removed afterwards. The launcher still needs Python 3 on
  the machine that runs it, and says so when it cannot find one.
- **The launcher only ever tried `python`.** On Linux and macOS that name is
  often absent or still bound to Python 2, so a launcher that could have worked
  reported "Python was not found". It now tries `python3` then `python`, and if
  neither starts it names both failures instead of panicking.

### Changed

- **`tarvos build` produces a working executable instead of refusing.** A native
  build is still preferred and still reported as such. When the program is
  outside the native subset the command now emits the single-file launcher
  described above, because that artifact runs, and states plainly what it needs.
  The previous change made this case a hard error, which left the caller with no
  artifact and no obvious next step. `--strict-native` restores the error for
  callers who cannot ship anything that needs an interpreter.
- **`tarvos build` could emit an executable that was not a build of the program.**
  When a module used behaviour the native backend cannot express, the command
  fell back to a launcher: a tiny Rust program that re-ran the original `.py`
  file through the system `python`, using a path baked in at build time. The
  result only ever worked on the machine that built it. It carried the original
  absolute path, needed a Python installation and the original source file
  beside it, and the help text claimed such binaries embed no interpreter. A
  Windows executable produced this way failed under Wine with
  `failed to start Python compatibility runtime: program not found`, and would
  fail identically on any target machine. `build` now stops with an error that
  names the unsupported construct, explains why the launcher is not portable,
  points at `tarvos run` for executing the program locally, and names the new
  `--compat-launcher` flag for callers who want it anyway. `run` still falls back
  automatically, which is the right default for a command meant to execute
  something now rather than ship it.
- **`tarvos build` produced a binary tied to the CPU of the build machine.** The
  release path passed `-C target-cpu=native`, which tunes generated code for
  whichever processor happened to run the build. A binary produced on a recent
  CPU could fault with an illegal instruction on an older one, so a program
  built here was not guaranteed to run there. The baseline is used instead and
  the output stays portable across processors of the same architecture. The
  release workflow deliberately avoided this flag for the same reason, which
  made a `tarvos build` output *less* portable than the release binary built
  from the same commit.
- **A missing managed toolchain silently fell back to whatever Rust was on
  `PATH`.** The toolchain module states that nothing in it may silently fall
  back from one mode to the other, and the function performing the fallback
  claims the managed path never consults `PATH`; both were untrue of this path.
  On a machine with an old system Rust the build continued against it and
  surfaced as `unexpected argument '-C' found` naming `rustup`, a program the
  user never asked for. The fallback is kept — a developer with a working system
  Rust should still be able to build — but it now announces itself, names the
  compiler it fell back to, and points at `tarvos toolchain --install`.

### Added

- **The managed toolchain install now shows a real progress meter.** Fetching the
  pinned channel used to hand the transfer to `curl --progress-bar`, which draws a
  bare bar, prints nothing at all when its output is redirected, and renders
  differently on each platform and curl version. `tarvos toolchain --install` now
  draws its own meter from the size of the file being written, so the same run
  reports the same numbers everywhere: percentage complete, megabytes received of
  megabytes expected, current transfer rate, and a remaining-time estimate. An
  interactive terminal gets one line redrawn in place; a redirected log or CI run
  gets one line per 10% rather than a file full of overwritten lines. The closing
  summary reports the downloaded size, the average rate, the installed size, and
  the total wall-clock time.
- The install reports each phase it enters (`[2/6]` through `[6/6]`), so the
  steps that move no bytes — checksum, unpack, validate, publish — are visibly
  distinct instead of appearing as a silent pause.

### Changed

- **The Unix install unpacks only the components it uses.** The distribution
  archive carries `rust-docs`, `rust-html` and `rustc-docs` alongside `rustc`,
  `cargo` and `rust-std`, and a full extraction wrote all of it to disk only for
  `install.sh` to copy three directories out of it and discard the rest. Selecting
  the members at the archive skips the bulk of the bytes written. The Windows
  path has always done this; the Unix path now matches it. `install.sh` is still
  what lays out the prefix, so the resulting tree is unchanged.

### Fixed

- **The Unix install failed after a full download.** The component-selection
  change below extracted only `rustc`, `cargo` and `rust-std`, but `install.sh`
  lives at the top level of the distribution archive beside `components`,
  `manifest.in` and `rust-installer-version`, and reads the first and third. Every
  Unix install therefore ended with
  `sh: 0: cannot open .../rust-1.98.0-<triple>/install.sh: No such file or
  directory` after transferring all 364 MB. The extraction now takes the whole
  distribution directory and excludes the documentation components, which is
  where the saving was anyway.
- The progress meter left the tail of its own bar on screen. The final line was
  erased with a fixed width, so when the drawn line was wider than that width —
  the rate and ETA at the end of it — the fragments stayed behind and appeared
  beside the next transfer's summary. The meter now erases exactly the number of
  characters it drew.
- A truncated download is now rejected. `curl` can exit successfully when a
  connection drops cleanly at a boundary, and the short file was handed to the
  checksum step as though it were complete. The installer now compares the byte
  count against the size the server announced and refuses to continue on a
  mismatch.
- Several hints and error messages spelled the command `tarvos toolchain
  install`, which reads as a subcommand and is rejected. They now say
  `tarvos toolchain --install`, matching the top-level help, the managed
  toolchain documentation, and the verification scripts.

## [1.1.0-rc.6] - 2026-09-30

A correctness release. `1.1.0-rc.5` shipped a real bug in which a tuple
assignment inside a loop could leave a stale constant in place, so a function
returning that variable produced a **silently wrong number** rather than a
compile error. This release fixes it and adds the regression coverage that was
missing.

### Fixed

- **A zero divisor aborted the process instead of raising a catchable error.**
  Division was lowered to `.checked_div(..).expect("ZeroDivisionError")` for
  integers and to `panic!("ZeroDivisionError")` for floats. Both kill the
  process, so an enclosing `except ZeroDivisionError` never ran: a program
  written to recover from division by zero instead died.

  Float `/` was worse than a crash. It emitted a bare `a / b`, so a zero divisor
  produced `inf` and the program carried on with a silently wrong number. Float
  `//` had no zero check at all and did the same.

  Division now goes through checking helpers that report failure as a
  `Result`. Inside a `try`, the failure jumps to the handler the same way a call
  to a fallible function already did; with no handler the program still fails
  loudly. A zero divisor now matches CPython for `//`, `/` on integers, `/` on
  floats, and `//` on floats.
- **A `return` inside an `except` handler produced Rust that did not compile.**
  The `try` body is lowered to a labelled block so a `return` inside it can
  `break` out after `finally` runs. That block has already been closed by the
  time a handler executes, but the handler was still emitted against it, so the
  generated code read `break '__tarvos_try1;` with the label out of scope:
  `error[E0426]: use of undeclared label`. A handler now sees the enclosing
  `try`, or none, so its `return` is a real return.
- **Tuple assignment inside a loop returned a stale constant.** Copy
  propagation learned `a = 0` and did not learn that `a, b = b, a + b` rebinds
  both names, so a function ending in `return a` compiled to `return 0_i64`.
  The binary built and ran, printing a plausible value. The Fibonacci loop

  ```python
  a = 0
  b = 1
  for _ in range(n):
      a, b = b, a + b
  return a
  ```

  returned `0` instead of `832040` at `n = 30`. Both the optimizer's mutation
  set and the code generator's assignment check now treat a tuple assignment as
  a write to every one of its targets.

  The reason this survived to a release: writing the initial values on one line
  as `a, b = 0, 1` did **not** trigger it. Only the separated form did, so the
  common-looking version of the idiom happened to be the working one.

### Added

- An end-to-end pipeline regression that runs the real lowering, optimizer, and
  code generator over this exact shape and asserts the generated Rust returns
  the variable rather than a folded literal.
- Unit coverage that a tuple assignment clears a propagated constant, both
  inside a function with a loop and in a plain block.

### Why this was missed

The existing differential suite compares compiled output against CPython, which
is exactly the check that should have caught this. It did not, because no
workload in the corpus used the separated initialisation followed by a tuple
swap. The gap was in the corpus, not in the parity harness.

## [1.1.0-rc.5] - 2026-09-30

This release candidate carries 22 commits on top of `v1.1.0-rc.4`. The headline
is that **Tarvos no longer requires a system Rust installation**, and that a warm
CLI start no longer re-proves what the previous command already proved.

### Added - managed toolchain

- `tarvos toolchain --install` fetches the pinned channel into
  `~/.tarvos/toolchain` and verifies its SHA-256 before use.
- `tarvos toolchain --status` and `--verify` report the resolved toolchain, its
  layout, and per-stage validation.
- Managed and system toolchains resolve through separate code paths that share
  nothing, so a mode can never be substituted for the other by accident.
- Windows assembles the managed toolchain directly, because the upstream static
  distribution has no `install.sh`.

### Changed - explicit toolchain selection

- A build no longer uses whatever `rustc` happens to be on `PATH`. System Rust is
  consulted only when `--system-rust` is passed, and that compiler is validated
  before use.
- The managed install no longer edits the user's shell profile. Component
  documentation trees are skipped, which removes roughly 993 MB of HTML that
  rustc and cargo never read.

### Performance

- The compiler link probe result is cached in `~/.tarvos/toolchain/cache`, keyed
  to the exact rustc path, the pinned channel, and the host triple. Previously
  every `build`, `run`, and `verify` re-linked a throwaway probe program, and
  that subprocess is what made the CLI feel slow.
- A stamp is rejected when the compiler is newer than the stamp or when the
  payload differs, so an upgraded toolchain is re-probed rather than trusted.

### Fixed

- **Tuple assignment inside a loop returned a stale constant.** Copy propagation
  tracked `a = 0` and did not learn that `a, b = b, a + b` rebinds both names, so
  a function ending in `return a` compiled to `return 0_i64`. The native binary
  ran and produced output, so this was a silently wrong answer rather than a
  compile error. The Fibonacci loop
  (`a = 0; b = 1; for ...: a, b = b, a + b; return a`) returned `0` instead of
  `832040`. Both the optimizer's mutation set and the code generator's
  assignment check now treat a tuple assignment as a write to every target.
  Writing the initial values as `a, b = 0, 1` happened to avoid the bug, which
  is why it went unnoticed.
- Two `non_fmt_panics` sites in the generated `int()` and `float()` error paths.
  `{value}` inside a plain `panic!` string is not an interpolation in Rust, so
  the literal text was emitted instead of the value. The value is now passed as
  a format argument; the message text is unchanged.
- `version_of` strips a leading `v` from the parsed version token so the pin
  comparison does not depend on how a given rustc spells its own version.
- A packed-project error path built a path with a literal backslash, which is one
  path element on Unix; the separator now comes from `Path::join`.
- CI captures a failing test's assertion text, not just its name, and no longer
  caches `target/`, so a test can never run a stale binary.

## [1.1.0-rc.4] - 2026-09-27

This release candidate carries 43 commits of compiler work on top of
`v1.1.0-rc.3`. The headline change is that **exceptions and the `statistics`
module are now real native features rather than compatibility fallbacks**.

### Added - native exception handling

- `try` / `except` / `else` / `finally` and `raise` compile to real native Rust.
  A `try` used to be rejected as a dynamic construct and the program silently fell
  back to the Python compatibility launcher.
- Exceptions are `Result` values, not panics. The previous lowering used
  `std::panic::catch_unwind`, which was wrong three ways: a panic cannot carry a
  Python exception class, it cannot run `finally` on a non-local exit, and under
  the `panic = "abort"` release profile it does not catch at all.
- A `try` lowers to a labelled block rather than a closure, because a closure
  would put the body's `let` bindings out of scope and a variable assigned inside
  the try would vanish after it.
- `return` inside a `try` is deferred so `finally` still runs. A function that can
  raise returns a `Result` and its call sites unwrap it, so an error raised in a
  function reaches the caller's `try`.
- Handlers match the Python exception hierarchy, so `except ValueError` catches a
  `StatisticsError`. Previously only the first handler was emitted and the bound
  name was the literal string `"Tarvos exception"`.
- `statistics.StatisticsError` is catchable.

### Added - statistics API completion

- `median_grouped`, `quantiles`, `covariance`, `correlation`, and
  `linear_regression` are implemented and differentially tested.
- `median_grouped` follows the CPython 3.13 algorithm. The older "nudge the two
  central values" formulation disagrees whenever the median value repeats.
- `quantiles` reproduces CPython's exact integer rescaling, which deliberately
  extrapolates outside the observed range: `quantiles([1.0, 2.0])` is
  `[0.75, 1.5, 2.25]`.
- `covariance`, `correlation`, and `linear_regression` validate pair length,
  minimum sample size, and constant input.

### Fixed

- Keyword arguments were silently dropped by the frontend, so `f(x, n=2)`
  compiled as `f(x)` and produced a native binary that quietly computed
  something else. They are now reported.
- `harmonic_mean` returned an error for a zero input. CPython returns `0`.
- `mean`, `mode`, `median`, and `median_low`/`median_high` now preserve CPython's
  int-versus-float result kind, so `mode([1, 2, 2, 3])` is the int `2`.
- `geometric_mean` reduces through logarithms, matching CPython's accuracy.

### CI reliability

- The Rust toolchain is pinned to `1.98.0`; `stable` floated.
- Cargo and `target/` are cached with first-party `actions/cache`; there was no
  caching at all before.
- The Rust gates run through `scripts/ci_gates.py`, the same entry point used
  locally, so local and CI results cannot drift.
- `scripts/check_toolchain_pin.py` fails if the toolchain is declared
  inconsistently or starts floating again.
- Failure diagnostics are uploaded so a failing run is diagnosable from itself.

### Known limitations

- `quantiles` supports only the default `n=4, method="exclusive"`, because CPython
  declares both keyword-only and keyword arguments are not lowered natively yet.
- `linear_regression` returns `(slope, intercept)` rather than a named tuple.
- An uncaught native exception prints `Class: message` and exits 1; it does not
  print a Python traceback.
- Random, HTTP/requests, regex, datetime, and the broader standard library are
  not implemented. See `docs/COMPATIBILITY.md`.

## [Unreleased]

### CI reliability

- The Rust toolchain is now pinned to `1.98.0`. `rust-toolchain.toml` and both
  workflows previously said `stable`, which floats: a new stable release can
  change rustfmt output or add Clippy lints and turn every commit red at once
  even though no source changed. `scripts/check_toolchain_pin.py` now fails if
  the three declarations disagree, or if the channel starts floating again.
- Cargo and `target/` are cached with first-party `actions/cache`, keyed on OS,
  toolchain, and `Cargo.lock`. There was previously no caching at all, so every
  run rebuilt the whole dependency tree from scratch on three platforms.
- The four Rust gates run through `scripts/ci_gates.py`, the same entry point a
  developer runs locally, so "passes locally" and "passes in CI" cannot drift.
  The script stops at the first failing gate and names it.
- Failure diagnostics: the `quality` job uploads its logs on failure, and the
  native job's artifact now includes the whole differential build tree, so a
  compiler regression can be diagnosed from the run that caught it.
- The CPython version used as the differential oracle is documented in `ci.yml`.
  It stays pinned to 3.12 because some `statistics` results differ between 3.12
  and 3.13; a single-element `median_grouped` or `quantiles` input raises on
  3.12 and returns a value on 3.13.

### Fixed - keyword arguments were silently dropped

- The frontend discarded keyword arguments, so `f(x, n=2)` compiled as `f(x)` and
  produced a native binary that quietly computed something else. This surfaced
  while completing `quantiles`, whose `n` and `method` are keyword-only in CPython.
- The bridge now emits a diagnostic, so such a program takes the explicit
  compatibility path with a stated reason instead of a silently wrong result.
  Native lowering of keyword arguments itself is future-release work.

### Added - remaining statistics APIs

- `median_grouped`, `quantiles`, `covariance`, `correlation`, and
  `linear_regression` are implemented and differentially tested by
  `tests/corpus/42_statistics_paired.py`.
- `median_grouped` follows the **CPython 3.13** algorithm: find the value at the
  midpoint, count the points at or below it, then interpolate across the class
  interval. The older "nudge the two central values" algorithm disagrees whenever
  the median value is repeated, so using it would have produced wrong answers.
- `quantiles` reproduces CPython's exact integer rescaling (`j = i*m // n`,
  `delta = i*m - j*n`). A floating-point equivalent drifts, and a naive clamp of
  `j` into `1..ld-1` changes the answer because CPython's cut points genuinely
  extrapolate outside the observed range: `quantiles([1.0, 2.0])` is
  `[0.75, 1.5, 2.25]`, not values inside `[1, 2]`.
- A single-element sequence is handled the way CPython handles it (the value is
  repeated per cut) instead of panicking on an out-of-range index.
- `covariance`, `correlation`, and `linear_regression` validate pair length,
  minimum sample size, and constant input, reporting `StatisticsError`.
- Statistics call sites now check arity and only borrow the *sequence* arguments,
  so a scalar such as `interval` is no longer borrowed and a wrong-arity call
  produces a Tarvos diagnostic rather than an opaque Rust error.

### Known limitations added

- `quantiles` accepts only the default `n=4, method="exclusive"`. CPython declares
  `n` and `method` keyword-only and the native backend does not lower keyword
  arguments yet.
- `linear_regression` returns `(slope, intercept)`. CPython returns a
  `LinearRegression` named tuple, so `result.slope` is not available; use
  `result[0]` and `result[1]`.
- An uncaught native exception prints `Class: message` on stderr and exits 1. It
  does not print a Python traceback, which would require source-level frame
  information a native binary does not carry.

### Added - native exception handling

- `try` / `except` / `else` / `finally` and `raise` now compile to real native
  Rust. Previously a `try` was rejected as a dynamic construct and the program
  silently fell back to the Python compatibility launcher.
- Exceptions are `Result` values, not panics. The previous lowering used
  `std::panic::catch_unwind`, which was wrong three ways: a panic cannot carry a
  Python exception class, it cannot run `finally` on a non-local exit, and under
  the `panic = "abort"` release profile it does not catch at all.
- A `try` lowers to a **labelled block** rather than a closure. A closure was
  rejected because its `let` bindings leave scope, so a variable assigned inside
  the try would have been missing after it.
- `return` inside a `try` is deferred into a slot and the real `return` is emitted
  after `finally`, which is what Python does.
- Handlers are matched against the Python exception hierarchy, so
  `except ValueError` catches a `StatisticsError`, exactly as in CPython.
  Previously only the *first* handler was ever emitted and the bound name was the
  literal string `"Tarvos exception"`.
- A function that can raise returns `__TarvosResult<T>`; call sites unwrap it, so
  an error raised in a function reaches the caller's `try` instead of terminating
  at the point of the raise.
- `statistics.StatisticsError` is now **catchable**. Every statistics call
  reports an empty sequence, a too-small sample, and a negative geometric product
  through a `Result` rather than a `panic!`.

### Fixed - statistics

- `harmonic_mean` returned an error for a zero input. CPython returns `0` as soon
  as it sees a zero; the native runtime now matches.

### Added - statistics module

- `import statistics` now resolves to a real native runtime instead of being
  rejected. Twelve APIs are supported and differentially tested against CPython
  by `tests/corpus/40_statistics.py`: `mean`, `fmean`, `geometric_mean`,
  `harmonic_mean`, `median`, `median_low`, `median_high`, `mode`, `multimode`,
  `pvariance`, `pstdev`, `variance`, and `stdev`.
- The runtime is generic over a small numeric trait, so one implementation serves
  both int and float lists, and it borrows its input, so a caller's list is not
  moved or mutated.
- `median`, `median_low`, `median_high`, `mode`, and `multimode` return an
  *element of the input* rather than a fresh float, matching CPython: `mode` of
  `[1, 2, 2, 3]` is the int `2`, not `2.0`, and `median` of an odd-length int
  list is an int. `multimode` preserves CPython's first-appearance ordering.
- `geometric_mean` reduces through logarithms, which is both what CPython does
  and the numerically better form: `[1.0, 4.0, 16.0]` gives exactly `4.0`, where
  the n-th-root-of-product form gives `3.9999999999999996`.
- An empty sequence aborts at run time with a `StatisticsError` message rather
  than returning an arbitrary value, and `harmonic_mean` rejects a zero.

### Known limitations added

- `median_grouped`, `quantiles`, `correlation`, `covariance`, and
  `linear_regression` are not implemented and are reported as unsupported rather
  than emitted as a stub.
- `statistics.StatisticsError` is not an importable name. The error is a run-time
  abort with a `StatisticsError` message, not a catchable Python exception, and
  `try`/`except` around a statistics call still falls back to the Python
  compatibility launcher rather than compiling natively.

### Added - json.dumps on run-time values

- `json.dumps` accepts a value the program built at run time, not only a compile-time
  literal. A serializer covers int, float, bool, str, list, and dict, with
  Python's `", "` and `": "` separators and `null` for non-finite floats.
- The literal fast path is unchanged: it renders during lowering and needs no
  runtime, so a program that only serializes literals carries no serializer.

### Known limitations added

- A dict must have one value type, so `{"name": "x", "n": 1}` is rejected.
- A `HashMap` cannot reproduce Python insertion order, so keys are emitted
  sorted. That is deterministic but is not Python's ordering guarantee.


### Fixed - conversions, truthiness, and generated-code hygiene

- `int(x)` and `float(x)` are chosen from the argument's static type instead of
  emitting `x as i64`. `print(int("3"))` is ordinary Python and previously
  produced `"3".to_string() as i64`, which is not valid Rust. A string source is
  now parsed by a runtime helper that raises `ValueError` when it cannot.
- `str(x)` renders type-directed: a bool becomes `True` rather than `true`, and
  an integral float keeps its `.0` rather than losing it to Rust's `Display`.
- `bool(x)` dispatches to emptiness for str/list/dict and to a zero comparison
  for numbers. The blanket `x != 0` produced `if 0_i64` for `bool(0)` and
  `"x".to_string() != 0` for `bool("x")`, neither of which compiles.
- `__tarvos_group_numeric` was pushed into every generated file
  unconditionally. A one-line `print("hello")` carried an unused ~25-line
  helper; it now generates exactly `fn main()`. Binary size is unchanged,
  because LTO already removed the dead code, so this is a code-quality fix
  rather than a size win.

### Added - compatibility matrix

- `docs/compatibility.json`: 72 classified features (37 supported, 14 partial,
  20 unsupported, 1 planned), generated by `scripts/compatibility_matrix.py`.
- `docs/COMPATIBILITY.md`: the human-readable view of the same data.
- The generator validates that every entry naming a differential case points at
  a case that exists, and that the matrix version matches the workspace. CI runs
  `python scripts/compatibility_matrix.py --check`, so the matrix cannot drift
  into claiming coverage no test proves.
- NumPy, Pandas, and PyTorch are recorded as `unsupported`, not `partial`: there
  is no adapter and no claim.

### Fixed - project compilation

- `from package import submodule` followed by `submodule.f()` now compiles. The
  module's definitions were inlined but the qualified access was never
  rewritten, so the call had no receiver and failed with "unknown object".
  Qualified calls are converted to plain calls during inlining.
- Inlining is now idempotent per file. A module reachable through two imports
  emitted its definitions twice, which is a duplicate-definition error in Rust.

### Changed - CI

- `quality` (fmt/check/clippy/test) runs on windows-latest, ubuntu-latest, and
  macos-latest instead of Linux only.
- A new `native` job runs the differential suite, the CLI audit, the project
  acceptance test, and a generated-Rust compile check on all three platforms.
  These were previously only ever run by hand, which is how a broken `elif`
  chain, a truncated modulo, and a wrapping negative index all reached main.
- The three harnesses resolve the CLI through a platform suffix instead of
  hard-coding `tarvos.exe`, so they work off Windows.
- CI now also checks the compatibility matrix is current.

### Known limitations

- Runtime integers are `i64`. Python's arbitrary precision is not emulated;
  compile-time reductions widen to `u128`, but a runtime value cannot exceed
  `i64`. See `docs/COMPATIBILITY.md` for the full list.
- `import *`, slice steps, file I/O, `pickle`, networking, `asyncio`, and
  threading are rejected with explicit diagnostics.
- `pyproject.toml` is not consumed; no dependency resolution is performed.
- `tarvos.toml` has not been introduced.
- **Remote CI status is unverified from the development machine.** The
  repository is not reachable through the unauthenticated GitHub API and no `gh`
  CLI or token is available, so the workflow changes above are verified locally
  (YAML parses, the embedded build step was extracted and executed, and all
  harnesses pass on Windows) but their result on GitHub-hosted runners is not
  confirmed.

### Fixed - Python semantics

Found by differential testing (`benchmarks/difftest.py`), which runs each case
under CPython and under a compiled native artifact and compares stdout, stderr,
and exit code. Every item below was invisible to the existing shape-asserting
unit tests.

- `if` / `elif` / `else` chains were lowered as a series of independent
  branches followed by an unconditional `else`, so a native binary could run
  more than one branch for a single evaluation. For `x = 10` a three-way chain
  printed both the matching `elif` and the `else`. The AST bridge now rebuilds
  the chain as nested `If`s, matching CPython's own `ast`.
- `%` was folded with Rust's truncated remainder, so `-7 % 2` compiled to `-1`
  instead of Python's `1`. Python's modulo is floored: the result takes the sign
  of the divisor.
- `7 / 2` folded to `3`. Type inference had already typed the expression as
  float, but the constant folder ignored the result type and truncated.
  `10 / 5` printed `10` instead of `10.0`.
- A float-typed division cast only integer *literals*, so `total / count` over
  two call results stayed integral.
- `xs[-1]` was compiled to `xs[(-1 as usize)]`, which wraps to `usize::MAX` and
  aborted the process. Negative indexing now works for lists and strings, on
  both the read and the write path, and raises `IndexError` out of range.
  `str` could not be indexed at all before, because `str` does not implement
  `Index<usize>`.
- `print(4.0)` printed `4`; Python's `str(4.0)` is `4.0`.
- `1 == 1.0` generated `1_i64 == 1.0_f64`, which is not valid Rust.
- `list.remove(v)` was emitted with an immutable slice while its helper takes
  `&mut Vec<T>`, so any program using `remove` failed to compile.
- `list.pop(index)` was rejected outright. `pop()` also returned a default value
  for an empty list instead of raising `IndexError`.
- Passing an owned value to a user function moved it, so a second use of the
  same variable failed to compile.
- `and` / `or` over non-bool operands generated invalid Rust (`0_i64 || 7_i64`).
  Python returns one of its operands, so the construct is now diagnosed
  explicitly with the `bool()` workaround named.

### Added - project compilation

- Native compilation of a project with local imports. `from module import name`,
  `from package import submodule`, and relative imports (`from . import x`) are
  resolved against the importing file and inlined before lowering, so a project
  uses the same parser, lowering, optimizer, and codegen as a single file. The
  `ImportFrom` AST node now carries a relative-import level.
- `tarvos analyze` accepts a project directory and reports per-module statistics.

### Fixed - CLI

- `secure_input_path`, `secure_output_path`, and `scan_project_mode` rejected any
  path that resolved outside the current directory, so
  `tarvos compile /opt/app/main.py` failed from anywhere else. The check was not
  a security control: the argument is the user's own, and the boundary that
  matters, a compiled module escaping its project with `..`, is enforced in
  `resolve_local_module`.
- `tarvos python` returned 1 for every failing program instead of forwarding the
  child's exit code.

### Added - validation tooling

- `benchmarks/difftest.py`: CPython-versus-native differential harness.
- `benchmarks/cli_audit.py`: exercises all 15 advertised commands in an isolated
  temporary directory, including building and running a native artifact.
- `benchmarks/project_acceptance.py`: builds a fixture project, packages it, and
  compares the native executable's output with CPython.
- `tests/corpus/`: eleven differential cases.

### Known limitations

- `from package import submodule` followed by `submodule.f()` is diagnosed, not
  compiled: the import binds a module namespace and the qualified call form is
  not yet rewritten. Use `from package.submodule import f`.
- `import *` and imports whose target does not exist on disk are reported
  rather than guessed at.

CI/CD and release-pipeline work. No compiler behaviour changes. The
`v1.1.0-rc.3` tag was not moved and its published release description was not
rewritten.

### Changed - workflow responsibilities

- `ci.yml` is now the only "is the source healthy?" workflow. It runs
  `fmt`, `check`, `clippy`, and `cargo test --no-fail-fast` on a single Linux
  runner, on pull requests and pushes to `main`. It no longer runs on tags and
  no longer builds release artifacts.
- Cross-platform verification moved to a `cross-target` job that runs on demand
  and on a weekly schedule instead of on every commit. A normal commit no longer
  pays for five cross-target builds.
- `.github/workflows/release-ci.yml` was deleted. Its release-grade validation
  (workspace tests, the production validation suite, the compile/build smoke
  test) moved into `release.yml` as the `validate` job, and its benchmark
  matrix became the non-gating `benchmarks` job. Previously the same tests ran
  on every push to `main` while releases skipped them.
- `release.yml` is the only workflow that may create or edit a release. It runs
  on a pushed `v*` tag or a manual dispatch that names an existing tag, never
  on a branch push.

### Fixed - release safety

- The release gate checked out the default branch, so a `workflow_dispatch`
  publish validated the metadata of `main` instead of the commit being
  released. Every job in the pipeline now checks out the release tag.
- The pipeline asserted that the tag exists and matches the version, but never
  that the checked-out tree was the tagged commit. It now fails with
  `Wrong source commit` when they differ.
- A blank, `main`, or malformed `release_tag` is refused with a specific error
  instead of resolving to a branch. The dispatch input no longer carries a
  hard-coded version default, which was a footgun that republished a stale
  release.
- Release binaries were built with `-C target-cpu=native`, producing a binary
  tuned to the runner's CPU that can fault on older hardware. Release builds are
  now portable; size and stripping are unchanged.
- The built CLI's reported version is checked against the release tag, and each
  release asset is required to exist together with its checksum before anything
  is published.
- A leftover draft release for the tag is refused rather than silently edited.
- `fail_on_unmatched_files` and `overwrite_files` were set on the distribution
  upload, so a re-run replaces assets by name instead of accumulating
  duplicates.
- `build` depended only on `verify`, so it ran in parallel with `validate`.
  Binaries and a published release could therefore be produced from a tree whose
  tests, validation suite, smoke build, and CPython differential had all failed.
  `build` and both publication jobs now require `validate` to succeed.
- The distribution job checked that each `.sha256` file existed but never
  recomputed it, so a stale checksum, or a binary corrupted in artifact
  transfer, would still have been published. Every digest is now recomputed and
  compared before upload.
- `extract_release_notes.py` required only a minimum length. The section for the
  current workspace version must now also carry `## Highlights`,
  `## Breaking Changes`, `## Validation`, and `## Installation`, and every
  section may contain at most one comparison link. The structural rule applies
  only to the release being cut, so re-running an older tag still works and
  published history is not re-validated.

### Changed - permissions, concurrency, and reporting

- `ci.yml` declares `permissions: contents: read` explicitly, as does
  `release.yml` at workflow level. Only the `publish-source-release` job holds
  `contents: write`.
- Release concurrency is keyed on the release tag, so two different tags do not
  block each other, and `cancel-in-progress` stays false: a publication is never
  cancelled by a newer run.
- CI cancels only superseded pull request runs, never push, schedule, or manual
  runs.
- Guard failures emit `::error title=...` annotations, and each job prints the
  tag, commit, target, and version it is working on, so a failure identifies
  itself instead of exiting silently. No step suppresses a failure with
  `|| true`.
- `check_version_consistency.py` now runs in exactly one place, the release
  gate, against the tagged commit. The two workflow-internal version sites were
  removed, leaving 12 version sites that are all shipped metadata.

### Known limitation

- The `repo-tech/tarvos-engine` release for `v1.1.0-rc.3` is still
  outstanding. It depends on GitHub Actions runners, and every job on this
  account currently fails before its first step with no log. The compiler source
  release in `repo-tech/tarvos` is unaffected and complete.

## [1.1.0-rc.3] - release candidate

Stabilization release on the 1.1.0 line. It extends the native subset with the
`list()`/`sorted()` builtins, closes the cross-front-end gaps that forced the
CPython fallback, and removes a class of stale-cache bugs. No new version was
declared in the binary until this release: `tarvos --version` now reports
`1.1.0-rc.3`, and the whole release metadata set is verified by
`scripts/check_version_consistency.py`.

### Native Compilation

- `list()` and `sorted()` are compiled natively with preserved element types
  for `range()`, `str`, list, homogeneous tuple, and dict keys (`sorted`).
  Lowering tags the source kind (`__tarvos_list_from_range`,
  `__tarvos_sorted_from_str`, ...) so the emitter never re-derives a bare
  name's type; a homogeneous tuple is desugared to element reads and an empty
  `list()` emits a typed vector instead of `Vec<()>`.
- `os` subset (`getcwd`, `listdir`, `mkdir`, `makedirs`, `chdir`) and static
  `json.dumps` for compile-time literals.
- Bitwise (`&`, `|`, `^`, `~`), shift (`<<`, `>>`), floor division (`//`), and
  power operators lower natively on both front ends, including `augmented`
  forms.

### Python Compatibility

- Chained comparisons (`a <= b <= c`) desugar to short-circuiting `and`, so a
  middle operand is evaluated once.
- Tuple assignment, nested subscript assignment (`grid[i][j]`, `grid[i][j] +=`),
  list comprehensions over strings, and boolean/comparison/set/dict literals
  bridge from the Ruff front end.
- String methods (`lower`, `upper`, `title`, `strip`, `split`, `join`,
  `replace`, `zfill`, ...), list methods (`append`, `extend`, `sort`, `pop`,
  ...), and dict methods (`get`, `update`, `keys`, `values`, `items`) dispatch
  on the receiver's static type through one registry shared by both front ends.
- `print` reproduces Python's `str`/`repr` split, and iteration follows Python
  semantics for `str` (by character) and `dict` (by key).

### Compiler

- Shared AST normalization pass in `tarvos-core` used by both front ends, so a
  new rewrite is implemented once instead of twice.
- IR widening: `Destructure`, chained `IndexAssign`, `Invert`, `BitXor`,
  `BitAnd`, `BitOr`, `LShift`, `RShift`, `FloorDiv`.
- Type inference improvements: variable type collection before lowering,
  iterable element types, empty-list element hints from the first `append`,
  comprehension element types.

### Fixed

- Stale native translation cache: the cache epoch now hashes the CLI
  executable's contents as well as its version/size/mtime, so a rebuilt binary
  can never reuse a translation or an `unsupported` verdict from a previous
  build. Previously a fast relink could preserve size and mtime granularity and
  silently reuse stale generated Rust.
- `list()` over a dict is rejected with an actionable diagnostic instead of
  emitting an order-dependent result (see Known limitations).

### Tooling

- `list()` over a dict, unknown element types, heterogeneous tuples, and
  keyword arguments produce explicit `--python-fallback` diagnostics rather
  than a `Vec<()>` type error.
- Benchmark workloads for matrix multiplication and a bitwise/LCG kernel with
  recorded expected outputs.
- `scripts/check_version_consistency.py` fails the build when any release
  metadata drifts from the Cargo workspace version; the workspace version is
  now the single source of truth (`version.workspace = true`).

### AI / Ollama

- `tarvos ai-status` (plus a `doctor` line and an installer post-install step)
  records the optional local AI capability in
  `~/.tarvos/ai-capability.json`. The probe is read-only, loopback-only, and
  timeout-bounded: it never starts, stops, downloads, or configures anything,
  and only a `TarvosProvisioned` ownership is sticky. Compilation works
  without any local AI capability.

### Security

- `SECURITY.md` replaced the unfilled GitHub template (including a fabricated
  version table) with the real supported-version policy and private advisory
  reporting. No secrets are uploaded by the CLI, and the sandbox/gateway
  boundary is unchanged.

### Validation

- `cargo check --workspace --all-targets`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace --no-fail-fast` (25 suites, 110 tests),
  `cargo build --workspace --release`, `python
  scripts/check_version_consistency.py --require-tag v1.1.0-rc.3`, and the
  native-vs-CPython differential gate in
  `crates/tarvos-cli/tests/compatibility_gate.rs` all pass against the tagged
  commit `cb1c333`.
- `python scripts/diff_test.py` reports 13/13 workloads for the release binary
  built from that commit.
- One gate does not pass: `cargo fmt --all -- --check` reports a single
  line-wrap deviation in `crates/tarvos-optimizer/src/lib.rs` introduced by the
  Clippy cleanup in the same commit. It is behaviour-neutral, the tag was not
  moved, and the deviation is fixed on `main`, where formatting is now a CI
  gate.

### Known limitations

- `list()` over a dict refuses native compilation because `HashMap` iteration
  order differs from Python insertion order; `sorted(dict)` is supported
  because sorting is deterministic.
- `set`, `enumerate`, `zip`, dynamic `json.loads`, arbitrary third-party
  imports, and dynamic JSON values still require `--python-fallback`.
- Cross-platform release artifacts and the Windows installer are produced by
  the release workflow on each platform's runner; the installer is not built
  or validated on a developer machine without Inno Setup.

## [1.1.0-rc.2] - release candidate

### Added

- Cross-target CI that separates native-host test execution from
  cross-target artifact builds.
- Type-preserving native lowering for branches, loops, classes, lists,
  strings, and dictionaries with explicit diagnostics for incompatible
  reassignment.

### Changed

- Hardened the native compatibility boundary after the 1.0.0 baseline;
  see `RELEASE_NOTES.md` for the full release-candidate notes.

## [1.1.0-rc.1]

- Hardened native Python compatibility (native lowering, compatibility
  gate, actionable fallback diagnostics).

## [1.0.0] - first stable release

### Added

- Native AST export, typed lowering, IR optimization, and Rust code
  generation.
- Standalone native executable generation for the supported Python subset.
- Explicit compatibility diagnostics for unsupported dynamic Python
  features.
- Production gateway, health endpoint, sandbox configuration, and
  Docker/Render deployment assets.

### Compatibility boundary

Tarvos is not a drop-in CPython replacement; see `docs/compiler-status.md`
for the exact supported subset.

[Unreleased]: https://github.com/repo-tech/tarvos/compare/v1.1.0-rc.3...HEAD
[1.1.0-rc.3]: https://github.com/repo-tech/tarvos/compare/v1.1.0-rc.2...v1.1.0-rc.3
[1.1.0-rc.2]: https://github.com/repo-tech/tarvos/compare/v1.1.0-rc.1...v1.1.0-rc.2
[1.1.0-rc.1]: https://github.com/repo-tech/tarvos/compare/v1.0.0...v1.1.0-rc.1
[1.0.0]: https://github.com/repo-tech/tarvos/releases/tag/v1.0.0
