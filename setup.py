import os
from pathlib import Path

from setuptools import find_packages, setup
from setuptools.command.install import install


ROOT = Path(__file__).parent


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
    version="1.5.0",
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
