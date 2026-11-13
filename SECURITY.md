# Security Policy

## Supported Versions

| Version | Supported |
| ------- | ------------------------------ |
| 1.1.x (release candidates) | :white_check_mark: |
| 1.0.x | :white_check_mark: |
| < 1.0 | :x: (update to a supported release) |

Security fixes are applied to the current release line only. Older tags remain
available but receive no backports.

## Reporting a Vulnerability

Please **do not open a public issue** for security vulnerabilities.

Report privately using GitHub's Security Advisories for
`repo-tech/tarvos`:

1. Open the **Security** tab of the repository.
2. Choose **Report a vulnerability** (Security Advisory).
3. Describe the affected component (compiler, CLI, gateway, sandbox,
   installer), a minimal reproduction, and the impact.

You can expect:

- An acknowledgement when a maintainer reads the report.
- An assessment of whether the report reproduces against the current `main`
  branch.
- Credit in the release notes if you want it (reports are private until a fix
  ships).

Out of scope / not vulnerabilities:

- Crashes or wrong output of the compiler on **untrusted Python source** are
  correctness bugs — open a normal issue instead. The compiler treats all
  input as untrusted, but it is not a sandbox.
- The native sandbox (Docker terminal) intentionally runs user code; issues
  about **escaping** that sandbox, accessing the host, or exceeding resource
  limits are in scope and welcome privately.

## Compiler and sandbox security notes

- The gateway never executes uploaded code inside the web server process; it
  compiles to a native artifact and runs the terminal only inside the
  constrained Docker sandbox (non-root, `--network=none`, dropped caps,
  memory/CPU/pids limits).
- The optional local AI probe is read-only, loopback-only, and never starts,
  downloads, or stops any process.
- No source code, telemetry, or secrets are uploaded anywhere by the CLI.
