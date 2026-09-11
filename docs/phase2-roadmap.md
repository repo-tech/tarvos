# Phase 2 Roadmap: Hardened Compiler Runtime

## Goal

Turn Tarvos from a promising subset compiler into a hardened, reproducible, and trusted build tool for static numeric Python workloads.

## Milestone 1: Secure execution boundary

Deliverables:
- enforced project-local input/output validation
- restricted tool discovery for Python, Rust, and benchmark tooling
- sanitized environment for child processes
- explicit rejection of attempts to write outside the safe workspace

Acceptance criteria:
- arbitrary outside paths are rejected
- PATH poisoning is mitigated by resolved-safe executable discovery
- compiler and benchmark flows still work for the supported workload set

## Milestone 2: Compiler correctness hardening

Deliverables:
- strict AST node allowlist with explicit unsupported-node rejection
- branch assignment and initialization verification coverage
- expanded workload corpus for arithmetic, branching, loops, and nested control flow
- stronger differential tests against CPython

Acceptance criteria:
- every supported node type has a proven translation path
- all audited workloads pass exact-output comparisons
- unsupported constructs fail fast with actionable diagnostics

## Milestone 3: Benchmark integrity and reproducibility

Deliverables:
- a pinned benchmark manifest for workload sets and expected outputs
- single-source benchmark configuration for runtime matrix generation
- JSON/CSV export for repeatable reporting
- stable competitor environment detection with clear unavailability reasons

Acceptance criteria:
- benchmark results can be reproduced from a clean environment
- runtime availability is not overstated
- compile and execution times are measured honestly and separately

## Milestone 4: Release packaging and distribution

Deliverables:
- packaged CLI release for supported platforms
- reproducible build scripts and pinned dependency versions
- signed or traceable artifacts for the released binary
- final user-facing docs with subset disclaimers and safe usage guidelines

Acceptance criteria:
- install steps are deterministic
- release artifacts are verifiable
- end users understand the product is a compiler for a subset, not a general Python replacement

## Milestone 5: Performance engineering for the real pipeline

Deliverables:
- optimize the transpile + Rust compile path without hiding costs
- profile codegen, optimizer passes, and rustc latency separately
- measure end-to-end overhead across the supported subset

Acceptance criteria:
- the benchmark harness reports compile and runtime separately
- optimization work is measured against the real pipeline, not cached or synthetic shortcuts
- the project can state meaningful performance claims with evidence

## Risks to manage

- overclaiming compatibility beyond the audited subset
- PATH hijacking or untrusted environment poisoning
- benchmark results that hide compile costs or rely on stale caches
- shipping a tool that executes arbitrary user code without clear trust boundaries

## Exit criteria for Phase 2

Phase 2 is complete when Tarvos is:
- secure enough to run in a trusted local environment
- confirmed correct for the audited subset
- benchmarked with honest, reproducible evidence
- packaged and documented as a subset compiler with explicit boundaries
