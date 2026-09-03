# DataPipe (`dp`)
[![Crates.io](https://img.shields.io/crates/v/datapipe-cli.svg)](https://crates.io/crates/datapipe-cli)


DataPipe is a streaming-first, Unix-inspired CLI for processing structured data (JSON, CSV). 
Instead of operating on raw text strings, `dp` operates on structured records natively, allowing you to filter, sort, aggregate, and transform gigabytes of data with strict O(1) memory bounds.

## Features
- **Streaming by Default:** Pipeline stages process data lazily. You can filter a 100GB JSONL file using virtually 0MB of RAM.
- **Unified Data Model:** Seamlessly pipe data between formats (`JSONL -> CSV` or `CSV -> JSONL`).
- **Custom Expression Engine:** A handwritten, recursive descent parser allows for powerful conditional filtering (`.age > 25 && .admin == true`).
- **Stateful Aggregations:** Easily compute statistics (`sum`, `avg`, `min`, `max`, `count`) directly in the shell.
- **High Performance:** Capable of processing hundreds of thousands of records per second on a single thread.

## Installation

You can install DataPipe globally directly from crates.io:

```bash
cargo install datapipe-cli
```
*(This will install the `dp` binary to your `~/.cargo/bin` directory)*

Alternatively, you can build from source:
```bash
cargo install --path .
```

## Quick Start

Process a stream of JSON records, filter them, sort them, and output as CSV:

```bash
cat examples/users.jsonl | dp filter '.age >= 21' | dp sort age --desc | dp csv
```

## Available Commands

### Transformations
- `filter <expression>`: Keeps only records where the expression evaluates to `true`.
- `select <fields>`: Keeps only the specified comma-separated fields.
- `limit <max>`: Halts the stream after yielding `N` records.

### Stateful Operations
*(Note: These operations must buffer the stream into memory)*
- `sort <field> [--desc]`: Sorts the records by the specified field.
- `unique <field>`: Keeps only the first occurrence of each unique value in a field.

### Aggregations
- `count`: Consumes the stream and yields the total record count.
- `sum <field>`: Computes the sum of a numeric field.
- `avg <field>`: Computes the average of a numeric field.
- `min <field>` / `max <field>`: Finds the minimum/maximum value.

### Schema & Formatting
- `schema`: Inspects the stream and infers the data types of all fields.
- `csv`: Outputs the resulting stream as a CSV instead of JSONL.
- `--in-csv`: A global flag to read the input as CSV instead of JSONL.

## Expressions
The `filter` command supports:
- Operators: `==`, `!=`, `<`, `>`, `<=`, `>=`, `&&`, `||`
- Literals: Strings (`"value"`), Integers (`42`), Booleans (`true`, `false`)
