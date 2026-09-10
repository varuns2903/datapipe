# Error Behavior

- By default, a malformed record (invalid JSON/CSV) is **skipped with a warning printed to stderr**, and processing continues with the rest of the stream. This applies uniformly, including to aggregations like `count`/`sum`/`group` — a bad line is excluded from the result rather than corrupting it.
- Pass the global `--strict` flag to abort the entire pipeline immediately on the first malformed record instead, with a diagnostic error and exit code `1`.
- An invalid `filter`/`map` expression fails immediately with a syntax error and exit code `1` (expression syntax errors are always fatal, regardless of `--strict`).
- A `filter`/`map` expression referencing a field that doesn't exist on a given record treats that field as `null` rather than erroring.
- `sort`/`min`/`max`/etc. on a field that's missing on some records treats those as `null`, which sorts before all other types.
- A missing `join` file fails with exit code `1`. A malformed record *within* the join file follows the same `--strict`/default behavior as the main stream (skipped with a warning by default, or aborts with `--strict`).
- Success exits `0`, including when the output stream is empty (e.g. `count` on empty input yields `0`, `avg` on empty input yields `null`) or when records were skipped under the default (non-strict) mode.
