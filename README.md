# DataPipe (`dp`)
[![Crates.io](https://img.shields.io/crates/v/datapipe-cli.svg)](https://crates.io/crates/datapipe-cli)
[![Docs](https://img.shields.io/badge/docs-book-blue)](https://varuns2903.github.io/datapipe/)


DataPipe is a streaming-first, Unix-inspired CLI for processing structured data (JSON, CSV). 
Instead of operating on raw text strings, `dp` operates on structured records natively, allowing you to filter, sort, aggregate, and transform gigabytes of data with a small memory footprint.

## Features
- **Streaming by Default:** Transformation stages (`filter`, `select`, `limit`, `explode`, `map`) and simple single-value aggregations (`count`, `sum`, `avg`, `min`, `max`) process data lazily with O(1) memory, independent of input size. `sort` is memory-bounded too, via an external merge sort that spills to temp files rather than buffering the whole stream. **`unique`, `group`, `schema`, and `join` are the exception**: they hold state proportional to the number of *distinct* keys (or, for `schema`, up to the first 10,000 records; for `join`, the entire right-hand file) rather than the main stream's length — fine for typical cardinality and typical lookup-table sizes, but not O(1) if you `unique`/`group` a field with an enormous number of distinct values, or `join` against a huge file (e.g. a UUID column over billions of rows, or a multi-GB join file).
- **Unified Data Model:** Seamlessly pipe data between formats (`JSONL -> CSV` or `CSV -> JSONL`).
- **Custom Expression Engine:** A handwritten, recursive descent parser allows for powerful conditional filtering (`.age > 25 && .admin == true`).
- **Stateful Aggregations:** Easily compute statistics (`sum`, `avg`, `min`, `max`, `count`) directly in the shell.
- **High Performance:** Capable of processing hundreds of thousands of records per second on a single thread.

## Installation

### Prebuilt binaries (recommended)

Download a prebuilt binary for your platform from the [latest release](https://github.com/varuns2903/datapipe/releases/latest), or install with the one-line script:

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/varuns2903/datapipe/releases/latest/download/datapipe-cli-installer.sh | sh
```

### Homebrew (macOS/Linux)

```bash
brew install varuns2903/tap/dp
```

### From crates.io

Requires a Rust toolchain.

```bash
cargo install datapipe-cli
```
*(This will install the `dp` binary to your `~/.cargo/bin` directory)*

### From source

```bash
cargo install --path .
```

### Shell completions

`dp` can generate completion scripts for bash, zsh, fish, PowerShell, and elvish via `dp completions <shell>`:

```bash
# bash (persist across sessions by adding this to your ~/.bashrc)
source <(dp completions bash)

# zsh (persist by saving to a directory in your $fpath)
dp completions zsh > "${fpath[1]}/_dp"

# fish
dp completions fish | source

# PowerShell (add to your $PROFILE to persist)
dp completions powershell | Out-String | Invoke-Expression
```

### Man page

`dp man` prints a troff-formatted man page to stdout:

```bash
dp man | gzip > dp.1.gz
sudo mv dp.1.gz /usr/local/share/man/man1/
```

## Quick Start

Process a stream of JSON records, filter them, sort them, and output as CSV:

```bash
cat examples/users.jsonl | dp filter '.age >= 21' | dp sort age:desc | dp csv
```

## Available Commands

Run `dp <command> --help` for full details on any command below.

### Transformations
- `filter <expression>`: Keeps only records where the expression evaluates to `true`.
- `search <text> [--regex]`: Keeps records where *any* field's value contains the given text, without needing to know the field names up front. With `--regex`, `<text>` is a regex compiled once (not per-record) instead of a literal substring. Useful for wide or unknown-schema data — the alternative is hand-writing `contains(.a,"x") || contains(.b,"x") || ...` for every field.
- `select <fields> [--exclude]`: Keeps only the specified comma-separated fields. Missing fields are filled with `null`. With `--exclude`, `<fields>` is instead an exclusion list — keeps everything except the named fields (field order preserved), e.g. `dp select password,secret --exclude`.
- `limit <max>`: Halts the stream after yielding `N` records.
- `explode <field>`: Expands an array-valued field into one record per element. Records where the field isn't an array pass through unchanged.
- `map <field> <expression> [--set FIELD=EXPRESSION ...]`: Computes a new field (or overwrites an existing one) using an expression. Repeat `--set` to compute multiple fields in one pass, e.g. `dp map total '.price * .qty' --set tax='.total * 0.1'` — assignments are applied in order, so a later one (like `tax` here) can reference a field set by an earlier one (`total`).
- `join <file> --on <fields> [--type <left|inner|right|full>] [--merge]`: Joins each record with a matching record from `<file>` (JSONL or CSV) on the given field(s). `--on` accepts one or more comma-separated fields for a composite key, e.g. `--on region,id` — all of them must match for two records to be considered the same. Defaults to `left`. By default, `<file>` is loaded entirely into memory as a hash table before the main stream starts, so memory usage is proportional to its size — fine for typical lookup-table-sized files, not bounded for huge ones.
  - `left` (default): keeps every record from the main stream; merges in matching fields from `<file>` when found, otherwise passes the record through unchanged.
  - `inner`: keeps only records that have a match in `<file>`.
  - `right`: keeps only records that have a match, then appends any record from `<file>` that was never matched (with no fields from the main stream).
  - `full`: behaves like `left`, then also appends any unmatched record from `<file>` at the end (equivalent to left + the unmatched tail from right).
  - `--merge`: uses a memory-bounded sort-merge join instead of the hash join — both sides are sorted by the join key first (the same external merge sort `sort` uses, spilling to temp files), so memory stays bounded regardless of `<file>`'s size. For a duplicate join key, `--merge` produces the full cross product of matching records (the textbook sort-merge behavior) rather than the hash join's last-write-wins, and its output comes out in join-key-sorted order rather than the main stream's original order.

  ```bash
  dp join customers.jsonl --on customer_id --type inner
  dp join huge_lookup.jsonl --on id --merge
  ```
- `inspect`: Passes the stream through unchanged — useful for debugging where in a pipeline something goes wrong.
- `rename <old:new,...>`: Renames one or more fields, e.g. `dp rename user_name:name,ts:timestamp`. Fields not mentioned are left untouched; field order is preserved.
- `flatten [--sep <sep>]`: Flattens nested objects into dot-path keys, e.g. `{"user":{"name":"Alice"}}` becomes `{"user.name":"Alice"}`. The separator defaults to `.`. Array-valued fields are left as-is — use `explode` for those. Useful before `csv` output, since nested objects otherwise render as `[complex]`.

### Stateful Operations
*(Note: These operations must buffer the stream into memory, or spill to temp files for `sort`)*
- `sort <fields>`: Sorts the records by one or more comma-separated fields. Each field defaults to ascending; append `:desc` (or `:asc`) to override per field, e.g. `dp sort country,age:desc` sorts by `country` ascending, then by `age` descending within each `country`. Uses an external k-way merge sort (temp files), so it isn't bounded by RAM even for very large streams.
- `topn <fields> <n>`: Keeps only the top `n` records by the given `sort`-style field spec (same syntax, e.g. `score:desc`). Unlike `sort <fields> | limit <n>`, this never buffers more than `n` records — a bounded max-heap (O(n log k) time, O(k) memory where `k = min(n, stream length)`) instead of a full external sort, so it's the better choice whenever you only need a handful of extremes out of a huge stream, e.g. `dp topn score:desc 10`.
- `unique <fields>`: Keeps only the first occurrence of each distinct combination of one or more comma-separated fields. Memory usage is proportional to the number of *distinct* combinations seen, not the stream length.
- `dedup`: Drops exact duplicate records (comparing every field), keeping the first occurrence. Unlike `unique <fields>`, which dedupes on specific fields, `dedup` only drops a record if it matches an earlier one in *every* field.
- `group <by> [--sum <field>] [--count]`: Groups records by one or more comma-separated fields, optionally summing another numeric field and/or counting records per group. Memory usage is proportional to the number of *distinct* groups, not the stream length. To filter on the aggregate itself (a SQL-style `HAVING`), pipe into `filter` afterward, e.g. `dp group category --count | dp filter '.count > 10'` keeps only groups with more than 10 records.
- `freq <field> [--limit <n>]`: Counts occurrences of each distinct value in a field, sorted most-frequent first, with each value's percentage of the stream. Unlike `group --count` (unsorted, no percentage), this is built for quick data exploration — pipe into `dp table` for a readable view. Optionally keep only the top N values with `--limit`. Same memory tradeoff as `group`/`unique`.
- `sample <n>`: Takes a uniform random sample of `n` records from the stream, via reservoir sampling — a single streaming pass with O(n) memory, without needing to know the stream length in advance. If the stream has fewer than `n` records, all of them are returned.

### Aggregations
- `count`: Consumes the stream and yields the total record count.
- `sum <field>`: Computes the sum of a numeric field. Non-numeric/missing values are ignored.
- `avg <field>`: Computes the average of a numeric field. Yields `null` if the stream is empty.
- `min <field>` / `max <field>`: Finds the minimum/maximum value.

### Schema & Formatting
- `schema`: Inspects (up to the first 10,000 records of) the stream and infers the data types of all fields, e.g. `"integer | null"` if a field is sometimes explicitly `null`. A field that's simply absent from a record isn't counted for that record.
- `stats`: Computes `count`/`nulls`/`distinct`/`min`/`max`/`mean`/`stddev` for every field in a single pass, yielding one summary record per field (pipe into `dp table` for a readable view). `mean`/`stddev` are `null` for non-numeric fields. Memory usage for `distinct` is proportional to the number of *distinct* values per field, same tradeoff as `unique`/`group`.
- `csv`: Outputs the resulting stream as a CSV instead of JSONL. Array/object fields are rendered as `[complex]`.
- `table`: Outputs an aligned, human-readable table (header row, dashed separator, then data rows) instead of JSONL — like `column -t`. Unlike the other output commands, this buffers the entire stream first, since column widths depend on every value seen. Missing fields render blank; array/object fields render as `[complex]`.
- `--in-csv`: A global flag to read the input as CSV instead of JSONL. CSV values are inferred as integer, float, boolean, or string. Integers are only inferred when they round-trip exactly (e.g. `"25"` → `25`), so values like zip codes or phone numbers with a leading zero (`"00501"`) are correctly kept as strings rather than silently losing that leading zero. Floats are similarly guarded: a field that's literally the text `"NaN"`, `"inf"`, or `"Infinity"` is kept as a string rather than parsed into a non-finite float, since JSON has no representation for either and would silently render it as `null`.
- `--strict`: A global flag that aborts the whole pipeline on the first malformed record instead of the default behavior (skip it with a warning and continue). See [Error behavior](#error-behavior).
- `--pretty` / `-p`: A global flag that indents JSON output for human reading, instead of the default compact one-object-per-line format. Each pretty-printed object may span multiple lines, so this output is **not** valid JSONL — don't pipe it into another `dp` command. Ignored for `csv` output.

### Utility
- `completions <shell>`: Prints a shell completion script for `bash`, `zsh`, `fish`, `powershell`, or `elvish`. See [Shell completions](#shell-completions).
- `man`: Prints a troff-formatted man page. See [Man page](#man-page).
- `run <file>`: Runs a multi-stage pipeline defined in a TOML file, instead of chaining many `dp` invocations with shell pipes. See [Pipeline files](#pipeline-files).

## Pipeline files

For a pipeline with many stages, `dp run pipeline.toml` runs them all in a single process instead of a long shell one-liner:

```toml
# pipeline.toml
strict = false     # optional, defaults to false; combines with --strict (either being true is enough)
out_csv = false    # optional, defaults to false - output JSONL or CSV
out_table = false  # optional, defaults to false - output as an aligned table
in_csv = false     # optional, defaults to false - read input as JSONL or CSV
pretty = false     # optional, defaults to false; combines with --pretty (either being true is enough)

[[stages]]
type = "filter"
expression = ".age >= 21 && .active == true"

[[stages]]
type = "sort"
fields = "age:desc"

[[stages]]
type = "select"
fields = ["name", "age"]
```

```bash
cat users.jsonl | dp run pipeline.toml
```

Each `[[stages]]` table's `type` corresponds to a subcommand (`filter`, `search`, `select`, `sort`, `topn`, `unique`, `dedup`, `count`, `sum`, `avg`, `min`, `max`, `schema`, `stats`, `group`, `freq`, `explode`, `rename`, `flatten`, `sample`, `map`, `join`) with the same field names as that subcommand's flags/arguments — e.g. `sort`/`topn`/`unique` take a comma-separated `fields` string (`sort`/`topn` fields may carry a `:desc`/`:asc` suffix, e.g. `"age:desc"`; `topn` also takes `n`), `group`/`join` take a comma-separated `by`/`on` string, and `join` also takes `file` and an optional `join_type` (`"left"` | `"inner"` | `"right"` | `"full"`, defaults to `"left"`). Format/utility commands (`csv`, `table`, `completions`, `man`, `run` itself) aren't valid `[[stages]]` entries — use the top-level `out_csv`/`out_table` settings for those output formats instead.

## Expressions

`filter` and `map` share the same expression language.

**Precedence** (lowest to highest binding): `||` → `&&` → comparison (`== != > < >= <=`) → `+ -` → `* /` → unary `!`.
All binary operators are left-associative. Parentheses `( )` can be used to override precedence. Unary minus (`-5`, `-.field`) is supported and binds tighter than binary `+`/`-`/`*`/`/`. If the expression *itself* starts with `-` (e.g. `-.age`), put `--` before it so the shell/CLI parser doesn't mistake it for a flag: `dp map delta -- '-.age'`. A `-` that isn't the first character (e.g. `.a > -5`) needs no such workaround.

- **Field access:** `.fieldname` — evaluates to `null` if the field is missing. Nested fields are supported via dotted paths, e.g. `.user.age`, which evaluates to `null` if any segment is missing or isn't an object.
- **Literals:** strings (`"value"`), integers (`42`), floats (`3.5`), booleans (`true`/`false`).
- **Comparison:** `==`, `!=`, `<`, `>`, `<=`, `>=`
- **Logical:** `&&`, `||` (both short-circuit), `!` (unary not)
- **Arithmetic:** `+`, `-`, `*`, `/` on integers and floats (mixed int/float promotes to float). Division by zero evaluates to `null` rather than erroring. Arithmetic on incompatible types (e.g. `"a" + 1`) evaluates to `null`.
- **String functions:** `contains(a, b)`, `starts_with(a, b)`, `ends_with(a, b)` (all return a boolean), and `lower(a)` / `upper(a)` (return a string). All operate on string values; a non-string operand evaluates to `null`. Function calls can be used anywhere an expression is expected, including as arguments to other functions or combined with `&&`/`||`/`!`.
- **`concat(a, b, ...)`:** joins two or more values into a string. Unlike the functions above, `concat` stringifies any scalar type (numbers, booleans), not just strings, and treats `null` as an empty string rather than making the whole result `null` — the point is building display text (e.g. `full_name`), where a missing optional field shouldn't blow up the rest of the string. Array/object arguments render as `[complex]`. Note `+` is arithmetic-only; use `concat` for string building.
- **Numeric functions:** `round(a)`, `floor(a)`, `ceil(a)` all return an integer (a no-op if `a` is already an integer); `abs(a)` preserves the input's numeric type (integer stays integer, float stays float). `least(a, b)` / `greatest(a, b)` work on any value type via the same total ordering `sort` uses, not just numbers, e.g. `least("banana", "apple") == "apple"`. All evaluate to `null` on a non-numeric operand (`least`/`greatest` excepted, since they accept any type).
- **Date/time functions:** `to_unix(a)` parses an RFC3339 datetime string (e.g. `"2024-01-15T10:30:00Z"`) or a bare `"YYYY-MM-DD"` date into a Unix timestamp (seconds since epoch), enabling date-range filtering via ordinary integer comparison. `year(a)`, `month(a)`, `day(a)` extract calendar components the same way. `now()` (no arguments) returns the current Unix timestamp. All evaluate to `null` on an unparseable or non-string input rather than erroring.
- **Membership:** `value in (a, b, c)` — evaluates to `true` if `value` equals any element of the list (compared the same way as `==`). Works with any value type, not just strings. An empty list (`in ()`) is always `false`.
- **Regex:** `matches(a, "pattern")` — returns a boolean; `a` must evaluate to a string (non-string evaluates to `null`). The pattern **must be a string literal**, not a computed expression — it's compiled once when the expression is parsed, not on every record, so `filter`/`map` stay fast on large streams. String literals in this language don't process backslash escapes, so write the pattern exactly as you would in a regex (a single `\` before a special character, e.g. `"^.+@example\.com$"` — not `\\.`).

Examples:
```bash
dp filter '.age >= 21 && .active == true'
dp filter '.status != "banned" || .admin == true'
dp map total '.price * .quantity'
dp filter '.address.city == "London"'
dp filter '!(.status == "banned") && (.age >= 18 || .verified == true)'
dp filter 'contains(.name, "Smith")'
dp filter 'lower(.email) == "alice@example.com"'
dp filter 'starts_with(.sku, "SKU-") && !ends_with(.sku, "-DISCONTINUED")'
dp filter '.status in ("active", "pending")'
dp filter 'matches(.email, "^.+@example\.com$")'
dp map full_name 'concat(.first, " ", .last)'
dp filter 'greatest(.score_a, .score_b) >= 90'
dp filter 'year(.created_at) == 2024'
```

## Error behavior

- By default, a malformed record (invalid JSON/CSV) is **skipped with a warning printed to stderr**, and processing continues with the rest of the stream. This applies uniformly, including to aggregations like `count`/`sum`/`group` — a bad line is excluded from the result rather than corrupting it.
- Pass the global `--strict` flag to abort the entire pipeline immediately on the first malformed record instead, with a diagnostic error and exit code `1`.
- An invalid `filter`/`map` expression fails immediately with a syntax error and exit code `1` (expression syntax errors are always fatal, regardless of `--strict`).
- A `filter`/`map` expression referencing a field that doesn't exist on a given record treats that field as `null` rather than erroring.
- `sort`/`min`/`max`/etc. on a field that's missing on some records treats those as `null`, which sorts before all other types.
- A missing `join` file fails with exit code `1`. A malformed record *within* the join file follows the same `--strict`/default behavior as the main stream (skipped with a warning by default, or aborts with `--strict`).
- Success exits `0`, including when the output stream is empty (e.g. `count` on empty input yields `0`, `avg` on empty input yields `null`) or when records were skipped under the default (non-strict) mode.

## Known limitations

- **Integer precision beyond `i64`:** JSON integers larger than `i64::MAX` (~9.2 × 10¹⁸, about 19 digits) lose precision — they're silently represented as a 64-bit float instead of the exact integer. This can affect very large numeric IDs (some 64-bit unsigned or 128-bit identifiers). Regular integers, and floats in general, are unaffected.
- **Hash-key collisions in `group`/`join`/`unique`:** these stages key non-string values by serializing them to a JSON string internally. Two different-typed values that happen to serialize identically could theoretically collide — an edge case that hasn't come up in practice but is worth knowing about if you're grouping/joining/deduplicating on a field with mixed or unusual types. When multiple fields form a composite key (`unique a,b`, `group a,b`, `join --on a,b`), each field's encoded value is joined with a control character (`\u{1}`) that's vanishingly unlikely to appear in real field content, so a key can't collide across a field-count boundary the way naive string concatenation could. `sort` isn't affected by this - it compares values directly rather than via a serialized key.
- **No input size guard:** there's currently no limit on a single record's size before it's parsed. An extremely long single line (or field) will be read into memory in full before any pipeline stage runs. Worth keeping in mind if you're processing data from an untrusted source.
- **Broken-pipe output prints an error:** piping `dp`'s output into a command that closes the pipe early (e.g. `dp inspect largefile.jsonl | head`) prints `Error: Broken pipe (os error 32)` to stderr and exits non-zero, rather than exiting silently the way most well-behaved Unix tools do. It doesn't panic or corrupt output, just surfaces a benign, expected condition as an error message.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for how to set up a dev environment and run the test suite.
