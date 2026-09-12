from pathlib import Path

from setuptools import find_packages, setup


ROOT = Path(__file__).parent

setup(
    name="tarvos",
    version="1.5.0",
    description="Tarvos Python wrapper for the native Python-to-Rust compiler",
    long_description=(ROOT / "README.md").read_text(encoding="utf-8"),
    long_description_content_type="text/markdown",
    packages=find_packages(include=["tarvos", "tarvos.*"]),
    entry_points={"console_scripts": ["tarvos=tarvos:main"]},
    python_requires=">=3.8",
    license="MIT",
    classifiers=[
        "Programming Language :: Python :: 3",
        "Operating System :: Microsoft :: Windows",
        "Operating System :: POSIX",
    ],
)
