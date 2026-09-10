# Pipeline Files

For a pipeline with many stages, `dp run pipeline.toml` runs them all in a single process instead of a long shell one-liner:

```toml
# pipeline.toml
strict = false     # optional, defaults to false; combines with --strict (either being true is enough)
out_csv = false     # optional, defaults to false - output JSONL or CSV
out_table = false   # optional, defaults to false - output as an aligned table
in_csv = false      # optional, defaults to false - read input as JSONL or CSV
pretty = false      # optional, defaults to false; combines with --pretty (either being true is enough)

[[stages]]
type = "filter"
expression = ".age >= 21 && .active == true"

[[stages]]
type = "sort"
field = "age"
desc = true

[[stages]]
type = "select"
fields = ["name", "age"]
```

```bash
cat users.jsonl | dp run pipeline.toml
```

Each `[[stages]]` table's `type` corresponds to a subcommand (`filter`, `select`, `sort`, `unique`, `dedup`, `count`, `sum`, `avg`, `min`, `max`, `schema`, `stats`, `group`, `freq`, `explode`, `rename`, `flatten`, `sample`, `map`, `join`) with the same field names as that subcommand's flags/arguments — e.g. `join` takes `file`, `on`, and an optional `join_type` (`"left"` | `"inner"` | `"right"` | `"full"`, defaults to `"left"`).

Format/utility commands (`csv`, `table`, `completions`, `man`, `run` itself) aren't valid `[[stages]]` entries — use the top-level `out_csv`/`out_table` settings for those output formats instead.
