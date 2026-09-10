# Aggregations

These stages consume the entire stream and yield a single summary record.

- `count`: Consumes the stream and yields the total record count.
- `sum <field>`: Computes the sum of a numeric field. Non-numeric/missing values are ignored.
- `avg <field>`: Computes the average of a numeric field. Yields `null` if the stream is empty.
- `min <field>` / `max <field>`: Finds the minimum/maximum value.
