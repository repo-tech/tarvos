# Stress Test: the measured native boundary

`stress_test.py` is ordinary Python, written to exercise the core language rather
than to flatter the compiler. It runs under CPython and prints 90 lines ending
in `TARVOS_STRESS_TEST_COMPLETE`. It is kept here, outside `tests/corpus/`,
because it does **not** compile natively yet, and `benchmarks/difftest.py`
auto-discovers `tests/corpus/*.py` and would fail on it.

This file records exactly where the boundary is, measured rather than estimated.
Every entry below was produced by running the compiler on this file and reading
the diagnostic; nothing here is a guess about what "should" work.

## What the compiler rejects, and why

The whole file is refused as a unit, because one unsupported construct anywhere
falls back the entire program. Removing constructs one at a time shows the file
is blocked by these, and only these:

| # | Construct | Occurrence in the test |
|---|---|---|
| 1 | Generator expression | `sum(x * 0.5 for x in numbers)` |
| 2 | Generator expression | `any(x > 8 for x in range(10))` |
| 3 | `in` membership test | `7 in values` |
| 4 | Nested comprehension (two `for` clauses) | `[x * y for x in ... for y in ...]` |
| 5 | Dict comprehension | `{x: x * x for x in range(6)}` |
| 6 | Lambda as a function argument | `map(lambda x: x * 2, ...)` |
| 7 | Lambda as a function argument | `filter(lambda x: ..., ...)` |
| 8 | Lambda as a `sorted(key=)` argument | `sorted(records, key=lambda i: i[1])` |
| 9 | `pass` statement | `finally: pass` |
| 10 | A comparison the operator table does not cover | see the diagnostic |

A single-comprehension comprehension (`[x * x for x in range(10)]`) is supported;
it is only the *second* `for` clause that is not.

## What already works

Everything else in the file lowers natively today, including several constructs
that are the usual suspects for a Python-to-Rust compiler:

- Dynamic reassignment across five types on one name
  (`10` → `"hello"` → `[1,2,3]` → `{...}` → `42.5`), which is the canonical
  dynamic-Python case and is handled by the tagged value path.
- Mutual and direct recursion (`factorial`, `fibonacci`).
- List mutation as a sequence: `append`, `extend`, `insert`, `remove`, `pop`,
  `sort`, `reverse`, membership, negative indexing.
- Slicing with a step, including `xs[::-1]`, on both lists and strings.
- String methods: `lower`, `upper`, `strip`, `split`, `replace`, `startswith`,
  `endswith`, `find`, `count`, `join`.
- `str.format` and f-strings.
- Dict construction, mutation, `get` with a default, and `keys`/`values`/`items`.
- Tuples, set operators with `sorted`, `enumerate`, `zip`, `map`, `filter`,
  `any`, `all`.
- Closures, higher-order functions, and a list returned from a function.
- Nested exception handling with `finally`, and `None` identity tests.
- `math` and `statistics`, including `variance` and `mode`.

So the gap is narrower than "dynamic Python is unsupported": it is eleven
specific constructs, concentrated in comprehension nesting and lambdas.

## How to reproduce this measurement

Clear the translation cache first, because a cached failure marker replaces the
real diagnostic with a one-line summary:

    rm -rf ~/.tarvos/cache            # Linux/macOS
    Remove-Item -Recurse -Force "$env:USERPROFILE\.tarvos\cache"   # Windows
    cargo run -p tarvos-cli -- compile tests/stress/stress_test.py -o out.rs

## Turning this file green

When each row above is implemented, move this file to `tests/corpus/` so
`difftest.py` holds it to CPython parity from then on. Until every row is done,
adding it there would turn a known, documented boundary into a CI failure that
looks like a regression.