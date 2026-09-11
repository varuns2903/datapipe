# Transformations

These stages process the stream lazily, one record at a time, with O(1) memory.

- `filter <expression>`: Keeps only records where the expression evaluates to `true`. See [Expressions](../expressions.md).
- `search <text> [--regex]`: Keeps records where *any* field's value contains the given text, without needing to know the field names up front. With `--regex`, `<text>` is a regex compiled once (not per-record) instead of a literal substring. Useful for wide or unknown-schema data — the alternative is hand-writing `contains(.a,"x") || contains(.b,"x") || ...` for every field.
- `select <fields> [--exclude]`: Keeps only the specified comma-separated fields. Missing fields are filled with `null`. With `--exclude`, `<fields>` is instead an exclusion list — keeps everything except the named fields (field order preserved), e.g. `dp select password,secret --exclude`.
- `limit <max>`: Halts the stream after yielding `N` records.
- `explode <field>`: Expands an array-valued field into one record per element. Records where the field isn't an array pass through unchanged.
- `map <field> <expression>`: Computes a new field (or overwrites an existing one) using an expression.
- `inspect`: Passes the stream through unchanged — useful for debugging where in a pipeline something goes wrong.
- `rename <old:new,...>`: Renames one or more fields, e.g. `dp rename user_name:name,ts:timestamp`. Fields not mentioned are left untouched; field order is preserved.
- `flatten [--sep <sep>]`: Flattens nested objects into dot-path keys, e.g. `{"user":{"name":"Alice"}}` becomes `{"user.name":"Alice"}`. The separator defaults to `.`. Array-valued fields are left as-is — use `explode` for those. Useful before `csv` output, since nested objects otherwise render as `[complex]`.

## join

```
join <file> --on <field> [--type <left|inner|right|full>] [--merge]
```

Joins each record with a matching record from `<file>` (JSONL or CSV) on the given field. Defaults to `left`.

- **`left`** (default): keeps every record from the main stream; merges in matching fields from `<file>` when found, otherwise passes the record through unchanged.
- **`inner`**: keeps only records that have a match in `<file>`.
- **`right`**: keeps only records that have a match, then appends any record from `<file>` that was never matched (with no fields from the main stream).
- **`full`**: behaves like `left`, then also appends any unmatched record from `<file>` at the end (equivalent to left + the unmatched tail from right).

```bash
dp join customers.jsonl --on customer_id --type inner
```

### Memory: hash join (default) vs `--merge`

By default, `join` loads `<file>` entirely into memory as a hash table before the main stream starts — fine for typical lookup-table-sized files, not bounded for huge ones. Pass **`--merge`** to use a memory-bounded sort-merge join instead: both sides are sorted by the join key first (the same external merge sort `sort` uses, spilling to temp files rather than buffering fully), so memory is bounded regardless of how large `<file>` is.

`--merge` has one deliberate behavioral difference worth knowing: for a **duplicate join key**, the default hash join keeps only the *last* matching record from `<file>` (a hash-table insert overwrites earlier ones), while `--merge` produces the full cross product — every combination of matching left/right records, which is the textbook-correct sort-merge join behavior. Also, `--merge`'s output comes out in join-key-sorted order rather than the main stream's original order, since sorting is inherent to the algorithm.

```bash
dp join huge_lookup.jsonl --on id --merge
```
