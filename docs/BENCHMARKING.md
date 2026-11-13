# Tarvos benchmark methodology

Tarvos performance claims are workload-specific. The CI benchmark is a
reproducibility and regression check, not a universal speed guarantee.

## Measurements

`benchmarks/benchmark_matrix.py` records:

- one warm-up execution, excluded from the measured samples;
- execution samples and median;
- minimum and maximum execution time;
- standard deviation;
- compiler/build time separately where a native compiler is invoked;
- stdout output for every available runtime;
- output-parity status across comparable runtimes;
- Python, Rust, Tarvos, runner, and platform information.

CI runs seven measured samples:

```bash
python benchmarks/benchmark_matrix.py \
  --runtime all \
  --repeats 7 \
  --json-output benchmarks/results/runtime_matrix.json
```

Optional runtimes that are not installed are reported as `unavailable`; they do
not invalidate the Tarvos/CPython/Rust comparison. A non-matching output from
an available runtime fails the benchmark.

## Interpreting speedups

The reported speedup is:

```text
CPython median execution time / Tarvos median execution time
```

It excludes compilation time and therefore describes repeated execution of an
already-built artifact. Report compiler time separately when discussing
end-to-end latency. Do not generalize a single workload's speedup to all
Python programs.

The benchmark workload is deliberately within Tarvos's supported static Python
subset. It is not a test of GUI, audio, threading, dynamic imports, or the full
Python standard library.
