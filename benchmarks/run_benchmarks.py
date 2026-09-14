#!/usr/bin/env python3
import argparse
import csv
import json
import os
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BUILD_TMP_ROOT = ROOT / ".build-tmp"
BUILD_TMP_ROOT.mkdir(parents=True, exist_ok=True)

PRESETS = {
    "dev": {
        "rustflags": "",
        "cranelift": False,
    },
    "release": {
        "rustflags": "-C opt-level=2 -C codegen-units=1",
        "cranelift": False,
    },
    "cranelift": {
        "rustflags": "-C codegen-units=1 -C debuginfo=0",
        "cranelift": True,
    },
}


def safe_tool_roots():
    roots = [ROOT.resolve(), Path(sys.executable).resolve().parent]
    home = Path.home()
    for candidate in [
        home / ".cargo" / "bin",
        home / ".local" / "bin",
        ROOT / ".bench-venv" / ("Scripts" if os.name == "nt" else "bin"),
        ROOT / "tools",
    ]:
        try:
            roots.append(candidate.resolve())
        except FileNotFoundError:
            roots.append(candidate)
    deduped = []
    for root in roots:
        if root not in deduped:
            deduped.append(root)
    return deduped


def is_safe_tool_path(path: Path):
    resolve_path = path.expanduser().resolve(strict=False)
    for root in safe_tool_roots():
        try:
            if resolve_path == root or root in resolve_path.parents:
                return True
        except Exception:
            return False
    return False


def resolve_tool_path(name: str, extra_candidates=None):
    candidates = []
    if extra_candidates:
        candidates.extend(extra_candidates)
    candidates.extend([name, str(Path(name).expanduser())])

    for candidate in candidates:
        try:
            resolved = shutil.which(candidate)
        except Exception:
            resolved = None
        if resolved:
            path = Path(resolved).expanduser().resolve(strict=False)
            sibling = None
            if path.name.lower() == "rustup.exe" and name.lower() in {"rustc", "cargo"}:
                sibling_name = "rustc.exe" if name.lower() == "rustc" else "cargo.exe"
                sibling = path.with_name(sibling_name)
            elif path.name.lower() == "rustc.exe" and name.lower() == "rustc":
                sibling = path
            elif path.name.lower() == "cargo.exe" and name.lower() == "cargo":
                sibling = path
            if sibling and sibling.exists() and is_safe_tool_path(sibling):
                return str(sibling)
            if path.exists() and is_safe_tool_path(path):
                return str(path)
        if candidate and os.path.isabs(candidate):
            path = Path(candidate).expanduser().resolve(strict=False)
            if path.exists() and is_safe_tool_path(path):
                return str(path)

    for candidate in ["python3", "python", "rustc", "cargo", "pypy3", "pypy", "nuitka", "codon"]:
        resolved = shutil.which(candidate)
        if resolved:
            path = Path(resolved).expanduser().resolve(strict=False)
            if path.name.lower() == "rustup.exe" and candidate.lower() in {"rustc", "cargo"}:
                sibling = path.with_name("rustc.exe" if candidate.lower() == "rustc" else "cargo.exe")
                if sibling.exists() and is_safe_tool_path(sibling):
                    return str(sibling)
            if path.exists() and is_safe_tool_path(path):
                return str(path)
    return None


def sanitize_environment(base_env=None):
    env = (base_env or os.environ).copy()
    path_entries = []
    for item in env.get("PATH", "").split(os.pathsep):
        if not item:
            continue
        try:
            dir_path = Path(item).expanduser().resolve(strict=False)
        except OSError:
            continue
        if dir_path.exists() and (dir_path == ROOT or ROOT in dir_path.parents or any(root in dir_path.parents for root in safe_tool_roots()) or "System32" in str(dir_path) or "Windows" in str(dir_path)):
            path_entries.append(str(dir_path))
    safe_roots = [str(root) for root in safe_tool_roots() if root.exists()]
    for root in safe_roots:
        if root not in path_entries:
            path_entries.insert(0, root)
    env["PATH"] = os.pathsep.join(dict.fromkeys(path_entries))
    return env


def safe_run(args, **kwargs):
    env = sanitize_environment(kwargs.pop("env", None))
    kwargs["env"] = env
    if not args:
        raise ValueError("safe_run requires at least one command argument")
    if isinstance(args, (list, tuple)) and args:
        first = str(args[0])
        if not os.path.isabs(first):
            resolved = resolve_tool_path(first, extra_candidates=list(args))
            if resolved:
                args = [resolved, *list(args[1:])]
    return subprocess.run(args, **kwargs)


def discover_benchmark_cases():
    examples_dir = ROOT / "examples"
    supported_stems = {"simple", "if_example"}
    cases = []
    seen = set()
    for py_file in sorted(examples_dir.glob("*.py")):
        if py_file.stem not in supported_stems:
            continue
        rs_file = py_file.with_suffix(".rs")
        if not rs_file.exists():
            continue
        stem = py_file.stem.replace("_", " ").replace("-", " ")
        name = stem.title() + " Benchmark"
        if py_file.name not in seen:
            seen.add(py_file.name)
            cases.append((py_file, rs_file, name))
    return cases


def apply_preset(preset_name: str):
    preset = PRESETS.get(preset_name.lower(), PRESETS["dev"])
    os.environ["TARVOS_USE_CRANELIFT"] = "1" if preset["cranelift"] else "0"
    if preset["rustflags"]:
        os.environ["TARVOS_RUSTFLAGS"] = preset["rustflags"]
    else:
        os.environ.pop("TARVOS_RUSTFLAGS", None)
    return preset


def windows_gnu_toolchain():
    if os.name != "nt":
        return None
    rustc_bin = resolve_tool_path("rustc")
    if not rustc_bin:
        return None
    for tc in ("+stable-x86_64-pc-windows-gnu", "+nightly-x86_64-pc-windows-gnu"):
        try:
            r = safe_run([rustc_bin, tc, "--version"], capture_output=True, text=True, env=sanitize_environment())
            if r.returncode == 0:
                return tc
        except (subprocess.CalledProcessError, FileNotFoundError):
            continue
    return None


def get_rustflags():
    flags = os.environ.get("TARVOS_RUSTFLAGS", "")
    return shlex.split(flags) if flags else []


def rustc_command(extra_flags=None):
    flags = list(get_rustflags())
    if extra_flags:
        flags.extend(extra_flags)
    rustc_bin = resolve_tool_path("rustc")
    if not rustc_bin:
        raise RuntimeError("rustc is not available in a safe toolchain location")
    cmd = [rustc_bin]
    tc = windows_gnu_toolchain()
    if tc:
        cmd.append(tc)
    if os.environ.get("TARVOS_USE_CRANELIFT", "").lower() in {"1", "true", "yes"}:
        cmd += ["-Zcodegen-backend=cranelift"]
    return cmd + flags


def cargo_env():
    env = sanitize_environment()
    flags = get_rustflags()
    if flags:
        env["RUSTFLAGS"] = " ".join(flags)
    if os.name == "nt":
        env["RUSTUP_TOOLCHAIN"] = "stable-x86_64-pc-windows-gnu"
    return env


def ensure_tarvos_binary():
    candidates = [
        ROOT / "target" / "x86_64-pc-windows-gnu" / "release" / ("tarvos.exe" if os.name == "nt" else "tarvos"),
        ROOT / "target" / "x86_64-pc-windows-gnu" / "debug" / ("tarvos.exe" if os.name == "nt" else "tarvos"),
        ROOT / "target" / "release" / ("tarvos.exe" if os.name == "nt" else "tarvos"),
        ROOT / "target" / "debug" / ("tarvos.exe" if os.name == "nt" else "tarvos"),
    ]

    existing = [c for c in candidates if c.exists()]
    if existing:
        existing.sort(key=lambda p: p.stat().st_mtime, reverse=True)
        return str(existing[0])

    cargo_bin = resolve_tool_path("cargo")
    if cargo_bin is None:
        raise RuntimeError("cargo is not available in a safe toolchain location")
    safe_run([cargo_bin, "build", "--bin", "tarvos"], cwd=str(ROOT), env=cargo_env(), check=True, capture_output=True)

    for c in candidates:
        if c.exists():
            return str(c)
    raise FileNotFoundError("tarvos binary was not built")


def benchmark_python(py_file, repeats=5):
    py_file = Path(py_file).resolve()
    timings = []
    python_bin = resolve_tool_path(sys.executable) or sys.executable
    for _ in range(repeats):
        start = time.perf_counter()
        safe_run([python_bin, str(py_file)], check=True, capture_output=True, cwd=str(py_file.parent), env=sanitize_environment())
        timings.append(time.perf_counter() - start)
    return sum(timings) / len(timings)


def benchmark_rust_file(rs_file, binary_name="bench_out", repeats=5):
    rs_file = Path(rs_file).resolve()
    import tempfile
    import uuid

    run_times = []
    rustflags = [
        "-C",
        "opt-level=3",
        "-C",
        "debuginfo=0",
        "-C",
        "codegen-units=1",
    ]
    if os.environ.get("TARVOS_USE_CRANELIFT", "").lower() in {"1", "true", "yes"}:
        rustflags.append("-Zcodegen-backend=cranelift")

    with tempfile.TemporaryDirectory(prefix=f"ep_rust_{uuid.uuid4().hex[:8]}_", dir=str(BUILD_TMP_ROOT)) as tmpdir:
        binary_path = Path(tmpdir) / (f"{binary_name}.exe" if os.name == "nt" else binary_name)
        compile_start = time.perf_counter()
        rustc = rustc_command([*rustflags, "-o", str(binary_path), str(rs_file)])
        proc = safe_run(rustc, capture_output=True, text=True, cwd=str(rs_file.parent), env=sanitize_environment())
        if proc.returncode != 0:
            raise subprocess.CalledProcessError(proc.returncode, rustc, output=proc.stdout, stderr=proc.stderr)
        compile_time = time.perf_counter() - compile_start

        safe_run([str(binary_path)], check=True, capture_output=True, cwd=str(rs_file.parent), env=sanitize_environment())
        for _ in range(repeats):
            run_start = time.perf_counter()
            safe_run([str(binary_path)], check=True, capture_output=True, cwd=str(rs_file.parent), env=sanitize_environment())
            run_times.append(time.perf_counter() - run_start)

    return sum(run_times) / len(run_times), compile_time


def benchmark_tarvos(py_file, generated_rs_name="tarvos_generated.rs", repeats=5):
    import tempfile
    import uuid

    compiler = ensure_tarvos_binary()
    py_file = Path(py_file).resolve()

    # Use a repo-local temporary directory so execution remains inside the safe workspace while still being fresh per run.
    with tempfile.TemporaryDirectory(prefix=f"ep_gen_{uuid.uuid4().hex[:8]}_", dir=str(BUILD_TMP_ROOT)) as tmpdir:
        output_path = Path(tmpdir) / generated_rs_name

        # 1. Transpile once
        transpile_start = time.perf_counter()
        r = safe_run(
            [compiler, "compile", str(py_file), "--output", str(output_path)],
            capture_output=True,
            text=True,
            cwd=str(ROOT),
            env=sanitize_environment(),
        )
        if r.returncode != 0:
            details = (r.stderr or r.stdout or "").strip()
            raise subprocess.CalledProcessError(
                r.returncode,
                [compiler, "compile", str(py_file), "--output", str(output_path)],
                output=r.stdout,
                stderr=(
                    f"Tarvos compilation failed for {py_file} (exit {r.returncode}).\n"
                    f"{details}"
                ),
            )
        transpile_time = time.perf_counter() - transpile_start

        # 2. Compile and run generated Rust
        run_time, rust_compile_time = benchmark_rust_file(output_path, binary_name="tarvos_bench_out", repeats=repeats)
        total_compile_time = transpile_time + rust_compile_time

    return run_time, total_compile_time


def format_ms(value_s: float) -> str:
    return f"{value_s * 1000:.2f} ms"


def detect_binary(name: str, version_args):
    import shutil

    candidates = []
    for candidate in [name, f"{name}.exe", f"{name}.cmd", f"{name}.bat"]:
        resolved = resolve_tool_path(candidate)
        if resolved:
            candidates.append(resolved)
    venv_bin = Path(sys.executable).resolve().parent
    if venv_bin.exists():
        for suffix in ["", ".exe", ".cmd", ".bat"]:
            candidate = venv_bin / f"{name}{suffix}"
            if candidate.exists() and is_safe_tool_path(candidate):
                candidates.append(str(candidate))
    candidates = list(dict.fromkeys(candidates))
    path = candidates[0] if candidates else None
    if not path:
        return {"available": False, "path": None, "version": None, "reason": "not in a safe toolchain location"}
    try:
        r = safe_run([path, *version_args], capture_output=True, text=True, timeout=20, env=sanitize_environment())
    except FileNotFoundError:
        return {"available": False, "path": path, "version": None, "reason": "binary missing"}
    if r.returncode == 0:
        version = (r.stdout or r.stderr).splitlines()[0] if (r.stdout or r.stderr) else "unknown"
        return {"available": True, "path": path, "version": version, "reason": "ok"}
    return {"available": False, "path": path, "version": None, "reason": (r.stderr or r.stdout or "version check failed")}


def detect_python_module(module_name: str):
    code = (
        "import importlib; "
        f"module = importlib.import_module({module_name!r}); "
        "print(getattr(module, '__version__', 'unknown'))"
    )
    cmd = [sys.executable, "-c", code]
    try:
        result = safe_run(cmd, capture_output=True, text=True, timeout=20, env=sanitize_environment())
    except FileNotFoundError:
        return {"available": False, "version": None, "source": "python-module", "error": "python not available"}
    if result.returncode == 0:
        return {"available": True, "version": (result.stdout or "unknown").strip(), "source": "python-module"}
    return {"available": False, "version": None, "source": "python-module", "error": (result.stderr or result.stdout or "module not importable").strip()}


def detect_runtime_status():
    report = {
        "cpython": {"available": True, "path": sys.executable, "version": sys.version.split()[0]},
        "pypy": detect_binary("pypy3", ["--version"]),
        "numba": detect_python_module("numba"),
        "nuitka": detect_binary("nuitka", ["--version"]),
        "codon": detect_binary("codon", ["--version"]),
        "tarvos": {"available": False, "path": None, "version": None, "reason": "not detected"},
        "rust": detect_binary("rustc", ["--version"]),
    }
    if report["rust"]["available"]:
        report["rust"]["cargo"] = detect_binary("cargo", ["--version"])
    else:
        report["rust"]["cargo"] = {"available": False, "path": None, "version": None, "reason": "rustc missing"}
    tarvos_binary = None
    for candidate in [
        ROOT / "target" / "x86_64-pc-windows-gnu" / "release" / "tarvos.exe",
        ROOT / "target" / "x86_64-pc-windows-gnu" / "release" / "tarvos",
        ROOT / "target" / "release" / "tarvos.exe",
        ROOT / "target" / "release" / "tarvos",
        ROOT / "target" / "debug" / "tarvos.exe",
        ROOT / "target" / "debug" / "tarvos",
    ]:
        if candidate.exists():
            tarvos_binary = candidate
            break
    if tarvos_binary is not None:
        report["tarvos"] = {"available": True, "path": str(tarvos_binary), "version": "built binary", "reason": "ok"}
    return report


def benchmark_script_runtime(runtime_name: str, script_path: Path, repeats: int = 5):
    runtime_name = runtime_name.lower()
    script_path = script_path.resolve()
    if runtime_name == "cpython":
        times = []
        for _ in range(repeats):
            start = time.perf_counter()
            proc = subprocess.run([sys.executable, str(script_path)], capture_output=True, text=True, cwd=str(script_path.parent), timeout=60)
            if proc.returncode != 0:
                raise subprocess.CalledProcessError(proc.returncode, [sys.executable, str(script_path)], output=proc.stdout, stderr=proc.stderr)
            times.append(time.perf_counter() - start)
        return {"runtime": runtime_name, "avg_s": sum(times) / len(times), "min_s": min(times), "max_s": max(times), "samples": times}

    if runtime_name == "pypy":
        pypy = shutil.which("pypy3") or shutil.which("pypy")
        if pypy is None:
            return {"runtime": runtime_name, "status": "unavailable", "reason": "pypy3 not installed"}
        times = []
        for _ in range(repeats):
            start = time.perf_counter()
            proc = subprocess.run([pypy, str(script_path)], capture_output=True, text=True, cwd=str(script_path.parent), timeout=60)
            if proc.returncode != 0:
                raise subprocess.CalledProcessError(proc.returncode, [pypy, str(script_path)], output=proc.stdout, stderr=proc.stderr)
            times.append(time.perf_counter() - start)
        return {"runtime": runtime_name, "avg_s": sum(times) / len(times), "min_s": min(times), "max_s": max(times), "samples": times}

    if runtime_name == "numba":
        return {"runtime": runtime_name, "status": "unavailable", "reason": "Numba requires a workload-specific njit wrapper; generic script execution is not a valid benchmark"}

    if runtime_name == "nuitka":
        compiler = shutil.which("nuitka")
        if compiler is None:
            return {"runtime": runtime_name, "status": "unavailable", "reason": "nuitka not installed"}
        with tempfile.TemporaryDirectory(prefix="nuitka_runtime_", dir=str(BUILD_TMP_ROOT)) as tmpdir:
            output_dir = Path(tmpdir)
            exe_name = script_path.stem if script_path.stem else "out"
            compile_proc = subprocess.run([compiler, str(script_path), "--output-dir", str(output_dir), "--assume-yes", "--remove-output"], capture_output=True, text=True, cwd=str(script_path.parent), timeout=180)
            if compile_proc.returncode != 0:
                raise subprocess.CalledProcessError(compile_proc.returncode, [compiler, str(script_path), "--output-dir", str(output_dir)], output=compile_proc.stdout, stderr=compile_proc.stderr)
            candidates = list(output_dir.rglob(f"{exe_name}.exe")) + list(output_dir.rglob(f"{exe_name}.cmd")) + list(output_dir.rglob(f"{exe_name}"))
            exe = next((p for p in candidates if p.is_file()), None)
            if exe is None:
                raise FileNotFoundError(f"No Nuitka output executable found for {script_path.name}")
            times = []
            for _ in range(repeats):
                start = time.perf_counter()
                proc = subprocess.run([str(exe)], capture_output=True, text=True, cwd=str(script_path.parent), timeout=60)
                if proc.returncode != 0:
                    raise subprocess.CalledProcessError(proc.returncode, [str(exe)], output=proc.stdout, stderr=proc.stderr)
                times.append(time.perf_counter() - start)
            return {"runtime": runtime_name, "avg_s": sum(times) / len(times), "min_s": min(times), "max_s": max(times), "samples": times}

    if runtime_name == "codon":
        codon = shutil.which("codon")
        if codon is None:
            return {"runtime": runtime_name, "status": "unavailable", "reason": "codon not installed"}
        times = []
        for _ in range(repeats):
            start = time.perf_counter()
            proc = subprocess.run([codon, "run", str(script_path)], capture_output=True, text=True, cwd=str(script_path.parent), timeout=60)
            if proc.returncode != 0:
                raise subprocess.CalledProcessError(proc.returncode, [codon, "run", str(script_path)], output=proc.stdout, stderr=proc.stderr)
            times.append(time.perf_counter() - start)
        return {"runtime": runtime_name, "avg_s": sum(times) / len(times), "min_s": min(times), "max_s": max(times), "samples": times}

    if runtime_name == "tarvos":
        binary = ensure_tarvos_binary()
        with tempfile.TemporaryDirectory(prefix="tarvos_runtime_bench_", dir=str(BUILD_TMP_ROOT)) as tmpdir:
            out_rs = Path(tmpdir) / "runtime_bench.rs"
            out_bin = Path(tmpdir) / ("runtime_bench.exe" if os.name == "nt" else "runtime_bench")
            times = []
            proc = safe_run([binary, "compile", str(script_path), "--output", str(out_rs)], capture_output=True, text=True, cwd=str(ROOT), timeout=60, env=sanitize_environment())
            if proc.returncode != 0:
                raise subprocess.CalledProcessError(proc.returncode, [binary, "compile", str(script_path), "--output", str(out_rs)], output=proc.stdout, stderr=proc.stderr)
            rustc_cmd = rustc_command(["-O", "-o", str(out_bin), str(out_rs)])
            compile_proc = safe_run(rustc_cmd, capture_output=True, text=True, timeout=60, env=sanitize_environment())
            if compile_proc.returncode != 0:
                raise subprocess.CalledProcessError(compile_proc.returncode, rustc_cmd, output=compile_proc.stdout, stderr=compile_proc.stderr)
            for _ in range(repeats):
                start = time.perf_counter()
                run_proc = safe_run([str(out_bin)], capture_output=True, text=True, timeout=60, env=sanitize_environment())
                if run_proc.returncode != 0:
                    raise subprocess.CalledProcessError(run_proc.returncode, [str(out_bin)], output=run_proc.stdout, stderr=run_proc.stderr)
                times.append(time.perf_counter() - start)
            return {"runtime": runtime_name, "avg_s": sum(times) / len(times), "min_s": min(times), "max_s": max(times), "samples": times}

    if runtime_name == "rust":
        return {"runtime": runtime_name, "status": "unavailable", "reason": "No hand-written Rust reference exists for the workload corpus in this repository"}

    return {"runtime": runtime_name, "status": "unsupported", "reason": "unknown runtime type"}


def detect_cache_state(path: Path):
    if path.exists():
        return "warm"
    return "cold"


def build_row(py_file, rs_file, name, preset_name, repeats, cache_aware=False):
    preset = apply_preset(preset_name)
    py_time = benchmark_python(py_file, repeats=repeats)
    tarvos_run_time, tarvos_compile_time = benchmark_tarvos(py_file, repeats=repeats)
    tarvos_total = tarvos_run_time + tarvos_compile_time
    rust_run_time, rust_compile_time = benchmark_rust_file(rs_file, repeats=repeats)
    rust_total = rust_run_time + rust_compile_time
    py_vs_tarvos = py_time / tarvos_run_time
    py_vs_rust = py_time / rust_run_time
    py_vs_tarvos_cold = py_time / tarvos_total
    py_vs_rust_cold = py_time / rust_total
    cache_status = detect_cache_state(rs_file) if cache_aware else "n/a"
    return {
        "name": name,
        "python_file": str(py_file),
        "rust_file": str(rs_file),
        "preset": preset_name,
        "cranelift": preset["cranelift"],
        "python_total_s": py_time,
        "tarvos_total_s": tarvos_total,
        "rust_total_s": rust_total,
        "tarvos_run_s": tarvos_run_time,
        "rust_run_s": rust_run_time,
        "python_ms": py_time * 1000,
        "tarvos_ms": tarvos_total * 1000,
        "rust_ms": rust_total * 1000,
        "tarvos_run_ms": tarvos_run_time * 1000,
        "rust_run_ms": rust_run_time * 1000,
        "tarvos_compile_ms": tarvos_compile_time * 1000,
        "rust_compile_ms": rust_compile_time * 1000,
        "python_vs_tarvos": py_vs_tarvos,
        "python_vs_rust": py_vs_rust,
        "python_vs_tarvos_cold": py_vs_tarvos_cold,
        "python_vs_rust_cold": py_vs_rust_cold,
        "cache_status": cache_status,
    }


def run_benchmark(py_file, rs_file, name, preset_name="dev", repeats=5, cache_aware=False):
    preset = apply_preset(preset_name)
    print(f"\n{'=' * 50}\n{name}\n{'=' * 50}")
    print(f"Profile: {preset_name.upper()} | Cranelift: {'yes' if preset['cranelift'] else 'no'}")

    result = build_row(py_file, rs_file, name, preset_name, repeats, cache_aware=cache_aware)

    py_ms = result["python_ms"]
    tarvos_ms = result["tarvos_ms"]
    rust_ms = result["rust_ms"]
    tarvos_compile_ms = result["tarvos_compile_ms"]
    rust_compile_ms = result["rust_compile_ms"]
    tarvos_run_ms = max(0.0001, tarvos_ms - tarvos_compile_ms)
    rust_run_ms = max(0.0001, rust_ms - rust_compile_ms)

    exec_speedup_vs_python = py_ms / tarvos_run_ms
    exec_speedup_vs_rust = rust_run_ms / tarvos_run_ms
    total_speedup_vs_python = py_ms / tarvos_ms

    print(f"\n1. Execution Performance (Pure Computing Runtime):")
    print(f"   Python:     {py_ms:8.2f} ms")
    print(f"   Rust:       {rust_run_ms:8.2f} ms")
    print(f"   Tarvos: {tarvos_run_ms:8.2f} ms")
    print(f"   Computing Speedup vs Python: {exec_speedup_vs_python:8.2f}x faster")
    if exec_speedup_vs_rust >= 1.0:
        print(f"   Computing Speedup vs Rust:   {exec_speedup_vs_rust:8.2f}x faster (Optimizer Won!)")
    else:
        print(f"   Computing Ratio vs Rust:     {exec_speedup_vs_rust:8.2f}x")

    print(f"\n2. Total Pipeline Time (including Transpilation & rustc):")
    print(f"   Tarvos total: {tarvos_ms:8.2f} ms (transpile+rustc: {tarvos_compile_ms:.2f} ms, run: {tarvos_run_ms:.2f} ms)")
    print(f"   Rust total:       {rust_ms:8.2f} ms (rustc: {rust_compile_ms:.2f} ms, run: {rust_run_ms:.2f} ms)")
    print(f"   Total Speedup vs Python:     {total_speedup_vs_python:8.2f}x")

    print("\nProduct summary:")
    print(f"  Python baseline (single run): {format_ms(result['python_total_s'])}")
    print(f"  Tarvos compute runtime:   {tarvos_run_ms:8.2f} ms vs Python {py_ms:8.2f} ms ({exec_speedup_vs_python:.2f}x faster)")
    print(f"  Tarvos cold start:        {format_ms(result['tarvos_total_s'])} total (transpile + rustc + run)")
    print(f"  Rust cold start:             {format_ms(result['rust_total_s'])} total")
    if tarvos_ms < py_ms:
        print(f"  Recommendation: Tarvos wins on repeated execution; cold-start compile cost dominates one-off runs.")
    else:
        print(f"  Recommendation: for one-off runs, prefer the Python baseline; for deployed or repeated execution, compile once and run many times.")
    print(f"  Main cost driver: {'Rust compile overhead' if rust_compile_ms > tarvos_compile_ms else 'transpile + generated Rust compile time'}")
    if cache_aware:
        print(f"  Cache status: {result['cache_status']}")

    return result


def export_results(results, csv_path=None, json_path=None):
    if csv_path:
        csv_path = Path(csv_path)
        csv_path.parent.mkdir(parents=True, exist_ok=True)
        with csv_path.open("w", newline="") as handle:
            writer = csv.DictWriter(
                handle,
                fieldnames=[
                    "name",
                    "preset",
                    "cranelift",
                    "python_file",
                    "rust_file",
                    "python_total_s",
                    "tarvos_total_s",
                    "rust_total_s",
                    "python_ms",
                    "tarvos_ms",
                    "rust_ms",
                    "tarvos_run_ms",
                    "rust_run_ms",
                    "tarvos_compile_ms",
                    "rust_compile_ms",
                    "python_vs_tarvos",
                    "python_vs_rust",
                    "python_vs_tarvos_cold",
                    "python_vs_rust_cold",
                    "cache_status",
                ],
            )
            writer.writeheader()
            writer.writerows(results)

    if json_path:
        json_path = Path(json_path)
        json_path.parent.mkdir(parents=True, exist_ok=True)
        json_path.write_text(json.dumps(results, indent=2))


def print_matrix_summary(results):
    if not results:
        return
    print("\nBenchmark matrix summary (execution runtime only):")
    print(f"{'Case':<24} {'Preset':<10} {'Python':>10} {'Tarvos':>12} {'Rust':>10} {'Winner':>8}")
    for item in results:
        winner = "Rust" if item["rust_run_s"] < item["tarvos_run_s"] else "Tarvos"
        print(
            f"{item['name']:<24} {item['preset']:<10} "
            f"{item['python_ms']:>10.1f} {item['tarvos_run_ms']:>12.1f} "
            f"{item['rust_run_ms']:>10.1f} {winner:>8}"
        )


def run_benchmark_matrix(preset_names, repeats=5, cache_aware=False, csv_path=None, json_path=None):
    cases = discover_benchmark_cases()
    results = []
    if not cases:
        raise FileNotFoundError("No benchmark cases with matching .py/.rs pairs were found in examples/")

    for preset_name in preset_names:
        for py_file, rs_file, name in cases:
            result = run_benchmark(py_file, rs_file, name, preset_name=preset_name, repeats=repeats, cache_aware=cache_aware)
            results.append(result)

    print_matrix_summary(results)
    export_results(results, csv_path=csv_path, json_path=json_path)
    return results


def run_runtime_benchmark_matrix(workload_dir: Path, runtime_names=None, repeats=5, json_output=None):
    if runtime_names is None:
        runtime_names = ["cpython", "tarvos"]
    runtime_names = [r.lower() for r in runtime_names]
    detected = detect_runtime_status()
    entries = []
    workloads = sorted(workload_dir.glob("*.py"))
    if not workloads:
        raise FileNotFoundError(f"No workload .py files found in {workload_dir}")

    for runtime_name in runtime_names:
        if runtime_name not in {"cpython", "pypy", "numba", "nuitka", "codon", "tarvos", "rust"}:
            entries.append({"runtime": runtime_name, "status": "unsupported", "reason": "unknown runtime name"})
            continue
        if runtime_name == "cpython":
            available = True
            reason = "cpython is always available in this benchmark harness"
        elif runtime_name == "pypy":
            available = detected.get("pypy", {}).get("available", False)
            reason = "toolchain not installed on this machine"
        elif runtime_name == "numba":
            available = detected.get("numba", {}).get("available", False)
            reason = "toolchain not installed on this machine"
        elif runtime_name == "nuitka":
            available = detected.get("nuitka", {}).get("available", False)
            reason = "toolchain not installed on this machine"
        elif runtime_name == "codon":
            available = detected.get("codon", {}).get("available", False)
            reason = "toolchain not installed on this machine"
        elif runtime_name == "tarvos":
            available = detected.get("tarvos", {}).get("available", False)
            reason = "toolchain not installed on this machine"
        elif runtime_name == "rust":
            available = False
            reason = "No hand-written Rust reference exists for the workload corpus in this repository"
        if not available:
            entries.append({"runtime": runtime_name, "status": "unavailable", "reason": reason})
            continue

        for workload in workloads:
            try:
                result = benchmark_script_runtime(runtime_name, workload, repeats=repeats)
            except Exception as exc:  # pragma: no cover - runtime failure reporting
                result = {"runtime": runtime_name, "workload": workload.name, "status": "failed", "reason": str(exc)}
            result["workload"] = workload.name
            entries.append(result)

    if json_output:
        json_path = Path(json_output)
        json_path.parent.mkdir(parents=True, exist_ok=True)
        json_path.write_text(json.dumps(entries, indent=2), encoding="utf-8")

    print("\nRuntime benchmark matrix:")
    for entry in entries:
        if entry.get("status") in {"unavailable", "unsupported"}:
            print(f"  - {entry['runtime']}: {entry['status']} ({entry.get('reason', 'no reason')})")
            continue
        if entry.get("status") == "failed":
            print(f"  - {entry['runtime']} / {entry['workload']}: failed ({entry.get('reason', 'no reason')})")
            continue
        avg_ms = entry.get("avg_s", 0.0) * 1000
        print(f"  - {entry['runtime']} / {entry['workload']}: {avg_ms:.2f} ms average")
    return entries


def parse_args():
    parser = argparse.ArgumentParser(description="Benchmark Python versus Tarvos and handwritten Rust")
    parser.add_argument("py_file", nargs="?", default=str(ROOT / "examples" / "simple.py"), help="Python file to benchmark")
    parser.add_argument("rs_file", nargs="?", default=str(ROOT / "examples" / "simple.rs"), help="Reference Rust file to benchmark")
    parser.add_argument("name", nargs="?", default="Python vs Tarvos vs Rust Benchmark", help="Benchmark title")
    parser.add_argument("--preset", choices=["dev", "release", "cranelift"], default="dev", help="Benchmark profile preset")
    parser.add_argument("--presets", nargs="*", default=None, help="Run a preset matrix (e.g. --presets dev release cranelift)")
    parser.add_argument("--repeats", type=int, default=5, help="Number of iterations for each benchmark")
    parser.add_argument("--cache-aware", action="store_true", help="Report whether the benchmark is running with warm or cold source caches")
    parser.add_argument("--export-csv", type=str, default=None, help="Write benchmark results to a CSV file")
    parser.add_argument("--export-json", type=str, default=None, help="Write benchmark results to a JSON file")
    parser.add_argument("--matrix", action="store_true", help="Run the benchmark matrix across examples and selected presets")
    parser.add_argument("--runtime-matrix", action="store_true", help="Run the benchmark matrix across CPython, PyPy, Numba, Nuitka, Codon, Tarvos, and Rust when available")
    parser.add_argument("--runtimes", nargs="*", default=None, help="Restrict the runtime benchmark matrix to a subset: cpython pypy numba nuitka codon tarvos rust")
    parser.add_argument("--workloads-dir", type=str, default=str(ROOT / "benchmarks" / "workloads"), help="Directory containing workload .py files for runtime benchmarking")
    parser.add_argument("--rust-opt-level", type=int, choices=[0, 1, 2, 3], default=3, help="Optimization level passed to rustc when compiling generated benchmark binaries")
    parser.add_argument("--cranelift", action="store_true", help="Enable the Cranelift backend for benchmark compilation (nightly-only)")
    return parser.parse_args()


if __name__ == "__main__":
    args = parse_args()
    os.chdir(ROOT)
    if args.cranelift:
        os.environ["TARVOS_USE_CRANELIFT"] = "1"
    else:
        os.environ.pop("TARVOS_USE_CRANELIFT", None)

    if args.rust_opt_level is not None:
        os.environ["TARVOS_RUSTFLAGS"] = " ".join(["-C", f"opt-level={args.rust_opt_level}", "-C", "debuginfo=0", "-C", "codegen-units=1"])

    try:
        if args.runtime_matrix:
            runtimes = args.runtimes if args.runtimes else ["cpython", "pypy", "numba", "codon", "tarvos", "rust"]
            run_runtime_benchmark_matrix(Path(args.workloads_dir), runtime_names=runtimes, repeats=max(1, args.repeats), json_output=args.export_json)
        elif args.matrix or args.presets:
            preset_names = args.presets if args.presets else [args.preset]
            results = run_benchmark_matrix(preset_names, repeats=max(1, args.repeats), cache_aware=args.cache_aware, csv_path=args.export_csv, json_path=args.export_json)
        else:
            result = run_benchmark(Path(args.py_file), Path(args.rs_file), args.name, preset_name=args.preset, repeats=max(1, args.repeats), cache_aware=args.cache_aware)
            export_results([result], csv_path=args.export_csv, json_path=args.export_json)
    except subprocess.CalledProcessError as error:
        details = error.stderr or error.output or "no compiler diagnostics were captured"
        print(f"Benchmark failed: {details}", file=sys.stderr)
        raise SystemExit(error.returncode or 1)
