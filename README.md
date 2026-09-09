# DataPipe (`dp`)
[![Crates.io](https://img.shields.io/crates/v/datapipe-cli.svg)](https://crates.io/crates/datapipe-cli)


DataPipe is a streaming-first, Unix-inspired CLI for processing structured data (JSON, CSV). 
Instead of operating on raw text strings, `dp` operates on structured records natively, allowing you to filter, sort, aggregate, and transform gigabytes of data with a small memory footprint.

## Features
- **Streaming by Default:** Transformation stages (`filter`, `select`, `limit`, `explode`, `map`) and simple single-value aggregations (`count`, `sum`, `avg`, `min`, `max`) process data lazily with O(1) memory, independent of input size. `sort` is memory-bounded too, via an external merge sort that spills to temp files rather than buffering the whole stream. **`unique`, `group`, and `schema` are the exception**: they hold state proportional to the number of *distinct* keys (or, for `schema`, up to the first 10,000 records) rather than the stream length — fine for typical cardinality, but not O(1) if you `unique`/`group` a field with an enormous number of distinct values (e.g. a UUID column over billions of rows).
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
cat examples/users.jsonl | dp filter '.age >= 21' | dp sort age --desc | dp csv
```

## Available Commands

Run `dp <command> --help` for full details on any command below.

### Transformations
- `filter <expression>`: Keeps only records where the expression evaluates to `true`.
- `select <fields>`: Keeps only the specified comma-separated fields. Missing fields are filled with `null`.
- `limit <max>`: Halts the stream after yielding `N` records.
- `explode <field>`: Expands an array-valued field into one record per element. Records where the field isn't an array pass through unchanged.
- `map <field> <expression>`: Computes a new field (or overwrites an existing one) using an expression.
- `join <file> --on <field> [--type <left|inner|right|full>]`: Joins each record with a matching record from `<file>` (JSONL or CSV) on the given field. Defaults to `left`.
  - `left` (default): keeps every record from the main stream; merges in matching fields from `<file>` when found, otherwise passes the record through unchanged.
  - `inner`: keeps only records that have a match in `<file>`.
  - `right`: keeps only records that have a match, then appends any record from `<file>` that was never matched (with no fields from the main stream).
  - `full`: behaves like `left`, then also appends any unmatched record from `<file>` at the end (equivalent to left + the unmatched tail from right).

  ```bash
  dp join customers.jsonl --on customer_id --type inner
  ```
- `inspect`: Passes the stream through unchanged — useful for debugging where in a pipeline something goes wrong.
- `rename <old:new,...>`: Renames one or more fields, e.g. `dp rename user_name:name,ts:timestamp`. Fields not mentioned are left untouched; field order is preserved.
- `flatten [--sep <sep>]`: Flattens nested objects into dot-path keys, e.g. `{"user":{"name":"Alice"}}` becomes `{"user.name":"Alice"}`. The separator defaults to `.`. Array-valued fields are left as-is — use `explode` for those. Useful before `csv` output, since nested objects otherwise render as `[complex]`.

### Stateful Operations
*(Note: These operations must buffer the stream into memory, or spill to temp files for `sort`)*
- `sort <field> [--desc]`: Sorts the records by the specified field. Uses an external k-way merge sort (temp files), so it isn't bounded by RAM even for very large streams.
- `unique <field>`: Keeps only the first occurrence of each unique value in a field. Memory usage is proportional to the number of *distinct* values seen, not the stream length.
- `group <by> [--sum <field>] [--count]`: Groups records by a field, optionally summing another numeric field and/or counting records per group. Memory usage is proportional to the number of *distinct* groups, not the stream length.
- `sample <n>`: Takes a uniform random sample of `n` records from the stream, via reservoir sampling — a single streaming pass with O(n) memory, without needing to know the stream length in advance. If the stream has fewer than `n` records, all of them are returned.

### Aggregations
- `count`: Consumes the stream and yields the total record count.
- `sum <field>`: Computes the sum of a numeric field. Non-numeric/missing values are ignored.
- `avg <field>`: Computes the average of a numeric field. Yields `null` if the stream is empty.
- `min <field>` / `max <field>`: Finds the minimum/maximum value.

### Schema & Formatting
- `schema`: Inspects (up to the first 10,000 records of) the stream and infers the data types of all fields, e.g. `"integer | null"` if a field is sometimes explicitly `null`. A field that's simply absent from a record isn't counted for that record.
- `csv`: Outputs the resulting stream as a CSV instead of JSONL. Array/object fields are rendered as `[complex]`.
- `--in-csv`: A global flag to read the input as CSV instead of JSONL. CSV values are inferred as integer, float, boolean, or string. Integers are only inferred when they round-trip exactly (e.g. `"25"` → `25`), so values like zip codes or phone numbers with a leading zero (`"00501"`) are correctly kept as strings rather than silently losing that leading zero.
- `--strict`: A global flag that aborts the whole pipeline on the first malformed record instead of the default behavior (skip it with a warning and continue). See [Error behavior](#error-behavior).

### Utility
- `completions <shell>`: Prints a shell completion script for `bash`, `zsh`, `fish`, `powershell`, or `elvish`. See [Shell completions](#shell-completions).
- `man`: Prints a troff-formatted man page. See [Man page](#man-page).

## Expressions

`filter` and `map` share the same expression language.

**Precedence** (lowest to highest binding): `||` → `&&` → comparison (`== != > < >= <=`) → `+ -` → `* /` → unary `!`.
All binary operators are left-associative. Parentheses `( )` can be used to override precedence. There is currently no support for unary minus (`-5`) — only subtraction between two operands.

- **Field access:** `.fieldname` — evaluates to `null` if the field is missing. Nested fields are supported via dotted paths, e.g. `.user.age`, which evaluates to `null` if any segment is missing or isn't an object.
- **Literals:** strings (`"value"`), integers (`42`), floats (`3.5`), booleans (`true`/`false`).
- **Comparison:** `==`, `!=`, `<`, `>`, `<=`, `>=`
- **Logical:** `&&`, `||` (both short-circuit), `!` (unary not)
- **Arithmetic:** `+`, `-`, `*`, `/` on integers and floats (mixed int/float promotes to float). Division by zero evaluates to `null` rather than erroring. Arithmetic on incompatible types (e.g. `"a" + 1`) evaluates to `null`.
- **String functions:** `contains(a, b)`, `starts_with(a, b)`, `ends_with(a, b)` (all return a boolean), and `lower(a)` / `upper(a)` (return a string). All operate on string values; a non-string operand evaluates to `null`. Function calls can be used anywhere an expression is expected, including as arguments to other functions or combined with `&&`/`||`/`!`.
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
- **Hash-key collisions in `group`/`join`/`unique`:** these stages key non-string values by serializing them to a JSON string internally. Two different-typed values that happen to serialize identically could theoretically collide — an edge case that hasn't come up in practice but is worth knowing about if you're grouping/joining/deduplicating on a field with mixed or unusual types.
- **No input size guard:** there's currently no limit on a single record's size before it's parsed. An extremely long single line (or field) will be read into memory in full before any pipeline stage runs. Worth keeping in mind if you're processing data from an untrusted source.
- **Broken-pipe output prints an error:** piping `dp`'s output into a command that closes the pipe early (e.g. `dp inspect largefile.jsonl | head`) prints `Error: Broken pipe (os error 32)` to stderr and exits non-zero, rather than exiting silently the way most well-behaved Unix tools do. It doesn't panic or corrupt output, just surfaces a benign, expected condition as an error message.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for how to set up a dev environment and run the test suite.
