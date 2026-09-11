import os, sys, subprocess, time, json
from pathlib import Path

def run_cmd(cmd, cwd=None):
    start = time.perf_counter()
    res = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, cwd=cwd)
    elapsed = (time.perf_counter() - start) * 1000
    return res.stdout, res.stderr, res.returncode, elapsed

def benchmark_pure_execution(bin_path, runs=5):
    """Measures pure native binary execution time taking the median of multiple runs."""
    times = []
    last_stdout = ""
    last_stderr = ""
    last_rc = 0
    for _ in range(runs):
        start = time.perf_counter()
        res = subprocess.run([str(bin_path)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        elapsed = (time.perf_counter() - start) * 1000
        times.append(elapsed)
        last_stdout = res.stdout
        last_stderr = res.stderr
        last_rc = res.returncode
        if last_rc != 0:
            break
    times.sort()
    median_time = times[len(times) // 2] if times else 0.0
    return last_stdout, last_stderr, last_rc, median_time

def compare_outputs(py_out, ep_out):
    py_lines = [l.strip() for l in py_out.strip().splitlines() if l.strip()]
    ep_lines = [l.strip() for l in ep_out.strip().splitlines() if l.strip()]
    if len(py_lines) != len(ep_lines):
        return False, f"Line count mismatch: Python={len(py_lines)}, Tarvos={len(ep_lines)}"
    for i, (p_line, e_line) in enumerate(zip(py_lines, ep_lines)):
        if p_line == e_line:
            continue
        try:
            p_val = float(p_line)
            e_val = float(e_line)
            if abs(p_val - e_val) < 1e-5:
                continue
        except ValueError:
            pass
        return False, f"Line {i+1} mismatch: Python='{p_line}', Tarvos='{e_line}'"
    return True, "Match"

def main():
    repo_root = Path(__file__).resolve().parent.parent
    test_dir = repo_root / "validation" / "test_cases"
    temp_dir = repo_root / ".build-tmp" / "validation_bins"
    temp_dir.mkdir(parents=True, exist_ok=True)
    
    target_bin = repo_root / "target" / "release" / ("tarvos.exe" if os.name == "nt" else "tarvos")
    if not target_bin.exists():
        target_bin = repo_root / "target" / "debug" / ("tarvos.exe" if os.name == "nt" else "tarvos")
    if not target_bin.exists():
        print("[+] Building tarvos CLI binary...")
        subprocess.run(["cargo", "build", "--release", "-p", "tarvos-cli"], check=True, cwd=repo_root)
        target_bin = repo_root / "target" / "release" / ("tarvos.exe" if os.name == "nt" else "tarvos")
        
    test_files = sorted(test_dir.glob("*.py"))
    if not test_files:
        print("No test files found in", test_dir)
        sys.exit(1)
        
    print("\n" + "="*95)
    print("  TARVOS 1.2 HIGH-RESOLUTION PERFORMANCE & VALIDATION SUITE")
    print(f"  Measuring {len(test_files)} Workloads (CPython vs Native Binary Pure Runtime)")
    print("="*95 + "\n")
    
    results = []
    passed = 0
    failed = 0
    
    header = f"{'Test Case':<24} | {'Status':<7} | {'CPython (ms)':<12} | {'Compile (ms)':<12} | {'Pure Run (ms)':<14} | {'Speedup':<9}"
    print(header)
    print("-" * 95)
    
    for tf in test_files:
        test_name = tf.stem
        exe_path = temp_dir / (test_name + (".exe" if os.name == "nt" else ""))
        
        # 1. Measure CPython execution runtime (median of runs)
        py_times = []
        py_out, py_err, py_rc = "", "", 0
        for _ in range(3):
            out, err, rc, el = run_cmd([sys.executable, str(tf)], cwd=repo_root)
            py_times.append(el)
            py_out, py_err, py_rc = out, err, rc
            if rc != 0:
                break
        py_times.sort()
        py_time = py_times[len(py_times) // 2]
        
        if py_rc != 0:
            print(f"{test_name:<24} | {'PY ERR':<7} | {py_time:<12.2f} | {'N/A':<12} | {'N/A':<14} | {'N/A':<9}")
            failed += 1
            results.append({"test": test_name, "status": "PY_ERROR", "error": py_err})
            continue
            
        # 2. Measure Tarvos AOT Transpile & Build Time
        build_cmd = [str(target_bin), "build", str(tf), "-o", str(exe_path)]
        build_out, build_err, build_rc, compile_time = run_cmd(build_cmd, cwd=repo_root)
        if build_rc != 0 or not exe_path.exists():
            print(f"{test_name:<24} | {'BUILD ERR':<7} | {py_time:<12.2f} | {compile_time:<12.2f} | {'N/A':<14} | {'N/A':<9}")
            print("  --> Build Error: " + build_err.strip()[:120])
            failed += 1
            results.append({"test": test_name, "status": "BUILD_ERROR", "error": build_err})
            continue
            
        # 3. Measure Pure Native Binary Execution Time (no rustc overhead)
        ep_out, ep_err, ep_rc, pure_run_time = benchmark_pure_execution(exe_path, runs=7)
        if ep_rc != 0:
            print(f"{test_name:<24} | {'RUN ERR':<7} | {py_time:<12.2f} | {compile_time:<12.2f} | {'N/A':<14} | {'N/A':<9}")
            print("  --> Runtime Error: " + ep_err.strip()[:120])
            failed += 1
            results.append({"test": test_name, "status": "RUN_ERROR", "error": ep_err})
            continue
            
        # 4. Compare outputs for strict functional parity
        ok, reason = compare_outputs(py_out, ep_out)
        
        # Calculate true native speedup
        speedup_val = py_time / max(pure_run_time, 0.0001)
        speedup_str = f"{speedup_val:.1f}x"
        
        if ok:
            print(f"{test_name:<24} | {'PASSED':<7} | {py_time:<12.2f} | {compile_time:<12.2f} | {pure_run_time:<14.3f} | {speedup_str:<9}")
            passed += 1
            results.append({
                "test": test_name,
                "status": "PASSED",
                "py_ms": py_time,
                "compile_ms": compile_time,
                "pure_run_ms": pure_run_time,
                "speedup": speedup_str
            })
        else:
            print(f"{test_name:<24} | {'FAILED':<7} | {py_time:<12.2f} | {compile_time:<12.2f} | {pure_run_time:<14.3f} | {'N/A':<9}")
            print("  --> Parity Mismatch: " + reason)
            failed += 1
            results.append({"test": test_name, "status": "FAILED", "reason": reason})
            
    print("\n" + "="*95)
    pct = (passed / len(test_files)) * 100.0
    print(f"  Summary: {passed} passed, {failed} failed out of {len(test_files)} tests ({pct:.1f}%)")
    print("="*95 + "\n")
    
    report_path = repo_root / "validation" / "validation_report.json"
    with open(report_path, "w") as f:
        json.dump(results, f, indent=2)
        
    if failed > 0:
        sys.exit(1)
    else:
        print("[OK] All validation tests passed with 100% output parity and verified pure native speedups!\n")

if __name__ == "__main__":
    main()
