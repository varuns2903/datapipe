# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `LICENSE` file (MIT).
- CI workflow (`fmt`, `clippy`, `test` matrix across Linux/macOS/Windows, `cargo-audit`).
- Release automation via `cargo-dist`: tagged pushes build cross-platform binaries
  (Linux x86_64/aarch64, macOS x86_64/aarch64, Windows x86_64) and publish them to
  GitHub Releases with shell/PowerShell installers and checksums.
- Unit test coverage for the expression parser (`expr.rs`) and all pipeline stages
  (`stages.rs`), including the external-merge sort path.

### Fixed
- CLI integration tests referenced the old binary name `datapipe` instead of `dp`.
- Various clippy warnings and a deprecated `criterion::black_box` usage.

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
