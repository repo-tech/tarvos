import os
import re
from pathlib import Path

from setuptools import find_packages, setup
from setuptools.command.install import install


ROOT = Path(__file__).parent


def _project_version() -> str:
    """Return the single Python-side version, read from `pyproject.toml`.

    The Python packaging metadata used to hard-code its own version (and had
    drifted to a value the compiler never shipped). `pyproject.toml` is the
    authoritative Python source; `scripts/check_version_consistency.py` keeps it
    aligned with the Cargo workspace version.
    """
    text = (ROOT / "pyproject.toml").read_text(encoding="utf-8")
    match = re.search(r'(?m)^version\s*=\s*"([^"]+)"', text)
    if match is None:
        raise RuntimeError("pyproject.toml does not declare a project version")
    return match.group(1)


class InstallWithOptionalPrefetch(install):
    """Keep wheel installation offline; optionally prefetch the verified CLI binary."""

    user_options = install.user_options + [
        ("prefetch-binary", None, "Download the native CLI during installation"),
    ]

    boolean_options = install.boolean_options + ["prefetch-binary"]

    def initialize_options(self):
        super().initialize_options()
        self.prefetch_binary = False

    def finalize_options(self):
        super().finalize_options()
        self.prefetch_binary = self.prefetch_binary or (
            os.environ.get("TARVOS_PREFETCH", "").lower() in {"1", "true", "yes"}
        )

    def run(self):
        super().run()
        if self.prefetch_binary:
            from tarvos.launcher import _download_binary

            _download_binary()


setup(
    name="tarvos",
    version=_project_version(),
    description="Tarvos Python wrapper for the native Python-to-Rust compiler",
    long_description=(ROOT / "README.md").read_text(encoding="utf-8"),
    long_description_content_type="text/markdown",
    packages=find_packages(include=["tarvos", "tarvos.*"]),
    cmdclass={"install": InstallWithOptionalPrefetch},
    entry_points={"console_scripts": ["tarvos=tarvos:main"]},
    python_requires=">=3.8",
    license="MIT",
    classifiers=[
        "Programming Language :: Python :: 3",
        "Operating System :: Microsoft :: Windows",
        "Operating System :: POSIX",
    ],
)
