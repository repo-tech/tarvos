# Tarvos Use Cases

## Why this matters

Tarvos is not a replacement for the entire Python ecosystem. It is a compiler for a defined, reliable subset of Python that is especially useful for numeric, loop-heavy, and deterministic compute workloads.

That makes it valuable for teams that want:

- Python-friendly development ergonomics
- faster execution for critical compute kernels
- safer, more predictable deployment paths
- native output for performance-sensitive workloads
- clear bounds around compatibility instead of vague “works everywhere” claims

## Who should use it

Tarvos fits teams that already write Python logic for:

- scientific and engineering calculations
- pricing and risk engines
- batch data processing
- simulation workloads
- ETL transformation logic
- financial and operational analytics
- infrastructure automation with numeric processing
- compute-intensive internal tooling

It is best for workload owners who need speed without rewriting the entire business logic in C++, Rust, or Go.

## Core use cases

### 1. Python scripts that are mostly numeric loops

A large number of business workloads are written in Python because the team wants fast prototyping, but the actual runtime cost comes from long arithmetic loops.

Example:

```python
def compute_total(limit: int) -> int:
    total = 0
    for i in range(limit):
        total += i * i
    return total

print(compute_total(1_000_000))
```

This kind of code is a natural fit for Tarvos because it is static, loop-driven, and arithmetic-heavy.

Benefits:

- lower execution latency than CPython for supported workloads
- generated Rust output can be compiled to a native executable
- easier to deploy than shipping a Python runtime in a constrained environment

### 2. ETL and transformation kernels

Teams often build Python ETL jobs that perform numeric aggregation, filtering, scoring, ranking, or normalization. These scripts may not need the full Python standard library or dynamic runtime features, but they do need to process large volumes of data deterministically.

Example:

```python
def normalize_score(x: float) -> float:
    return (x * 10.0) / 100.0

values = [1.0, 2.0, 3.0, 4.0]
for i in range(len(values)):
    values[i] = normalize_score(values[i])

print(values)
```

In a real enterprise environment this can become:

- risk calculation jobs
- signal preprocessing pipelines
- model feature scoring
- pricing and tax computation
- internal data operations tools

For these workflows, Tarvos helps by converting stable compute kernels into native binaries while preserving a Python-first authoring workflow.

### 3. Internal tooling and operational automation

Many companies use Python for internal tools, but those tools frequently end up as shell utilities or scheduled jobs. If the core logic is numeric and loop-heavy, Tarvos can help convert the script into a native executable with clearer operational boundaries.

Example scenarios:

- nightly report generators
- batch simulation runners
- internal calculators
- backend support automation
- dashboard data-prep tasks

This matters because compiled executables are often easier to:

- schedule in cron / Task Scheduler / CI
- deploy across machines
- isolate from interpreter drift
- run without a user environment setup

### 4. Performance-sensitive prototypes before full rewrite

Tarvos is useful when engineering teams want to prototype in Python and gradually move the hot path to native code without rewriting everything into Rust or C++ from day one.

This is especially helpful for:

- research teams
- product engineering teams
- data science prototypes turning into production jobs
- quant or financial teams moving from notebook code to deployable workloads

The product works best when the code is close to a pure compute kernel: loops, arithmetic, functions, branching, and numerical transforms.

## How to use Tarvos in a Python workflow

### Option A: Source-only generation

Use this when you want generated Rust source without requiring a local Rust toolchain.

```bash
tarvos compile my_workload.py output.rs
```

This is ideal for:

- CI pipelines
- reviewable generated code
- portability across development machines
- teams that want a source artifact first

### Option B: Direct native executable

Use this when you want a final compiled binary.

```bash
tarvos build my_workload.py app.exe
```

Or the shorthand path:

```bash
tarvos my_workload.py output.rs
```

This is useful when the workload is intended for:

- deployment in internal systems
- fast repeated execution
- production scheduling
- environments with no Python runtime dependency

### Option C: Run and benchmark the workload

```bash
tarvos run my_workload.py
```

```bash
tarvos benchmark my_workload.py my_workload.rs --repeats 10
```

This makes Tarvos useful for teams comparing Python baseline behavior against generated native output.

## Example: a realistic enterprise workload

```python
def pricing_model(base: int, quantity: int, tax_rate: float) -> float:
    subtotal = base * quantity
    tax = subtotal * tax_rate
    total = subtotal + tax
    return total

result = 0.0
for i in range(50000):
    result += pricing_model(120, 3, 0.18)

print(round(result, 2))
```

This is exactly the kind of logic that can live in a Python prototype and then be compiled to native code for heavier repeated use.

## Why it helps a large company or global team

For an enterprise environment, Tarvos is useful because it gives engineering teams a middle path:

- Python remains the authoring language
- the subset is statically analyzable
- generated code is native and faster
- deployment is more stable than a full Python runtime
- compatibility boundaries are explicit and understandable

This is especially relevant for companies that:

- have legacy Python services with hot compute paths
- need to keep business logic readable for analysts and engineers
- want to reduce cost of repeated compute in batch systems
- need a path to native performance without large rewrite effort

## Honest positioning for production adoption

Tarvos is not intended to replace the entire Python ecosystem or general-purpose Python execution.

It is best suited for:

- deterministic numeric workloads
- static loop-heavy logic
- subset-based translation to Rust
- well-bounded internal runtime acceleration

It is not designed for:

- arbitrary dynamic Python programs
- full web frameworks or runtime-heavy libraries
- undocumented runtime introspection
- broad untyped Python compatibility

This honesty is actually a strength in enterprise settings because teams know what they are adopting, what it optimizes for, and where the limits are.

## Recommended adoption pattern

For a mature engineering team, the best path is:

1. keep the Python prototype
2. identify the hot compute subset
3. validate the logic against CPython
4. compile via Tarvos
5. benchmark and compare execution
6. deploy the generated native output for the stable path

This gives a practical release model without overpromising universal Python support.

## Bottom line

Tarvos is a strong fit when a team wants to keep Python as the development language but needs native execution for a clearly defined subset of performance-sensitive workloads.

It is especially effective for:

- compute-heavy internal tools
- numeric batch jobs
- pricing and simulation logic
- loop-intensive data processing
- production-ready performance wins without full-language rewrite

This is the right product framing for real-world adoption in professional engineering environments and enterprise-scale teams.
