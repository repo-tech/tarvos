# Tarvos: Product Strategy and Global Adoption Roadmap

- Status: Working strategy
- Date: 2026-11-04
- Audience: Maintainers, contributors, design partners, and potential investors

## 1. Product thesis

Tarvos should become a compiler and execution platform that helps developers take a supported program from source to multiple deployment environments through one coherent toolchain.

The long-term ambition is broad: native applications, WebAssembly, backend/server workloads, cloud deployment, and embedded/IoT. The near-term product must be narrower and demonstrably excellent. A wide target list without verified compatibility is not a product advantage.

**Working positioning:** Tarvos is a multi-target compiler platform, beginning with a Python-compatible frontend and a reliable native execution path.

This positioning must be revised as the language, runtime, and target support are proven. Do not claim that Tarvos compiles arbitrary Python or that every target is production-ready.

## 2. Who should adopt it first?

Do not target every developer persona simultaneously. Start with users whose problem can be solved and measured today.

### Initial developer audience
- Python developers with performance-sensitive, self-contained workloads.
- Teams that need to package selected Python programs as native executables.
- Compiler/runtime engineers who want to evaluate a modular compiler pipeline.

### Design-partner audience
- Small engineering teams willing to provide representative programs and correctness feedback.
- Open-source maintainers with repeatable workloads and CI.
- Infrastructure teams only after reproducible startup, memory, CPU, and deployment measurements exist.

### Later audiences
- Web developers needing a WASM deployment target.
- Cloud teams evaluating cold start, resource use, and cost.
- Embedded developers after actual target triples, linker scripts, board support, and hardware tests exist.

## 3. Product pillars

### A. Trustworthy compiler
- Explicit language compatibility matrix.
- Deterministic, actionable diagnostics.
- Differential tests against the reference Python implementation for supported semantics.
- Reproducible builds and pinned toolchains.
- No silent semantic changes or fallback that is presented as native compilation.

### B. Multi-target architecture
- Shared frontend, semantic analysis, and intermediate representation.
- Explicit backend contract and target capability model.
- Runtime/ABI boundary documented per target.
- Native Rust path retained as the initial reference backend.
- WASM and embedded backends introduced as independently tested vertical slices.

### C. Developer experience
- Clear installation and quick start.
- One-command compile/run workflow where technically valid.
- Useful error messages with source locations.
- Examples that run in CI.
- Editor integration and language-server support only after core diagnostics stabilize.

### D. Measured performance
Every performance claim must publish:
- Exact Tarvos and baseline versions.
- Hardware, operating system, toolchain, and runtime.
- Workload source and invocation.
- Warm and cold measurements where relevant.
- Median and tail latency, memory, binary size, and correctness result.
- Reproduction instructions.

Never claim universal speedups from a single benchmark.

### E. Enterprise readiness
- Versioned compatibility policy.
- Security reporting and dependency update process.
- Software bill of materials and release provenance.
- Reproducible release artifacts.
- Support and maintenance policy.
- Clear licensing and trademark ownership.

## 4. Execution roadmap and exit gates

### Phase 0 — Establish truth (now)
Deliverables:
- Architecture decision records for target/backend boundaries.
- Audit of existing native and embedded paths.
- Baseline test and benchmark inventory.
- Accurate feature/target support matrix.
- Reproducible contributor setup.

Exit gate: maintainers can state exactly what works, what is experimental, and what is unsupported.

### Phase 1 — Native reliability and backend boundary
Deliverables:
- Extract the current Rust codegen behind a minimal backend interface.
- Preserve native output behaviour and differential test results.
- Add tests for target selection and unsupported targets.
- Keep backend-independent crates free of concrete backend dependencies.

Exit gate: all existing workspace and differential tests pass; no regression is accepted without explicit review.

### Phase 2 — First WASM vertical slice
Deliverables:
- Choose a WASI profile and supported runtime based on toolchain evaluation.
- Compile a deliberately small supported program to WASM.
- Run it in CI under a real runtime.
- Compare output, exit status, and errors against native and reference Python.
- Document imports, runtime assumptions, and unsupported features.

Exit gate: a user can reproduce compile, run, and test from a clean environment.

### Phase 3 — Developer adoption
Deliverables:
- Installation and package distribution.
- Quick-start tutorials and realistic examples.
- Versioned language compatibility matrix.
- Benchmark suite with independent reproduction instructions.
- Issue templates, contribution guide, security policy, and release cadence.

Exit gate: external developers can install Tarvos and complete a documented task without maintainer intervention.

### Phase 4 — Backend and cloud workloads
Deliverables:
- HTTP/API examples only after networking/runtime support is explicit.
- Container deployment example.
- AWS Lambda experiment with cold-start, memory, duration, package size, and correctness measurements.
- Cost calculator based on published workload measurements and current provider pricing.

Exit gate: show a reproducible workload where Tarvos offers a meaningful operational advantage; do not promise lower bills in general.

### Phase 5 — Embedded/IoT
Deliverables:
- Supported architecture/target-triple list.
- Linker/startup/runtime integration.
- Explicit memory and allocation model.
- Hardware abstraction strategy.
- At least one board-level example built and tested on real hardware.
- CI compile checks plus hardware-in-the-loop testing where practical.

Exit gate: a documented supported board/toolchain combination works end-to-end. A `no_std` source file alone is not board support.

## 5. Adoption and distribution strategy

1. Make the public repository understandable in five minutes.
2. Publish a working quick start and a small set of compelling examples.
3. Invite external contributors only after the build/test process is reliable.
4. Find 3–5 design partners with real workloads; record their baseline and success criteria.
5. Publish benchmark methodology and negative results as well as positive results.
6. Build integrations where users already work: CI, package managers, editors, and deployment pipelines.
7. Seek ecosystem partnerships only when the relevant target is stable and demonstrable.

## 6. Business model hypotheses to validate

Potential models—not commitments:
- Open-source core with paid enterprise support.
- Managed build/test infrastructure for teams.
- Enterprise distribution, compliance, and long-term support.
- Commercial tooling for fleet-wide compilation and deployment.

Do not add paywalls before validating which user problem is valuable enough to pay for. First establish repeated usage and retention.

## 7. Investor-readiness evidence

A credible technical and business data room should eventually contain:
- Clear product positioning and differentiation.
- Architecture and threat-model documents.
- Public compatibility and target-support matrices.
- Release history and reproducible benchmarks.
- Adoption metrics: active users, repeat usage, contributors, downstream projects.
- Design-partner references and documented customer outcomes.
- Roadmap with completed milestones and engineering risks.
- Licensing, IP ownership, dependency, and security records.
- Hiring plan and use of funds tied to measurable milestones.

Funding and valuation cannot be guaranteed. The goal is to build evidence that makes the opportunity legible and investable.

## 8. Operating principles

- Correctness before performance.
- Working vertical slices before broad claims.
- Public roadmap, measurable milestones.
- No fake support, fake benchmarks, or inflated compatibility.
- Preserve a known-good release path.
- Small pull requests with tests and review.
- Security and supply-chain hygiene from the beginning.
- Treat user trust as a core product feature.

## 9. Immediate next engineering tasks

1. Complete source audit of the native compilation path and generated runtime helpers.
2. Inventory current test suites and identify semantic versus code-shape tests.
3. Design the smallest backend contract that fits the existing Rust codegen.
4. Implement the extraction on an isolated branch and run all available CI gates.
5. Only then start the first WASM runtime experiment.
