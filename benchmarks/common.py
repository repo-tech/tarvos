import os
import shlex
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


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
        if dir_path.exists() and (
            dir_path == ROOT or ROOT in dir_path.parents or any(root in dir_path.parents for root in safe_tool_roots()) or "System32" in str(dir_path) or "Windows" in str(dir_path)
        ):
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
    return cmd + flags
