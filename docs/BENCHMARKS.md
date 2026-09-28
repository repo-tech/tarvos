# Benchmarks and binary size

All numbers below were **measured on this repository**, not carried over. The
harness is `benchmarks/size_benchmark.py`; it prints its own environment and can
write a JSON report.

```bash
cargo build --release -p tarvos-cli
python benchmarks/size_benchmark.py --json benchmarks/size_report.json
```

## Method

Transpile time, Cargo build time, total build time, native runtime, and binary
size are measured **separately**. They are not combined into one "Tarvos is N×
faster" number, because a bundler that embeds a Python interpreter and a
compiler that emits a single static binary are not measuring the same thing.

- Transpile and native runtime: 3 runs each, reported as median/min/max.
- Cargo build: measured once. A repeat would mostly time the incremental cache.
- Binary size: the artifact on disk, after the release profile
  (`opt-level="z"`, `lto`, `codegen-units=1`, `strip`).
- Compressed size: zlib level 9 on the raw artifact. This is **not** a tarball
  or installer size, which include headers, metadata, and compression of a
  different granularity.
- Timing uses `time.perf_counter()` in the harness, which is monotonic. The
  generated programs do not time themselves.

## Environment

| | |
|---|---|
| OS | Windows 10, AMD64 |
| Python | 3.13.13 |
| cargo | 1.98.0 (797e8a9bc 2026-08-05) |
| rustc | 1.98.0 (88d9e12ae 2026-08-18) |
| Tarvos | 1.1.0-rc.4 |
| Profile | release: `opt-level="z"`, `lto = true`, `codegen-units = 1`, `strip = true` |

**Single machine, single run of the harness.** These are indicative, not a
statistically meaningful cross-platform result. Linux and macOS figures are not
available because no runner on those platforms could execute here; CI records
the cross-platform *correctness* result, not timings.

## Results

Times in milliseconds, median of 3. `binary` is the raw artifact.

| Workload | Transpile | Cargo build | Total build | Native run | Binary | zlib-9 |
|---|---|---|---|---|---|---|
| hello | 50.2 | 2359 | 2409 | 38.48 | 126.0 KiB | 63.5 KiB |
| arithmetic (100k loop) | 104.6 | 3336 | 3440 | 12.67 | 127.0 KiB | 63.9 KiB |
| collections (1k list) | 50.6 | 2301 | 2351 | 23.93 | 128.5 KiB | 64.4 KiB |
| strings (200 parts) | 59.9 | 4550 | 4610 | 145.99 | 133.0 KiB | 66.2 KiB |

Reading these honestly:

- **The compiler is fast; the Rust toolchain is not.** Transpile is 31–42 ms.
  Cargo is 1.0–1.5 s on a warm cache, so a one-off build is dominated by
  `rustc`, not by Tarvos.
- **A generated program is close to an empty Rust binary.** An empty
  `fn main() {}` with the same profile is 104 KiB; the generated programs are
  126–133 KiB. The delta is the Python runtime helpers the compiler emits for
  printing, lists, and strings.
- **Binary size is dominated by that floor, not by the program.** hello and the
  100k-iteration arithmetic loop differ by 1 KiB. Adding computation does not
  add size; adding library surface would.
- **Unused helpers are not emitted.** A `print("hello")` program generates
  exactly `fn main()` and no runtime at all. An earlier revision pushed a
  thousands-separator helper into every file unconditionally; it is now emitted
  only when a program uses `f"{value:,}"`. This did **not** change binary size,
  because LTO already eliminated the dead code, so it is a code-quality fix
  rather than a size win.

## Baselines measured the same way

| Artifact | Bytes | KiB |
|---|---|---|
| Empty Rust binary, same release profile | 106,496 | 104.0 |
| Tarvos-generated `hello` | 129,024 | 126.0 |
| Tarvos-generated `strings` | 136,192 | 133.0 |
| **`tarvos.exe` (the compiler itself)** | **6,358,528** | **6,209.5** |

### Correction to a previously recorded figure

An earlier note recorded a "Tarvos base binary: approximately 103 KB". **That
does not reproduce.** Measured on this machine and this commit, `tarvos.exe` is
6.2 MB. The ~104 KiB figure is the size of an *empty Rust binary* with this
release profile, and the generated programs are 126–133 KiB. Whichever artifact
the earlier number referred to, it is not the compiler. The historical value is
not carried forward here.

## Competitors

**No competitor was measured, so no comparison is claimed.**

`benchmarks/competitor_env.py` and `install_competitors.py` exist for setting up
Nuitka and PyInstaller, but neither was run in this environment. A fair
comparison needs the same workload, the same Python version, the same
optimization level, and a stated artifact definition:

- A PyInstaller or Nuitka artifact **embeds a CPython interpreter**; a Tarvos
  artifact does not. That is a genuine architectural difference, and it makes a
  raw size or startup number a comparison of two different products, not of two
  compilers.
- Startup time would favour a static binary; first-run size would favour the
  bundler only if a runtime were already present on the machine.
- Tarvos cannot compile the workloads those tools accept, so a "faster build"
  comparison over a shared corpus is not currently constructible.

Until that is done properly on a shared corpus, any "faster than X" or "smaller
than X" claim would be unfounded. None is made.

## Reproducing

The harness records its own OS, architecture, tool versions, and profile, and
writes them into the JSON report, so a number is never separated from the
conditions it was produced under. It does not compare against anything, by
design.
