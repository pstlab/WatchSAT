# WatchSAT

[![Rust](https://img.shields.io/badge/Rust-1.95+-orange?logo=rust)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-MIT-green)](LICENSE)
![Build Status](https://github.com/pstlab/WatchSAT/actions/workflows/rust.yml/badge.svg)
[![codecov](https://codecov.io/gh/pstlab/WatchSAT/branch/main/graph/badge.svg)](https://codecov.io/gh/pstlab/WatchSAT)

SAT engine in Rust based on CDCL (Conflict-Driven Clause Learning) with the Two-Watched Literals (2WL) scheme for efficient propagation.

The crate exposes a lightweight, reusable solver core designed to be embedded in planning, verification, or reasoning systems.

## Features

- CDCL with 1-UIP-style conflict analysis
- Boolean propagation with two watched literals per clause
- Simple API for variables, literals, and clauses
- Variable-assignment callbacks via listeners
- Safe Rust implementation with unit tests

## Installation

In `Cargo.toml`:

```toml
[dependencies]
watchsat = "0.1"
```

Or from a local source checkout:

```toml
[dependencies]
watchsat = { path = "../WatchSAT" }
```

## Quick Example

```rust
use watchsat::{Engine, LBool, neg, pos};

fn main() {
	let mut engine = Engine::new();

	let a = engine.add_var();
	let b = engine.add_var();

	// (not a or b)
	engine.add_clause(vec![neg(a), pos(b)]).unwrap();

	// Decide a = true, then b is propagated to true.
	engine.assert(pos(a)).unwrap();

	assert_eq!(engine.value(b), LBool::True);
}
```

## Core Concepts

- `VarId`: variable identifier created by `Engine::add_var`
- `Lit`: literal (variable + sign)
- `LBool`: tri-state value (`True`, `False`, `Undef`)
- `Engine`: solver state (assignments, watch lists, clauses)

Important: variable index `0` and literals `TRUE_LIT`/`FALSE_LIT` are reserved internally by the solver.

## Essential API

- `Engine::new()`: create an empty engine
- `Engine::add_var()`: add a variable
- `Engine::add_clause(...)`: insert a clause
- `Engine::assert(lit)`: make a decision and propagate
- `Engine::value(var)`: read the current value of a variable
- `Engine::lit_value(lit)`: read the current value of a literal
- `Engine::add_listener(var, callback)`: register assignment callback

## Conflict Handling

Insertion or propagation operations can return:

- `PropagationError::Conflict { clause }`

The conflict clause is a learned clause produced by 1-UIP analysis.

## Running Tests

```bash
cargo test
cargo clippy --all-targets --all-features
```
