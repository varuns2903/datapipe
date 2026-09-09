# Contributing to DataPipe

## Prerequisites

- A recent stable Rust toolchain ([rustup.rs](https://rustup.rs))

## Setup

```bash
git clone https://github.com/varuns2903/datapipe.git
cd datapipe
cargo build
```

## Running tests

```bash
cargo test
```

## Before submitting a PR

Run the same checks CI runs:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
```

If `cargo fmt --all -- --check` fails, just run `cargo fmt` to fix it.

## Project layout

- `src/cli.rs` — clap CLI/subcommand definitions
- `src/expr.rs` — the expression lexer/parser/evaluator used by `filter` and `map`
- `src/stages.rs` — pipeline stage implementations (filter, sort, aggregations, join, etc.)
- `src/io.rs` — JSON/CSV streaming input and output
- `src/pipeline.rs` — the `Stage` trait and `Pipeline` that chains stages together
- `src/model.rs` — the `Value`/`Record` data model and value comparison logic
- `src/par_iter.rs` — the parallel (rayon-based) streaming filter iterator
- `tests/cli.rs` — end-to-end CLI tests via `assert_cmd`
- `benches/bench.rs` — Criterion benchmarks

## Adding a new pipeline stage

1. Add a struct implementing `Stage` in `src/stages.rs`, with unit tests alongside the other stage tests at the bottom of that file.
2. Add a subcommand variant in `src/cli.rs` with a `///` doc comment (this becomes the `--help` text).
3. Wire it up in the `match cli.command` block in `src/lib.rs`.
4. Document the new command in `README.md`.

## Adding a new expression operator

Operators are defined in `src/expr.rs`: add the token in `lex`, the operator in `Operator`, the parsing rule in the appropriate precedence level (`parse_or` / `parse_and` / `parse_cmp` / `parse_term` / `parse_factor`), and the evaluation logic in `Expr::evaluate`. Add tests to the `tests` module at the bottom of the file — cover both the happy path and the type-mismatch/edge-case behavior.
