# Getting Help with Tarvos

## Read the docs first

- `docs/compiler-status.md` — what the compiler actually supports today,
  feature by feature, with the documented compatibility boundary.
- `docs/ARCHITECTURE.md` — how the pipeline fits together.
- `INSTALL.md` — installation and `tarvos doctor` verification.
- `RELEASE_NOTES.md` — what changed in each released version.

## Bug reports and feature requests

Open a GitHub issue using the provided templates:

- **Compiler bug** — needs a minimal Python example, expected CPython
  output, and actual Tarvos output/diagnostic.
- **Feature request** — describe the Python construct or workflow and why
  the native subset should cover it.

Check existing issues before filing a duplicate.

## Security issues

Do **not** file public issues for vulnerabilities. Follow
[SECURITY.md](SECURITY.md) and use a private GitHub Security Advisory.

## "It fell back to CPython" questions

A `tarvos run` message about the local Python runtime or a
`--python-fallback` diagnostic is expected behavior when a program uses
constructs outside the native subset. The diagnostic names the construct.
If you believe a construct *should* be natively supported and the docs say
it is, that is a compiler bug — file it with a minimal reproduction.

## What is not available (yet)

- There is no paid support, SLA, or hosted chat at this time.
- Community chat/discussion channels will be linked here if and when they
  exist; nothing is linked right now.

## Troubleshooting checklist

1. `tarvos --version` — confirms the binary on PATH.
2. `tarvos doctor` — validates toolchain, cache, and optional local AI
   capability.
3. `cargo test --workspace` — from a source checkout, confirms your build.
4. Re-run with `--python-fallback` to compare against CPython behavior.