# Tarvos compiler status

## Scope and audit basis

The current implementation is a Rust workspace with a Python AST exporter, a typed IR, analysis/lowering, optimization, and Rust code generation. The project targets a carefully bounded subset of Python rather than full Python compatibility.

## Feature matrix

| Feature | Python AST | IR | Semantic Analysis | Type Checking | Optimization | Codegen | Test | Status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| int | Yes (CPython `ast` exporter) | Yes | Yes | Yes | Yes | Yes | Yes | Implemented |
| float | Yes | Yes | Yes | Yes | Yes | Yes | Yes | Implemented |
| bool | Yes | Yes | Yes | Yes | Yes | Yes | Yes | Implemented |
| string | Yes | Yes | Yes | Yes | Partial | Partial | Yes | Partially implemented |
| None | Yes | Yes | Yes | Yes | Partial | Partial | Yes | Partially implemented |
| variable assignment | Yes | Yes | Yes | Yes | Yes | Yes | Yes | Implemented |
| augmented assignment | Yes | Yes | Yes | Yes | Partial | Partial | Yes | Partially implemented |
| arithmetic | Yes | Yes | Yes | Yes | Yes | Yes | Yes | Implemented |
| bitwise (`&`, `\|`, `^`, `~`) and shifts (`<<`, `>>`) | Yes (Ruff front end + CPython AST export) | Yes | Yes | Yes | Yes | Yes | Yes | Implemented |
| floor division `//` | Yes (Ruff front end + CPython AST export) | Yes | Yes | Yes | Yes | Yes | Yes | Implemented (Python floor rounding preserved) |
| comparisons | Yes | Yes | Yes | Yes | Yes | Yes | Yes | Implemented |
| if | Yes | Yes | Yes | Yes | Partial | Yes | Yes | Implemented |
| while | Yes | Yes | Yes | Yes | Partial | Yes | Yes | Implemented |
| for | Yes | Yes | Yes | Yes | Partial | Yes | Yes | Implemented |
| range | Yes | Yes | Yes | Yes | Partial | Yes | Yes | Implemented |
| functions | Yes | Yes | Yes | Yes | Partial | Yes | Yes | Implemented |
| return | Yes | Yes | Yes | Yes | Partial | Yes | Yes | Implemented |
| lists | Yes | Yes | Yes | Yes | Partial | Yes | Yes | Partially implemented |
| indexing | Yes | Yes | Yes | Partial | Partial | Yes | Yes | Partially implemented |
| function calls | Yes | Yes | Yes | Yes | Partial | Yes | Yes | Implemented |
| nested scopes | Yes | Yes | Partial | Partial | Partial | Partial | Partial | Limited |
| loop variables | Yes | Yes | Yes | Yes | Partial | Yes | Yes | Implemented |
| basic error handling | Partial | Partial | Partial | Partial | Partial | Partial | Partial | Limited |

The native standard-library bridge includes selected `math`, `time`, `os`,
`os.path`, and `json` operations. The current `os` subset is `getcwd`,
`listdir`, `mkdir`, `makedirs`, and `chdir`; this is not a claim that the
complete Python `os` API is statically compiled. Native JSON support currently
covers `json.dumps` for compile-time literal lists, tuples, scalar values, and
string-keyed dictionaries. Dynamic JSON values and `json.loads` still require
the explicit CPython fallback.

The native loop lowering is type-aware for `list`/array values, strings (each
Python character is materialized as a one-character string), and dictionaries
(Python key iteration). Integer literals larger than `i64` are transported
without floating-point rounding; native `range()` rejects such bounds with an
actionable fallback diagnostic because the current native loop ABI is `i64`.

## Current implementation notes

- The Python front end is intentionally not a full CPython parser replacement. It uses two front ends: the native Ruff-based front end (`tarvos-ruff-frontend`) for the default compile path, and a CPython `ast` export step via `python/ast_export.py` for the embedded target and the `TARVOS_COMPAT_AST=1` migration path. Both front ends accept the same operator and annotation subset.
- Constructs that Python spells differently from the shared IR are desugared once,
  in `tarvos-core`'s `normalize` pass, so both front ends benefit:
  - **Chained comparisons.** `'a' <= c <= 'z'` becomes a short-circuiting `and`
    of pairwise comparisons, which preserves Python's left-to-right evaluation.
    The shared operand is only re-used when it is free of side effects; an
    effectful middle operand is left intact and reported rather than evaluated
    twice.
  - **Parallel assignment through subscripts.** `a[i], a[j] = a[j], a[i]` is
    staged through generated temporaries before any target is written, which is
    what makes the swap idiom work. Temporaries never shadow a name bound in the
    same scope.
- The Ruff front end preserves parameter and return annotations, so declared signatures are used for call return-type inference (including recursive calls) instead of defaulting to `None`.
- Unannotated parameters are typed from their call sites. A pre-pass collects
  the type of every variable assigned in the module (including loop variables and
  `xs = []` refined by its first `append`), so `def f(s): return s.lower()` binds
  `s: str` when it is called as `f(word)`. A parameter that cannot be resolved
  still falls back to `i64`, which is the historical behavior.
- `str`, `list` and `dict` builtin methods are lowered natively. The receiver's
  static type selects the method, and the generated program carries a small
  `std`-only runtime for the ones with no direct Rust equivalent (`join`,
  `zfill`, `sort`, ...). Covered: `lower`, `upper`, `title`, `capitalize`,
  `swapcase`, `strip`, `lstrip`, `rstrip`, `ljust`, `rjust`, `zfill`, `center`,
  `replace`, `join`, `split`, `splitlines`, `startswith`, `endswith`, `isdigit`,
  `isalpha`, `isalnum`, `isspace`, `isupper`, `islower`, `count`, `find`,
  `rfind`, `index`; `append`, `extend`, `insert`, `remove`, `pop`, `clear`,
  `sort`, `reverse`, `index`, `count`; `update`, `setdefault`, `get`, `pop`,
  `keys`, `values`, `items`.
  - A method that mutates its receiver is only accepted in statement position;
    used as an expression it is reported, because the IR cannot express a value
    that a later write would invalidate.
  - `sort()` dispatches on the element type, since `f64` is not `Ord`.
- `list()` and `sorted()` are lowered natively with a preserved element type.
  Lowering tags the source kind (`__tarvos_list_from_range`,
  `__tarvos_sorted_from_str`, ...) so codegen never re-derives a bare `Name`'s
  type: lazy `range()` is collected, `str` splits into one-character strings, a
  homogeneous tuple is desugared to field reads (`t = (1, 2); list(t)` reads
  `t.0`, `t.1`), an existing list is cloned, and `sorted()` sorts with the
  element-typed helper (`total_cmp` for `f64`). `list()` over a dict refuses
  native compilation because `HashMap` iteration order differs from Python
  insertion order; `sorted(d)` stays native because sorting is deterministic.
  Anything else bails with an actionable `--python-fallback` diagnostic instead
  of emitting `Vec<()>`.
- `print` reproduces Python's `str`/`repr` split: a top-level value prints
  unquoted, while a value nested in a list or dict prints quoted
  (`['a', 'b']`, not `["a", "b"]`).
- Iteration follows Python semantics: a `str` iterates by character and a `dict`
  by key, in both `for` loops and comprehensions.
- Native f-string rendering preserves literal format specifications (`.4f`, `05d`, `,`, `_`); a dynamic specification or a debug f-string (`f"{x=}"`) is reported as unsupported and requests the compatibility runtime.
- The IR is a compact typed representation that supports numerical and branch-heavy workloads.
- Code generation is focused on Rust output for a static subset of Python and is not designed to cover arbitrary Python semantics.
- Native locals preserve the inferred primitive/container/object type through
  branch and loop pre-initialization. Reassigning a native variable to an
  incompatible type is rejected with an actionable `--python-fallback`
  diagnostic instead of emitting a guessed `i64` declaration.
- Truly dynamic values still require the Python fallback; this is an explicit
  compatibility boundary, not a claim of a complete dynamic Python runtime.
- The CLI compatibility gate verifies native execution, actionable native
  failures, and `tarvos run ... --python-fallback` through CPython. Runtime
  arguments are forwarded to both execution paths; native programs only observe
  them when their supported subset exposes an argument-reading API.
- Performance benchmarking in this project should be treated as a comparison against explicitly supported subset workloads, not full-language execution.

## Audit status

The current workspace compiles under the GNU Rust toolchain and passes the existing workspace test suite. Remaining gaps are mostly in coverage beyond the supported subset, especially for advanced Python semantics, runtime exceptions, and wider edge-case behaviors.

## Proof harness and environment reporting

The project now includes a reproducible differential harness for the audited workload set:

- `scripts/diff_test.py` compares CPython stdout to Tarvos-generated Rust stdout for each workload in `benchmarks/workloads/`.
- `benchmarks/workloads/expected_outputs.json` records the deterministic expected output for each supported workload.
- `benchmarks/competitor_env.py` detects whether PyPy, Numba, Nuitka, Codon, and Rust are available on the local system without attempting unapproved installs.
- `benchmarks/results/results.json` stores the pass/fail matrix and machine-readable results for the current environment.

In the current environment, the audited workload corpus passes end-to-end, while PyPy, Numba, Nuitka, and Codon are not installed or available on PATH and Rust is available as the native compiler toolchain.
