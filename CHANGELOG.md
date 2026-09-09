# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `--strict` global flag to abort the pipeline on the first malformed record.
- Nested field access in `filter`/`map` expressions via dotted paths, e.g.
  `.user.age` or `.a.b.c`. Previously only flat top-level fields were
  supported, which was a significant gap given how common nested JSON is.
- Parentheses `( )` to override expression precedence, and a unary `!` (not)
  operator, e.g. `!(.status == "banned") && (.age >= 18 || .verified == true)`.
  Previously there was no way to express arbitrary grouping at all.

### Changed
- `sort`'s external-merge temp files are now kept alive for the full duration
  of reading (via `ExternalSortIter`) instead of being dropped immediately
  after opening each one. This removes a dependency on platform-specific
  delete-while-open file semantics that the previous code relied on
  implicitly without guaranteeing it.

### Fixed
- **Correctness bug**: aggregations (`count`, `sum`, `avg`, `min`, `max`, `group`,
  `schema`) could silently produce wrong results on malformed input. `count`
  specifically counted `Err` items as if they were valid records, and
  `serde_json`'s streaming deserializer stopped yielding entirely after the
  first parse error, silently dropping every valid record after a bad line.
- `read_json_stream` now parses true line-delimited JSON (one `serde_json::from_str`
  call per line) instead of a single continuous `Deserializer` stream, so a
  malformed line no longer prevents subsequent valid lines from being read.
- By default, a malformed record is now skipped with a warning on stderr and
  processing continues, instead of the whole pipeline dying on the first bad
  line with no way to process the rest of a large file.

## [0.1.1] - 2026-09-09

### Added
- `LICENSE` file (MIT).
- CI workflow (`fmt`, `clippy`, `test` matrix across Linux/macOS/Windows, `cargo-audit`).
- Release automation via `cargo-dist`: tagged pushes build cross-platform binaries
  (Linux x86_64/aarch64, macOS x86_64/aarch64, Windows x86_64) and publish them to
  GitHub Releases with shell/PowerShell installers and checksums.
- Homebrew tap publishing (`varuns2903/homebrew-tap`) on release.
- Unit test coverage for the expression parser (`expr.rs`) and all pipeline stages
  (`stages.rs`), including the external-merge sort path.
- `CONTRIBUTING.md`, an expression grammar reference, and documented error/exit-code
  behavior in the README. `--help` doc comments for every subcommand.
- Documented previously-undocumented commands: `unique`, `group`, `join`, `explode`,
  `map`, `inspect`.

### Fixed
- CLI integration tests referenced the old binary name `datapipe` instead of `dp`.
- Various clippy warnings and a deprecated `criterion::black_box` usage.
- `map` with an invalid expression panicked (exit 101) instead of failing gracefully
  (exit 1) like `filter` does.

### Changed
- `Cargo.lock` is now committed for reproducible binary builds.
- Codebase formatted with `cargo fmt`.

## [0.1.0] - 2026-09-03

Initial release.

### Added
- Streaming pipeline engine for JSON/CSV structured data.
- Stages: `filter`, `select`, `limit`, `sort`, `unique`, `explode`, `map`, `schema`,
  `group`, `join`.
- Aggregations: `count`, `sum`, `avg`, `min`, `max`.
- Handwritten recursive-descent expression parser for `filter` conditions.
- CSV and JSONL input/output support.
- Parallel streaming filter via `rayon`.
