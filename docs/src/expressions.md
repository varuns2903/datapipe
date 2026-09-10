# Expressions

`filter` and `map` share the same expression language.

**Precedence** (lowest to highest binding): `||` → `&&` → comparison (`== != > < >= <=`) → `+ -` → `* /` → unary `!`.
All binary operators are left-associative. Parentheses `( )` can be used to override precedence. There is currently no support for unary minus (`-5`) — only subtraction between two operands.

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

## Examples

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
