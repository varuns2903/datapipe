# Stateful Operations

These operations must buffer the stream into memory, or spill to temp files for `sort`.

- `sort <field> [--desc]`: Sorts the records by the specified field. Uses an external k-way merge sort (temp files), so it isn't bounded by RAM even for very large streams.
- `unique <field>`: Keeps only the first occurrence of each unique value in a field. Memory usage is proportional to the number of *distinct* values seen, not the stream length.
- `group <by> [--sum <field>] [--count]`: Groups records by a field, optionally summing another numeric field and/or counting records per group. Memory usage is proportional to the number of *distinct* groups, not the stream length.
- `sample <n>`: Takes a uniform random sample of `n` records from the stream, via reservoir sampling — a single streaming pass with O(n) memory, without needing to know the stream length in advance. If the stream has fewer than `n` records, all of them are returned.
