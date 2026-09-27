"""Generate and verify the machine-readable compatibility matrix.

`docs/compatibility.json` is the single source of truth for what Tarvos
supports. This script regenerates it and verifies that every entry claiming
coverage names a differential case that actually exists, so the matrix cannot
claim more than the corpus proves.

    python scripts/compatibility_matrix.py --write
    python scripts/compatibility_matrix.py --check
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
CORPUS = ROOT / "tests" / "corpus"
MATRIX = ROOT / "docs" / "compatibility.json"
VERSION = "1.1.0-rc.3"


def workspace_version() -> str | None:
    """Read the version from the workspace manifest.

    Kept in step with Cargo.toml so the matrix cannot describe a release that
    does not exist. Returns None when the manifest cannot be read, in which case
    the check is skipped rather than guessed.
    """
    import re

    manifest = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(r'(?m)^version = "([^"]+)"', manifest)
    return match.group(1) if match else None

# (category, feature, status, tests, note)
# status: supported | partial | unsupported | planned
ROWS: list[tuple[str, str, str, list[str], str]] = [
    # --- language basics -------------------------------------------------
    ("syntax", "variables and assignment", "supported", ["03_integer", "19_scope"], ""),
    ("syntax", "arithmetic + - * / // % **", "supported", ["03_integer"], ""),
    ("syntax", "floats", "supported", ["04_float"], ""),
    ("syntax", "comparisons", "supported", ["06_comparison"], ""),
    ("syntax", "boolean logic and/or", "partial", ["06_comparison"],
     "Non-bool operands are rejected: Python returns an operand, not a bool. Use bool()."),
    ("syntax", "f-strings", "partial", [],
     "Literal parts and simple interpolation; a dynamic format spec is rejected."),
    ("syntax", "ternary expression", "supported", [], ""),
    ("syntax", "list comprehension", "partial", [], "Single generator only."),
    ("syntax", "classes", "partial", [],
     "Methods and attributes on own instances; no inheritance or dunder dispatch."),
    ("syntax", "lambda", "unsupported", [], "Parsed by the front end, not lowered natively."),
    ("syntax", "generators / decorators / async", "unsupported", [], "Not lowered natively; the front end rejects them."),
    # --- control flow ----------------------------------------------------
    ("control_flow", "if / elif / else", "supported", ["08_control_flow"], ""),
    ("control_flow", "while", "supported", ["08_control_flow"], ""),
    ("control_flow", "for", "supported", ["08_control_flow", "22_data_pipeline"], ""),
    ("control_flow", "break / continue", "supported", ["08_control_flow"], ""),
    ("control_flow", "nested control flow", "supported", ["08_control_flow", "19_scope"], ""),
    # --- functions -------------------------------------------------------
    ("functions", "definition and calls", "supported", ["19_scope"], ""),
    ("functions", "multiple return paths", "supported", ["19_scope"], ""),
    ("functions", "default and annotated parameters", "partial", [],
     "Annotations type the binding; defaults are parsed but not always applied."),
    ("functions", "*args / **kwargs", "unsupported", [], "A variadic call signature is not modelled in the IR."),
    ("functions", "recursion", "partial", [], "Direct recursion works; mutual recursion is unresolved."),
    # --- collections -----------------------------------------------------
    ("collections", "list literals and methods", "supported", ["12_lists"], ""),
    ("collections", "empty list", "partial", ["12_lists"],
     "An empty literal needs a later append to infer its element type."),
    ("collections", "negative indexing", "supported", ["13_indexing"], ""),
    ("collections", "list conversion list(...)", "partial", ["14_list_conversion"],
     "list, tuple, range, str sources. dict is rejected: HashMap order is not Python insertion order."),
    ("collections", "tuple", "supported", [], ""),
    ("collections", "dict", "partial", [],
     "Key access and mutation; repr is sorted like Python. Insertion order is not preserved."),
    ("collections", "set", "unsupported", [], "A set literal is parsed but has no native lowering."),
    ("collections", "slicing", "partial", [], "Positive step only; a slice step is rejected."),
    # --- strings ---------------------------------------------------------
    ("strings", "concatenation, length, indexing", "supported", ["13_indexing", "17_strings"], ""),
    ("strings", "str methods", "supported", ["17_strings"], ""),
    ("strings", "unicode indexing", "supported", ["13_indexing"], "Indexed by character, not byte."),
    # --- integers / floats -----------------------------------------------
    ("integers", "bounded native arithmetic", "supported", ["03_integer"], ""),
    ("integers", "arbitrary precision", "unsupported", [],
     "Runtime integers are i64. Compile-time reductions widen to u128, but a runtime value cannot exceed i64."),
    ("integers", "floored modulo and division", "supported", ["03_integer"], ""),
    ("integers", "true division yields float", "supported", ["03_integer", "04_float"], ""),
    ("integers", "bitwise and shift", "supported", ["03_integer"], ""),
    ("integers", "numeric promotion int/float", "supported", ["03_integer", "04_float"],
     "Promoted by the inferred result type, not by scattered casts in codegen."),
    ("conversions", "int() and float() from str", "supported", ["04_float"],
     "Parses the string and raises ValueError when it cannot; Rust's `as` cannot convert a String at all."),
    ("conversions", "int()/float()/str() from scalars", "supported", ["04_float"],
     "Type-directed: a bool becomes True/False, an integral float keeps its .0."),
    ("conversions", "bool() truthiness", "supported", ["04_float"],
     "Emptiness for str/list/dict, zero for numbers; a single named operation rather than `!= 0` per type."),
    # --- modules and packages --------------------------------------------
    ("imports", "from module import name", "supported", [], "Project acceptance fixture."),
    ("imports", "from package import submodule", "supported", [],
     "Direct and qualified call forms. Project acceptance fixture."),
    ("imports", "relative imports", "partial", [],
     "Resolved against the importing module's package; more dots than depth is rejected."),
    ("imports", "import *", "unsupported", [], "The exported name set is not statically known."),
    ("imports", "import module (bare)", "partial", [], "Stdlib only: math, time, os, os.path, json."),
    ("imports", "dynamic import (importlib)", "unsupported", [], "The module cannot be resolved statically."),
    ("imports", "circular imports", "unsupported", [], "Reported rather than mis-inlined."),
    # --- stdlib ------------------------------------------------------------
    ("stdlib", "math", "supported", [], ""),
    ("stdlib", "time", "supported", [], ""),
    ("stdlib", "json", "supported", [], "dumps only; loads is unsupported."),
    ("stdlib", "json.dumps on a literal", "supported", [], "Rendered during lowering; no runtime needed."),
    ("stdlib", "json.dumps on a runtime value", "partial", ["04_float"],
     "Serializes int, float, bool, str, list, and dict. A dict must have one "
     "value type, so heterogeneous values are rejected. A HashMap cannot "
     "reproduce Python insertion order, so keys are emitted sorted."),
    ("stdlib", "os / os.path", "partial", [], ""),
    # statistics. `40_statistics` compares 35 output lines against CPython for
    # int, float, negative, zero, repeated, singleton, and odd/even-length data.
    ("stdlib", "statistics.mean / fmean", "supported", ["40_statistics"],
     "mean reduces through exact integer arithmetic and returns an int when all "
     "input is int and the quotient is whole, matching CPython; fmean always "
     "returns a float."),
    ("stdlib", "statistics.geometric_mean / harmonic_mean", "supported", ["40_statistics"],
     "geometric_mean reduces through logarithms, matching CPython's accuracy; "
     "both raise on an empty sequence."),
    ("stdlib", "statistics.median", "supported", ["40_statistics"],
     "Odd-length input returns the middle element with its original type, so an "
     "int list yields an int; even-length input returns a float."),
    ("stdlib", "statistics.median_low / median_high", "supported", ["40_statistics"],
     "Return an element of the input, preserving int vs float."),
    ("stdlib", "statistics.mode / multimode", "supported", ["40_statistics"],
     "Return input elements; multimode preserves CPython's first-appearance order."),
    ("stdlib", "statistics.pvariance / pstdev", "supported", ["40_statistics"],
     "pstdev requires at least one data point; pvariance of a singleton is 0.0."),
    ("stdlib", "statistics.variance / stdev", "supported", ["40_statistics"],
     "variance and stdev require at least two data points, as in CPython."),
    ("stdlib", "statistics.median_grouped / quantiles / correlation / covariance / "
     "linear_regression", "unsupported", [],
     "Not yet implemented; the compiler reports these as unsupported rather than "
     "emitting a stub."),
    ("stdlib", "statistics.StatisticsError", "unsupported", [],
     "The exception class is not importable. An empty sequence aborts with a "
     "StatisticsError message at runtime rather than raising a catchable Python "
     "exception, and try/except still falls back to the Python launcher."),
    ("stdlib", "collections / itertools / functools", "unsupported", [], "Not in the supported stdlib list."),
    ("stdlib", "file I/O (open)", "unsupported", [], "No filesystem runtime is emitted into generated code."),
    ("stdlib", "dataclasses / enum / typing", "unsupported", [], "Not lowered natively; annotations are parsed but not enforced."),
    # --- numerical ecosystem ----------------------------------------------
    ("numpy", "ndarray", "unsupported", [],
     "No implementation. Future compatibility target; no claim is made."),
    ("pandas", "DataFrame", "unsupported", [],
     "No implementation. Future compatibility target; no claim is made."),
    ("pytorch", "Tensor", "unsupported", [],
     "No implementation. Future compatibility target; no claim is made."),
    # --- concurrency / io --------------------------------------------------
    ("concurrency", "threading / multiprocessing", "unsupported", [], "Generated code is single-threaded."),
    ("concurrency", "asyncio", "unsupported", [], "The event loop has no native equivalent here."),
    ("networking", "socket / http", "unsupported", [], "No network runtime is emitted into generated code."),
    ("serialization", "pickle", "unsupported", [], "Depends on CPython object layout, which is not reproduced."),
    # --- project builds ----------------------------------------------------
    ("project", "single file compilation", "supported", ["03_integer", "10_range"], ""),
    ("project", "multi-module project compilation", "supported", [], "Project acceptance fixture."),
    ("project", "package directories with __init__.py", "supported", [], ""),
    ("project", "entry point selection (--entry)", "supported", [], ""),
    ("project", "Cargo project export", "supported", [], ""),
    ("project", "pyproject.toml metadata", "unsupported", [],
     "Present on disk but not consumed. No dependency resolution is performed."),
    ("project", "tarvos.toml manifest", "planned", [],
     "Not introduced; deliberately avoided until a real need is demonstrated."),
    ("project", "resource bundling", "unsupported", [], "Assets are not embedded; open() is unsupported."),
]

VALID_STATUS = {"supported", "partial", "unsupported", "planned"}


def build() -> dict:
    features = []
    for category, feature, status, tests, note in ROWS:
        entry = {
            "category": category,
            "feature": feature,
            "status": status,
            "tests": tests,
        }
        if note:
            entry["note"] = note
        features.append(entry)
    return {
        "schema": 1,
        "tarvos_version": VERSION,
        "note": (
            "Generated by scripts/compatibility_matrix.py. Do not edit by hand. "
            "'supported' means a differential case compares CPython against a "
            "compiled native executable; the case is named in 'tests'."
        ),
        "features": features,
    }


def verify(matrix: dict) -> list[str]:
    """Check the matrix against the rules it claims to enforce."""
    problems: list[str] = []
    available = {path.stem for path in CORPUS.glob("*.py")}

    for entry in matrix["features"]:
        status = entry.get("status")
        label = entry.get("feature", "<unnamed>")
        if status not in VALID_STATUS:
            problems.append(f"{label}: invalid status {status!r}")
            continue
        if status in {"unsupported", "planned"} and not entry.get("note"):
            problems.append(f"{label}: {status} without an explanatory note")
        for name in entry.get("tests", []):
            if name not in available:
                problems.append(
                    f"{label}: claims test {name!r} but tests/corpus/{name}.py "
                    f"does not exist"
                )

    # The matrix must describe the version the workspace is actually on.
    current = workspace_version()
    if current is not None and matrix["tarvos_version"] != current:
        problems.append(
            f"matrix targets version {matrix['tarvos_version']!r} but the workspace "
            f"is {current!r}; update VERSION in this script"
        )
    return problems


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--write", action="store_true", help="regenerate the file")
    parser.add_argument("--check", action="store_true", help="fail if the file is stale")
    args = parser.parse_args()

    matrix = build()
    problems = verify(matrix)
    for problem in problems:
        print(f"ERROR: {problem}", file=sys.stderr)
    if problems:
        return 1

    if args.write:
        MATRIX.parent.mkdir(parents=True, exist_ok=True)
        MATRIX.write_text(json.dumps(matrix, indent=2) + "\n", encoding="utf-8")
        print(f"wrote {MATRIX.relative_to(ROOT)} ({len(matrix['features'])} features)")
    if args.check:
        if not MATRIX.exists():
            print("ERROR: docs/compatibility.json is missing", file=sys.stderr)
            return 1
        if json.loads(MATRIX.read_text(encoding="utf-8")) != matrix:
            print("ERROR: docs/compatibility.json is stale; run --write", file=sys.stderr)
            return 1
        print("compatibility matrix is current")
    if not args.write and not args.check:
        counts: dict[str, int] = {}
        for entry in matrix["features"]:
            counts[entry["status"]] = counts.get(entry["status"], 0) + 1
        print(f"{len(matrix['features'])} features: "
              + ", ".join(f"{k}={v}" for k, v in sorted(counts.items())))
    return 0


if __name__ == "__main__":
    sys.exit(main())
