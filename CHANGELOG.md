# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `--pretty` / `-p` global flag to indent JSON output for human reading.
  Off by default (output stays compact JSONL). Also available as a
  `pretty` setting in pipeline files, combining with the CLI flag the
  same way `strict` does (either being true is enough).
- `concat(a, b, ...)` function in `filter`/`map` expressions, e.g.
  `concat(.first, " ", .last)`. Previously there was no way to build a
  string from multiple values at all - `+` only handles numeric types
  and silently evaluates to `null` on strings. Unlike the other string
  functions, `concat` stringifies any scalar (not just strings) and
  treats `null` as an empty string rather than nulling out the whole
  result, since its purpose is building display text where a missing
  optional field shouldn't break the rest of the string.
- An mdBook documentation site under `docs/`, deployed to GitHub Pages via
  `.github/workflows/docs.yml` on pushes touching `docs/**`. Restructures
  the README's content into browsable chapters (installation, commands,
  pipeline files, expressions, error behavior, known limitations); the
  Contributing chapter embeds `CONTRIBUTING.md` directly via mdBook's
  `{{#include}}` so the two never drift out of sync. Added `documentation`
  to `Cargo.toml` and a docs badge to the README, both pointing at the
  site. Requires enabling "Pages: source = GitHub Actions" once in repo
  settings before the first deploy will actually publish.
- GitHub issue templates (bug report, feature request) and a pull request
  template, under `.github/`.
- `CODE_OF_CONDUCT.md` (Contributor Covenant) and `SECURITY.md`
  (vulnerability reporting policy via GitHub private advisories).

### Documentation
- `CONTRIBUTING.md`: fixed a stale reference to the old `match cli.command`
  block (refactored into `command_into_stage` when pipeline files were
  added) and added `src/pipeline_file.rs` to the project layout list.

## [0.2.0] - 2026-09-10

### Added
- `dp run <pipeline.toml>` for declarative multi-stage pipelines defined in
  a TOML file, run in a single process instead of chaining many `dp`
  invocations with shell pipes. Added the `toml` crate as a dependency.
  Refactored the per-command stage-construction logic in `run_cli` into a
  shared `command_into_stage` function so direct CLI dispatch and pipeline
  files build stages identically, with no duplicated logic.
- `matches(a, "pattern")` regex function in `filter`/`map` expressions, e.g.
  `matches(.email, "^.+@example\.com$")`. The pattern must be a string
  literal so it can be compiled once at parse time rather than recompiled
  on every record - important for keeping large-stream throughput. Added
  the `regex` crate as a dependency.
- `rename <old:new,...>` command to rename one or more fields.
- `flatten [--sep <sep>]` command to flatten nested objects into dot-path
  keys, e.g. `{"user":{"name":"Alice"}}` -> `{"user.name":"Alice"}`. Useful
  before `csv` output, since nested objects otherwise render as `[complex]`.
- `sample <n>` command for uniform random sampling via reservoir sampling
  (single streaming pass, O(n) memory, no need to know stream length up
  front). Added the `rand` crate as a dependency for this.
- `in` membership operator in `filter`/`map` expressions, e.g.
  `.status in ("active", "pending")`. Works with any value type, not just
  strings. Factored out a shared parenthesized-list parser reused by both
  `in (...)` and function-call arguments.
- `join --type <left|inner|right|full>` flag. Previously `join` only
  supported a left join; `inner`, `right`, and `full` are now available too.
- String functions in `filter`/`map` expressions: `contains`, `starts_with`,
  `ends_with`, `lower`, `upper`, e.g. `contains(.name, "Smith")` or
  `lower(.email) == "alice@example.com"`. Previously there was no way to
  do substring matching or case-insensitive comparison at all.
- `dp man` command generating a troff-formatted man page.
- `dp completions <shell>` command generating completion scripts for bash,
  zsh, fish, PowerShell, and elvish.
- `--strict` global flag to abort the pipeline on the first malformed record.
- Nested field access in `filter`/`map` expressions via dotted paths, e.g.
  `.user.age` or `.a.b.c`. Previously only flat top-level fields were
  supported, which was a significant gap given how common nested JSON is.
- Parentheses `( )` to override expression precedence, and a unary `!` (not)
  operator, e.g. `!(.status == "banned") && (.age >= 18 || .verified == true)`.
  Previously there was no way to express arbitrary grouping at all.

### Fixed
- `join` silently dropped malformed records in the join file (`<file>`)
  without any warning, via the same `.flatten()`-discards-errors pattern
  fixed elsewhere in this release. It now respects `--strict`/the default
  skip-with-warning behavior, consistent with the main stream.
- **Correctness bug**: aggregations (`count`, `sum`, `avg`, `min`, `max`, `group`,
  `schema`) could silently produce wrong results on malformed input. `count`
  specifically counted `Err` items as if they were valid records, and
  `serde_json`'s streaming deserializer stopped yielding entirely after the
  first parse error, silently dropping every valid record after a bad line.
  `read_json_stream` now parses true line-delimited JSON (one
  `serde_json::from_str` call per line) instead of a single continuous
  `Deserializer` stream, so a malformed line no longer prevents subsequent
  valid lines from being read. By default, a malformed record is now
  skipped with a warning on stderr and processing continues, instead of
  the whole pipeline dying on the first bad line.

### Documentation
- Added a "Known limitations" section to the README covering: integer
  precision loss beyond `i64::MAX`, potential hash-key collisions in
  `group`/`join`/`unique` (they key non-string values by JSON-serializing
  them), the lack of an input size guard for a single record/line, and
  that `join` loads its entire right-hand file into memory as a hash
  table (same class of tradeoff as `unique`/`group`'s memory usage).
  Considered a streaming sort-merge join instead but scoped it out as a
  larger algorithmic change without clear demand yet.

### Changed
- README no longer claims blanket "O(1) memory bounds" for the whole tool.
  `unique`, `group`, and `schema` hold state proportional to distinct key
  cardinality (or, for `schema`, up to the first 10,000 records), not O(1) -
  this was always true but previously undocumented and inconsistent with the
  README's headline claim.
- `--in-csv` integer inference now requires the parsed value to round-trip
  exactly back to the original text. Previously a zip code or phone number
  with a leading zero (e.g. `"00501"`) would silently become the integer
  `501`, losing the leading zero. Fixed-decimal floats (e.g. `"5.00"`) are
  unaffected and still infer as numeric.
- `sort`'s external-merge temp files are now kept alive for the full duration
  of reading (via `ExternalSortIter`) instead of being dropped immediately
  after opening each one. This removes a dependency on platform-specific
  delete-while-open file semantics that the previous code relied on
  implicitly without guaranteeing it.

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
