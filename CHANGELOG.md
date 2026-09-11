# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed
- `--in-csv` no longer corrupts a field whose value is literally the text
  `"NaN"`, `"inf"`, `"Infinity"`, or `"-inf"` (any case). Rust's
  `f64::from_str` accepts these as valid floats, but JSON has no
  representation for non-finite numbers, so such a field was silently
  parsed to a non-finite float and then serialized as JSON `null` on
  output - a real value quietly turning into `null` with no warning.
  Numeric inference now checks `f64::is_finite()` before accepting a
  float parse; a non-finite parse falls through and the field is kept as
  its original string, consistent with the existing leading-zero-integer
  guard (`"00501"` staying a string rather than becoming `501`). Legit
  finite numbers, including negative floats, are unaffected. Verified
  with new `io` unit tests and a CLI integration test.

### Documentation
- Documented that `group`/`freq`'s output already composes with `filter`
  for a SQL-style `HAVING` clause (e.g. `dp group category --count | dp
  filter '.count > 10'`) - no code change was needed, this already worked
  since `group`'s output is just another JSONL stream, but it wasn't
  spelled out anywhere and is a natural thing to reach for.

### Added
- `topn <fields> <n>`: keeps only the top `n` records by a `sort`-style
  field spec (same syntax, e.g. `score:desc`), without buffering or
  sorting the whole stream. Maintains a bounded max-heap of at most `n`
  items (O(n log k) time, O(k) memory where `k = min(n, stream length)`),
  evicting the current worst-kept record whenever a better candidate
  arrives once the heap is full - unlike `sort <fields> | limit <n>`,
  which fully sorts (spilling to temp files) before truncating. Produces
  identical output to that pipeline, just without the unnecessary work
  and disk I/O when only a handful of extremes are needed out of a huge
  stream. Implemented as `TopNStage`/`TopNItem`, reusing `sort`'s
  `cmp_by_sort_fields()` comparator and `parse_sort_spec()` parsing.
  Available in pipeline files as `type = "topn"` (`fields` + `n`).
  Verified: unit tests against `sort | limit` for both ascending and
  descending single/multi-field specs, `n` larger than the stream,
  `n = 0`, and error propagation; CLI and pipeline-file integration
  tests.
- `map`: repeatable `--set FIELD=EXPRESSION` for computing multiple fields
  in one pass, in addition to the existing primary `<field> <expression>`
  positional pair (fully backward compatible - the primary assignment
  behaves exactly as before when `--set` isn't used). Assignments are
  applied in order, so a later `--set` can reference a field computed by
  an earlier one in the same `map` call, e.g. `dp map total '.price *
  .qty' --set tax='.total * 0.1' --set grand_total='.total + .tax'`.
  Previously this required three separate `map` stages piped together.
  `MapStage` now holds `Vec<(String, Expr)>` instead of a single
  `(field, ast)` pair; expression-parsing error handling was factored out
  into a shared `parse_expr()` helper (previously duplicated between
  `filter` and `map`) so both the primary assignment and every `--set`
  entry get the same miette-rendered diagnostics on a syntax error.
- Multi-field support for `sort`, `unique`, `group`, and `join --on`. Real
  tabular data routinely needs a composite key (e.g. sort by country then
  age, join on region+id), and previously every one of these stages was
  limited to a single field.
  - `sort <fields>`: now takes a comma-separated list, each optionally
    suffixed with `:desc` or `:asc` (ascending is the default), e.g.
    `sort country,age:desc`. Earlier fields take precedence; later ones
    only break ties. Implemented via `parse_sort_spec()` and a shared
    `cmp_by_sort_fields()` comparator reused by both the in-chunk sort and
    the k-way merge's `HeapItem` ordering (now holding an
    `Rc<Vec<(String, bool)>>` instead of a single field, to avoid a
    per-heap-push allocation).
  - `unique <fields>` and `group <by>`: now take a comma-separated field
    list forming a composite key, internally joined with a control
    character (`\u{1}`) to avoid key collisions across a field-count
    boundary. `group`'s output also now preserves each `by` field's
    original `Value` type instead of always stringifying it (a
    side-effect improvement: the old single-field code path stringified
    even integer/boolean group keys).
  - `join --on <fields>`: now takes a comma-separated list; a record only
    matches when *all* fields agree. Applies to both the default hash
    join and `--merge`'s sort-merge join, which now sorts and compares by
    the full composite key (`cmp_key_values()`) rather than one field.
  - **Breaking CLI/pipeline-file change**: `sort`'s standalone `--desc`
    flag is removed in favor of the `field:desc` suffix syntax (`sort
    age --desc` → `sort age:desc`); pipeline-file `[[stages]]` entries
    for `sort`/`unique` now take a single `fields` string instead of
    `field`(`+desc`) - see updated README/mdBook examples.
  - Verified: new unit tests for `parse_sort_spec`/`parse_field_list`
    validation, multi-field sort (primary+secondary, mixed asc/desc),
    composite-key `unique`/`group`/`join`/`join --merge`, plus end-to-end
    CLI integration tests for each. All pre-existing single-field tests
    pass unchanged (a single field is just a one-element list).
- Unary minus in `filter`/`map` expressions: `-5`, `-.field`, and nested
  forms like `--5` or `3 - -5` now parse and evaluate correctly. Previously
  only binary subtraction was supported, so producing a negative value
  required a workaround like `0 - 5`. Implemented by desugaring `-<expr>`
  to `0 - <expr>` at parse time in `parse_unary`, reusing the existing
  `Operator::Sub` evaluation (which already handles Integer/Float
  promotion correctly) instead of adding a dedicated AST variant. Note: if
  the expression itself starts with `-` (e.g. `-.age`), it needs `--`
  before it on the command line so the CLI parser doesn't mistake it for a
  flag, e.g. `dp map delta -- '-.age'` — documented in README and mdBook.
- `join --merge`: a memory-bounded sort-merge join alternative to the
  default hash join. `join <file>` normally loads `<file>` entirely into
  memory as a hash table before the main stream starts, which is fine for
  lookup-table-sized files but unbounded for huge ones. `--merge` instead
  sorts both the main stream and `<file>` by the join key (reusing the same
  external merge sort `sort` already uses, spilling 50k-record chunks to
  temp files rather than buffering fully) and merges them with a two-pointer
  scan, so memory stays bounded regardless of `<file>`'s size. Implemented
  as `MergeJoinStage`/`MergeJoinIter` using a "step + queue" iterator
  pattern (`step()` performs one atomic unit of work — advancing a pointer
  or emitting a matched group — pushing zero or more results into a
  `VecDeque` that `next()` drains), which avoids the correctness pitfalls of
  a hand-rolled resumable state machine spanning multiple `next()` calls.
  `--merge` has two deliberate behavioral differences from the default hash
  join, both documented in the README and mdBook: for a duplicate join key,
  the hash join keeps only the *last* matching record from `<file>`
  (a hash-table insert overwrites earlier ones), while `--merge` produces
  the full cross product of every matching left/right pair, which is the
  textbook-correct sort-merge join behavior; and `--merge`'s output comes
  out in join-key-sorted order rather than the main stream's original
  order, since sorting is inherent to the algorithm. Supports all four join
  types (`left`/`inner`/`right`/`full`). Verified against the hash join for
  unique keys, all four join types, duplicate-key cross-products on both
  sides, empty-stream edges on both sides, and at a 60,000-record scale
  that exercises the external sort's 50k-record chunk boundary.

### Fixed
- `external_sort` (used by `sort` and `join --merge`) silently dropped
  malformed records instead of honoring `--strict`, unlike every other
  stage in the pipeline. The chunk-filling loop treated `Some(Err(_))` the
  same as `None` (end of input), so a parse error partway through the
  stream silently truncated the sort instead of aborting under `--strict`.
  Fixed to distinguish the two cases and abort with the error, matching
  the abort-on-error convention used by every other eager stage
  (`sum`/`avg`/`min`/`max`/`group`/`schema`). Discovered while building
  `join --merge` above (both share the same external-sort code path).

### Added
- Date/time functions in `filter`/`map` expressions: `to_unix(a)` parses an
  RFC3339 datetime or bare `"YYYY-MM-DD"` date into a Unix timestamp,
  enabling date-range filtering via ordinary integer comparison; `year(a)`,
  `month(a)`, `day(a)` extract calendar components; `now()` returns the
  current Unix timestamp. Previously there was no way to filter or extract
  from timestamp fields at all, despite how common they are in real-world
  JSON. Added the `chrono` crate (minimal features: `clock`, `std` only)
  as a dependency. Verified `to_unix`'s output against a hand-computed
  timestamp rather than just eyeballing a plausible-looking number.
- `search <text> [--regex]` command: keeps records where *any* field's
  value contains the given text, without needing to know the field names
  up front (`xsv search`'s equivalent). Previously the only way to search
  across unknown/wide-schema fields was hand-writing
  `contains(.a,"x") || contains(.b,"x") || ...` for every field. With
  `--regex`, the pattern is compiled once (not per-record), same
  principle as the `matches()` expression function. Made
  `value_to_display_string` (used by the `csv`/`table` writers)
  `pub(crate)` so search renders field values identically to those
  writers rather than duplicating the logic a third time.
- `dedup` command: drops exact duplicate records (comparing every field),
  keeping the first occurrence (`xsv dedup`'s equivalent). Complements
  `unique <field>`, which only dedupes on one field - `dedup` only drops
  a record if it matches an earlier one in *every* field.
- Numeric functions in `filter`/`map` expressions: `round(a)`, `floor(a)`,
  `ceil(a)` (all return an integer), `abs(a)` (preserves numeric type),
  and `least(a, b)` / `greatest(a, b)` (SQL-style naming, chosen to avoid
  confusion with the existing `min`/`max` *subcommands* - a different
  namespace, but easy to conflate). `least`/`greatest` work on any value
  type via the same total ordering `sort` already uses, not just numbers.
  Found and worked around a pre-existing, already-documented limitation
  while writing tests: there's no unary minus, so `abs(-5)` doesn't parse
  - use `abs(0 - 5)`.
- `freq <field> [--limit <n>]` command: counts occurrences of each distinct
  value in a field, sorted most-frequent first, with each value's
  percentage of the stream (`xsv frequency`'s equivalent). Unlike
  `group --count`, which is unsorted and has no percentage, this is built
  for quick data exploration and composes naturally with `dp table`.
- `stats` command: computes `count`/`nulls`/`distinct`/`min`/`max`/`mean`/
  `stddev` for every field in a single pass, yielding one summary record
  per field - composes naturally with `dp table` for a readable profiling
  view. Previously getting this required N separate `min`/`max`/`sum`/`avg`
  invocations per field, one full stream pass each. `mean`/`stddev` are
  `null` for non-numeric fields; `min`/`max` work for any type via the
  existing `cmp_values` ordering. `distinct` tracking has the same
  proportional-to-cardinality memory tradeoff as `unique`/`group`.
  Verified the stddev computation against the classic textbook example
  (2,4,4,4,5,5,7,9 → mean 5, population stddev 2).
- `table` output command: an aligned, human-readable table (header row,
  dashed separator, data rows) instead of JSONL/CSV — like `column -t` or
  `mlr --opprint`. Unlike every other output writer here, this buffers the
  entire stream first, since column widths depend on every value seen;
  documented that tradeoff explicitly. Also available as an `out_table`
  setting in pipeline files, mirroring `out_csv`. Refactored the CSV/JSON
  output-format selection from a `bool` into an `OutputFormat` enum to
  make room for the third format cleanly.
- `select --exclude` mode: `<fields>` becomes an exclusion list (keep
  everything except the named fields, field order preserved) instead of
  the default inclusion list, e.g. `dp select password,secret --exclude`.
  Previously `select` was allow-list only, so dropping a couple of fields
  from a wide/unknown-schema record meant naming every other field.
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
