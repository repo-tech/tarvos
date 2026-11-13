# Tarvos 1.0 Roadmap

## Mission

Take Tarvos from a credible 0.0.1 subset compiler to a production-grade 1.0 release for deterministic, numeric, and loop-heavy Python workloads.

The 1.0 scope is intentionally clear:

- supported Python subset is explicit and tested
- end-to-end correctness is proven against CPython for the audited workload set
- direct native execution is reliable for supported workloads
- benchmark reporting is fair, reproducible, and honest
- the CLI is polished and suitable for developer adoption
- the product is marketed as a subset compiler, not a universal Python runtime

## Strategic goal

Version 1.0 should clearly answer:

- What Python features are supported?
- What output guarantees do we provide?
- What workloads are meant to be compiled and deployed?
- How do users install and validate the tool in a clean environment?
- What is the cost and risk of moving from Python to generated Rust?

## Core 1.0 objectives

### Objective 1: lock the supported language subset

Deliverables:
- stable feature matrix for all supported Python constructs
- explicit rejection of unsupported nodes with actionable diagnostics
- parser and compiler checks for type stability, loop structure, and function contracts
- examples and tests for valid/invalid input coverage

Acceptance criteria:
- every supported construct is mapped to a validated compiler pass
- unsupported features fail fast with clear messages
- the subset remains narrow, auditable, and manageable for 1.0

### Objective 2: strengthen compiler correctness

Deliverables:
- deterministic semantic validation passes
- branch and assignment verification for loops and conditionals
- more robust lowering for range, function calls, and returns
- expanded runtime parity tests against CPython

Acceptance criteria:
- the audited workload corpus passes exact-output comparison for all supported cases
- generated Rust matches CPython semantics for the supported subset
- compiler diagnostics clearly explain where a workload falls outside support

### Objective 3: make the CLI production-ready

Deliverables:
- polished command surface: compile, build, run, analyze, benchmark, doctor
- stable default behavior and consistent exit codes
- clear handling of missing Rust / toolchain fallback flows
- safe path enforcement and no accidental workspace escapes
- content-addressed incremental translation cache for repeated local builds

Acceptance criteria:
- command behavior is consistent across local developer setups
- direct EXE flow works when Rust is available
- source-only flow works without requiring Rust
- unsafe or ambiguous paths are rejected before execution
- unchanged source can be recompiled from the local cache and cleaned explicitly

### Objective 4: build a trustworthy benchmark harness

Deliverables:
- reproducible benchmark script for CPython, Tarvos, and optional competitors
- separate reporting for compile time vs runtime time
- stable fairness workloads that prevent trivial compile-time constant folding
- JSON/CSV artifact export for release benchmarking

Acceptance criteria:
- benchmark results can be rerun in a clean environment
- runtime comparison is fair and documented
- performance claims are tied to the supported subset and explicit workload classes

### Objective 5: package the release for real users

Deliverables:
- installation guide for Linux/macOS/Windows
- release bundle with validation steps
- quick-start examples for common use cases
- public docs for supported subset, limitations, and security posture

Acceptance criteria:
- end users can install and run the tool without repo internals
- release docs clearly explain supported scope and limitations
- the project is ready for outside developer trials

## 1.0 milestone plan

### Milestone 1: language contract freeze

Timeline: weeks 1-2

Focus:
- finalize supported subset specification
- define and document invalid constructs
- freeze grammar-level behavior
- add acceptance tests for every supported pattern

Exit criteria:
- the project can state exactly what belongs to the 1.0 supported subset

### Milestone 2: compiler safety and correctness pass

Timeline: weeks 3-5

Focus:
- validate AST lowering and semantic checks
- fix remaining edge cases in functions, loops, and returns
- add coverage for nested control flow and typed operations
- improve error reporting

Exit criteria:
- supported cases have reproducible parity validation against CPython

### Milestone 3: CLI and build ergonomics

Timeline: weeks 6-7

Focus:
- polish CLI UX and error flows
- add direct EXE and source-only workflows
- improve build/run behavior for Windows and Unix
- validate clean developer install path

Exit criteria:
- a new user can install, compile, and run a supported script without project-specific knowledge

### Milestone 4: benchmark and release quality

Timeline: weeks 8-9

Focus:
- implement reproducible benchmark matrix
- document fairness rules and expected results
- package release artifact and validation docs
- produce benchmark result files for the supported matrix

Exit criteria:
- 1.0 release candidate is benchmarked and documented with evidence

### Milestone 5: release candidate sign-off

Timeline: week 10

Focus:
- full smoke test suite and correctness regression runs
- release notes and changelog review
- security review and final hardening pass
- finalize 1.0 documentation and examples

Exit criteria:
- the project is ready for a stable 1.0 announcement

## Success definition for 1.0

Tarvos 1.0 is successful when it is a trustworthy, documented, and reproducible compiler for a clearly bounded subset of Python that:

- compiles supported workloads reliably
- matches CPython behavior on the audited workload set
- emits correct Rust code for real numeric workflows
- supports both source-only and native binary output modes
- offers fair and transparent benchmark reporting
- communicates limits honestly and clearly

## What 1.0 is not

1.0 will not claim to be:

- a universal Python replacement
- a full web or framework runtime
- compatible with every Python library or dynamic code pattern
- a benchmark winner across all Python workloads

This is intentional. The 1.0 release should be credible because it is honest about its scope and strong within that scope.

## Immediate next actions

The next concrete implementation priorities are:

1. freeze and document the supported subset
2. harden verification and test coverage for all supported constructs
3. polish CLI usability and installation flow
4. finish reproducible benchmark reporting for release artifact generation
5. produce the 1.0 release candidate bundle and docs

This is the correct starting point for the 1.0 phase.
