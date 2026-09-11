# Tarvos Product Overview

## Build faster compute without abandoning Python

Tarvos is a compiler and transpiler for a defined, reliable subset of Python. It turns Python source into optimized Rust and then into native executables when a Rust toolchain is available.

This is not a general-purpose Python runtime replacement. It is a production-friendly solution for teams that want to keep Python as the authoring language while moving performance-critical compute kernels to native execution.

## The problem Tarvos solves

Many teams start with Python because it is fast to write, easy to reason about, and easy for analysts and engineers to maintain. But when the same script is executed repeatedly in loops, simulations, scoring, pricing, data processing, or internal automation, Python can become the bottleneck.

Typical pain points include:

- slow CPython execution on compute-heavy loops
- repeated batch work that is too expensive to run in Python
- difficult deployment of Python-heavy scripts into operational systems
- tradeoff between developer speed and runtime speed
- the need to rewrite large logic blocks in C++, Rust, or Go

Tarvos creates a middle path: keep the Python logic, compile the supported subset to Rust, and generate a fast native binary when needed.

## Why teams choose Tarvos

### Python-first development
Teams can prototype and maintain logic in Python while getting a native execution path for the hot compute kernel.

### Predictable scope
The compiler targets a narrow but useful subset of Python. That means the product is easier to reason about, test, and deploy than a vague “all Python” claim.

### Honest performance story
Tarvos is built for benchmarkable, static workloads. It intentionally emphasizes correctness and transparency over broad compatibility claims.

### Better operational flexibility
For supported workloads, teams can compile to Rust source only or produce a final native executable depending on the environment.

## Best-fit workloads

Tarvos is ideal for:

- numeric and scientific workloads
- loop-heavy compute kernels
- business logic with arithmetic and branching
- pricing and risk calculations
- scoring and normalization pipelines
- internal operational automation
- ETL-style transformation logic
- batch processing jobs

Examples of supported patterns include:

- integer and float arithmetic
- variable assignment and reassignment
- comparisons and branching
- `for` loops and `while` loops
- simple function definitions and returns
- deterministic compute flows written in a statically analyzable style

## What Tarvos does

Tarvos follows a simple pipeline:

```text
Python source
  -> AST export
  -> typed IR / analysis
  -> optimization
  -> Rust source generation
  -> native binary (optional)
```

The product is intentionally designed around a subset that can be lowered to native Rust with predictable behavior.

## Two modes for real-world use

### 1. Source-only mode

This is the safest and most portable mode for teams that want generated Rust without requiring Rust to be installed locally.

```bash
tarvos compile my_script.py output.rs
```

This is useful when:

- you want to review generated code
- you are running in CI/CD
- you want a source artifact before compilation
- you need portability across developer machines

### 2. Native executable mode

If Rust is available, Tarvos can generate and compile a native binary.

```bash
tarvos build my_script.py app.exe
```

Or directly:

```bash
tarvos run my_script.py
```

This path is useful when:

- you need higher execution speed
- the workload is repeated often
- you want a deployable executable
- you want to remove the Python runtime dependency from execution

## Example workflow

```python
def compute_total(limit: int) -> int:
    total = 0
    for i in range(limit):
        total += i * i
    return total

print(compute_total(1_000_000))
```

This is a textbook Tarvos workload:

- static numeric logic
- deterministic behavior
- loop-heavy execution
- suited to native compilation

## Product positioning

Tarvos is best positioned as:

- a modern Python-to-Rust subset compiler
- a performance accelerator for static compute kernels
- a practical bridge between Python development and native deployment
- a tool for teams that want bounded compatibility and real speed

It is not positioned as:

- a general-purpose Python interpreter replacement
- a solution for arbitrary dynamic Python programs
- a universal runtime for all Python workloads

That boundary is a strength, not a weakness. It makes the product honest, testable, and easier to adopt in production.

## Enterprise value

For engineering teams, Tarvos offers a credible path to:

- reduce runtime cost of hot loops
- speed up internal batch jobs
- simplify deployment of compute-heavy scripts
- keep business logic readable in Python
- avoid rewriting everything in lower-level languages immediately

This matters for teams in:

- financial services
- analytics and reporting
- simulation and modeling
- scientific computing
- internal tooling and automation
- data engineering and transformation pipelines

## Simple adoption pattern

A practical adoption model is:

1. write the prototype in Python
2. isolate the hot compute logic
3. validate it against CPython
4. compile it with Tarvos
5. benchmark the generated native output
6. deploy the binary for the optimized path

This keeps engineering velocity high without forcing a premature full rewrite.

## Quick start

Build the CLI:

```bash
cargo build --release --bin tarvos
```

Compile a Python file to Rust:

```bash
tarvos compile examples/simple.py output.rs
```

Build a native executable:

```bash
tarvos build examples/simple.py app.exe
```

Run the workload:

```bash
tarvos run examples/simple.py
```

Benchmark the workload:

```bash
tarvos benchmark examples/simple.py examples/simple.rs --repeats 10
```

## Release posture

Version 0.0.1 is a solid first release focused on a reliable subset, safer execution, and honest benchmark reporting. It is not claiming universal Python compatibility; it is shipping a disciplined compiler for compute-focused workflows.

That gives Tarvos a realistic place in the market: a pragmatic compiler for teams who value speed, clarity, and bounded correctness over broad but unreliable compatibility.

## Bottom line

Tarvos is a compelling solution when a team wants to keep Python as the development language, but needs a faster and more deployable native path for real computational work.

It is especially strong for structured, numeric, loop-heavy Python workloads that fit the supported subset and benefit from Rust-backed execution.
