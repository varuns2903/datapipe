# Transformations

These stages process the stream lazily, one record at a time, with O(1) memory.

- `filter <expression>`: Keeps only records where the expression evaluates to `true`. See [Expressions](../expressions.md).
- `select <fields>`: Keeps only the specified comma-separated fields. Missing fields are filled with `null`.
- `limit <max>`: Halts the stream after yielding `N` records.
- `explode <field>`: Expands an array-valued field into one record per element. Records where the field isn't an array pass through unchanged.
- `map <field> <expression>`: Computes a new field (or overwrites an existing one) using an expression.
- `inspect`: Passes the stream through unchanged — useful for debugging where in a pipeline something goes wrong.
- `rename <old:new,...>`: Renames one or more fields, e.g. `dp rename user_name:name,ts:timestamp`. Fields not mentioned are left untouched; field order is preserved.
- `flatten [--sep <sep>]`: Flattens nested objects into dot-path keys, e.g. `{"user":{"name":"Alice"}}` becomes `{"user.name":"Alice"}`. The separator defaults to `.`. Array-valued fields are left as-is — use `explode` for those. Useful before `csv` output, since nested objects otherwise render as `[complex]`.

## join

```
join <file> --on <field> [--type <left|inner|right|full>]
```

Joins each record with a matching record from `<file>` (JSONL or CSV) on the given field. Defaults to `left`. `<file>` is loaded entirely into memory as a hash table before the main stream starts, so memory usage is proportional to its size — fine for typical lookup-table-sized files, not bounded for huge ones.

- **`left`** (default): keeps every record from the main stream; merges in matching fields from `<file>` when found, otherwise passes the record through unchanged.
- **`inner`**: keeps only records that have a match in `<file>`.
- **`right`**: keeps only records that have a match, then appends any record from `<file>` that was never matched (with no fields from the main stream).
- **`full`**: behaves like `left`, then also appends any unmatched record from `<file>` at the end (equivalent to left + the unmatched tail from right).

```bash
dp join customers.jsonl --on customer_id --type inner
```
