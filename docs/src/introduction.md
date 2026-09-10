# DataPipe (`dp`)

DataPipe is a streaming-first, Unix-inspired CLI for processing structured data (JSON, CSV).
Instead of operating on raw text strings, `dp` operates on structured records natively, allowing you to filter, sort, aggregate, and transform gigabytes of data with a small memory footprint.

[![Crates.io](https://img.shields.io/crates/v/datapipe-cli.svg)](https://crates.io/crates/datapipe-cli)

## Features

- **Streaming by Default:** Transformation stages (`filter`, `select`, `limit`, `explode`, `map`) and simple single-value aggregations (`count`, `sum`, `avg`, `min`, `max`) process data lazily with O(1) memory, independent of input size. `sort` is memory-bounded too, via an external merge sort that spills to temp files rather than buffering the whole stream.

  `unique`, `group`, `schema`, and `join` are the exception: they hold state proportional to the number of *distinct* keys (or, for `schema`, up to the first 10,000 records; for `join`, the entire right-hand file) rather than the main stream's length — fine for typical cardinality and typical lookup-table sizes, but not O(1) if you `unique`/`group` a field with an enormous number of distinct values, or `join` against a huge file.

- **Unified Data Model:** Seamlessly pipe data between formats (`JSONL -> CSV` or `CSV -> JSONL`).
- **Custom Expression Engine:** A handwritten, recursive descent parser allows for powerful conditional filtering (`.age > 25 && .admin == true`), string functions, regex, and membership tests.
- **Stateful Aggregations:** Easily compute statistics (`sum`, `avg`, `min`, `max`, `count`) directly in the shell.
- **High Performance:** Capable of processing hundreds of thousands of records per second on a single thread.

## Where to go next

- New to `dp`? Start with [Installation](./installation.md) and [Quick Start](./quick-start.md).
- Looking for a specific command? See [Commands](./commands/README.md).
- Writing a `filter`/`map` condition? See [Expressions](./expressions.md).
- Chaining many stages together? See [Pipeline Files](./pipeline-files.md).
