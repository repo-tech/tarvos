# Automatic Native Subset & Library Loop Detector

Tarvos now exposes an opt-in project scanner:

```powershell
tarvos scan path\to\project
```

The scanner recursively visits Python files (excluding `__pycache__`), exports
their CPython AST, and runs a Rust-side visitor with an extensible library
registry. The initial registry recognizes NumPy and Pandas aliases and reports:

- imports and aliases;
- `for` and `while` loop nodes;
- index access and cumulative assignments;
- NumPy array creation and element-wise loop candidates;
- Pandas `iterrows()` and `itertuples()` loops;
- a conservative native plan (`Vec<f64>`, contiguous/ndarray-style storage, or
  typed iterators);
- explicit CPython fallback when the loop is too dynamic.

The detector remains analysis-only for dynamic library behavior. As a narrowly
verified native subset, constant one-dimensional `np.arange(n)`, `np.zeros(n)`,
and `np.ones(n)` expressions are lowered to typed Tarvos list storage when
`n` is a non-negative integer literal. Dynamic shapes, dtypes, and library
calls remain on CPython instead of producing an incorrect native executable.

The registry is implemented by `LibraryDetector` in
`crates/tarvos-analysis/src/native_detector.rs`. New libraries can register
their import aliases, recognized calls, and method patterns without changing
the project scanner.

Security and correctness rules:

- no Python code is executed during scanning;
- source paths remain under the current safe project root;
- unknown calls inside hot loops are marked for CPython fallback;
- parallelization is only a hint and requires a later purity/data-race pass;
- generated native code is not emitted until the existing type checker accepts
  the program.
