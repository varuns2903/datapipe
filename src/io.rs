use crate::model::{Record, Value};
use anyhow::Result;
use std::io::{BufRead, Write};

// JSON In
//
// Parses one JSON object per line (true JSONL semantics) rather than treating
// the input as a single concatenated JSON value stream. This means a
// malformed line only affects that line - unlike `serde_json::Deserializer`'s
// streaming parser, which stops yielding entirely after the first parse
// error, silently dropping every valid record that follows it.
pub fn read_json_stream<'a, R: BufRead + 'a>(
    reader: R,
) -> impl Iterator<Item = Result<Record>> + 'a {
    reader.lines().enumerate().filter_map(|(i, line_res)| {
        let line_no = i + 1;
        let line = match line_res {
            Ok(l) => l,
            Err(e) => return Some(Err(anyhow::anyhow!("IO error reading line {line_no}: {e}"))),
        };
        if line.trim().is_empty() {
            return None;
        }
        Some(
            serde_json::from_str::<Record>(&line)
                .map_err(|e| anyhow::anyhow!("JSON parse error on line {line_no}: {e}")),
        )
    })
}

// JSON Out
pub fn write_json_stream<W: Write>(
    mut writer: W,
    records: impl Iterator<Item = Result<Record>>,
    pretty: bool,
) -> Result<()> {
    for record in records {
        let rec = record?;
        if pretty {
            serde_json::to_writer_pretty(&mut writer, &rec)?;
        } else {
            serde_json::to_writer(&mut writer, &rec)?;
        }
        writer.write_all(b"\n")?;
    }
    Ok(())
}

// CSV In
pub fn read_csv_stream<'a, R: BufRead + 'a>(
    reader: R,
) -> Result<impl Iterator<Item = Result<Record>> + 'a> {
    let mut csv_reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(reader);

    let headers = csv_reader.headers()?.clone();

    // We want to return an iterator that produces Record (IndexMap<String, Value>)
    let iter = csv_reader.into_records().map(move |res| {
        let string_record = res.map_err(|e| anyhow::anyhow!("CSV parse error: {}", e))?;
        let mut record = indexmap::IndexMap::new();

        for (i, field) in string_record.iter().enumerate() {
            let header_name = headers.get(i).unwrap_or("unknown").to_string();

            // Try to infer numbers, otherwise treat as string.
            //
            // Integers are only inferred when the parsed value round-trips
            // back to the exact original text. Without this check, values
            // like zip codes ("00501") or phone numbers with a leading zero
            // would silently lose that leading zero by being reparsed as the
            // integer 501 - a real footgun for real-world CSV data. This
            // check doesn't apply to floats, since fixed-decimal formatting
            // like "19.99" or "5.00" is common and should still be numeric.
            let value = if let Ok(n) = field.parse::<i64>() {
                if n.to_string() == field {
                    Value::Integer(n)
                } else {
                    Value::String(field.to_string())
                }
            } else if let Ok(f) = field.parse::<f64>() {
                Value::Float(f)
            } else if field.eq_ignore_ascii_case("true") {
                Value::Boolean(true)
            } else if field.eq_ignore_ascii_case("false") {
                Value::Boolean(false)
            } else if field.is_empty() {
                Value::Null
            } else {
                Value::String(field.to_string())
            };

            record.insert(header_name, value);
        }

        Ok(record)
    });

    Ok(iter)
}

// CSV Out
pub fn write_csv_stream<W: Write>(
    writer: W,
    mut records: impl Iterator<Item = Result<Record>>,
) -> Result<()> {
    let mut csv_writer = csv::Writer::from_writer(writer);

    // We need to fetch the first record to write the headers.
    // If the stream is empty, we do nothing.
    let mut first_record = None;
    if let Some(res) = records.next() {
        let rec = res?;
        let headers: Vec<String> = rec.keys().cloned().collect();
        csv_writer.write_record(&headers)?;
        first_record = Some((rec, headers));
    }

    let (first_rec, headers) = match first_record {
        Some(x) => x,
        None => return Ok(()),
    };

    // Write first record
    write_csv_row(&mut csv_writer, &first_rec, &headers)?;

    // Write remaining records
    for record in records {
        let rec = record?;
        write_csv_row(&mut csv_writer, &rec, &headers)?;
    }

    csv_writer.flush()?;
    Ok(())
}

fn write_csv_row<W: Write>(
    csv_writer: &mut csv::Writer<W>,
    record: &Record,
    headers: &[String],
) -> Result<()> {
    let mut row = Vec::new();
    for header in headers {
        row.push(value_to_display_string(
            record.get(header).unwrap_or(&Value::Null),
        ));
    }
    csv_writer.write_record(&row)?;
    Ok(())
}

/// Renders a `Value` as plain text for tabular display (CSV cells, table
/// columns) - not JSON, just a human-readable flat representation.
fn value_to_display_string(v: &Value) -> String {
    match v {
        Value::Null => "".to_string(),
        Value::Boolean(b) => b.to_string(),
        Value::Integer(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::String(s) => s.clone(),
        Value::Array(_) | Value::Object(_) => "[complex]".to_string(),
    }
}

// Table Out
//
// Renders an aligned, human-readable table (header + dashed separator +
// rows), similar to `column -t` or `mlr --opprint`. Unlike the streaming
// writers above, this must buffer the entire stream first: column widths
// depend on every value in that column, which can't be known until the
// whole stream has been seen.
pub fn write_table_stream<W: Write>(
    mut writer: W,
    records: impl Iterator<Item = Result<Record>>,
) -> Result<()> {
    let mut rows: Vec<Record> = Vec::new();
    let mut headers: Vec<String> = Vec::new();
    let mut seen_headers: std::collections::HashSet<String> = std::collections::HashSet::new();

    for res in records {
        let rec = res?;
        for key in rec.keys() {
            if seen_headers.insert(key.clone()) {
                headers.push(key.clone());
            }
        }
        rows.push(rec);
    }

    if headers.is_empty() {
        return Ok(());
    }

    let cells: Vec<Vec<String>> = rows
        .iter()
        .map(|rec| {
            headers
                .iter()
                .map(|h| value_to_display_string(rec.get(h).unwrap_or(&Value::Null)))
                .collect()
        })
        .collect();

    let widths: Vec<usize> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            cells
                .iter()
                .map(|row| row[i].chars().count())
                .max()
                .unwrap_or(0)
                .max(h.chars().count())
        })
        .collect();

    write_table_row(&mut writer, &headers, &widths)?;
    let dashes: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    write_table_row(&mut writer, &dashes, &widths)?;
    for row in &cells {
        write_table_row(&mut writer, row, &widths)?;
    }

    Ok(())
}

fn write_table_row<W: Write>(writer: &mut W, cells: &[String], widths: &[usize]) -> Result<()> {
    let padded: Vec<String> = cells
        .iter()
        .zip(widths)
        .map(|(cell, width)| format!("{:<width$}", cell, width = width))
        .collect();
    writeln!(writer, "{}", padded.join("  ").trim_end())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn read_all(input: &str) -> Vec<Result<Record>> {
        read_json_stream(Cursor::new(input.as_bytes())).collect()
    }

    #[test]
    fn read_json_stream_recovers_after_a_malformed_line() {
        let results = read_all("{\"a\":1}\nnot json\n{\"a\":2}\n{\"a\":3}\n");
        assert_eq!(results.len(), 4);
        assert!(results[0].is_ok());
        assert!(results[1].is_err());
        // Records after the bad line must still be parsed - this is the
        // behavior the previous serde_json::Deserializer-based streaming
        // implementation did NOT provide (it stopped yielding entirely
        // after the first error).
        assert!(results[2].is_ok());
        assert!(results[3].is_ok());
    }

    #[test]
    fn read_json_stream_skips_blank_lines() {
        let results = read_all("{\"a\":1}\n\n{\"a\":2}\n");
        assert_eq!(results.len(), 2);
        assert!(results[0].is_ok());
        assert!(results[1].is_ok());
    }

    #[test]
    fn read_json_stream_error_includes_line_number() {
        let results = read_all("{\"a\":1}\nnot json\n");
        let err = results[1].as_ref().unwrap_err();
        assert!(err.to_string().contains("line 2"));
    }

    fn read_csv_all(input: &str) -> Vec<Record> {
        read_csv_stream(Cursor::new(input.as_bytes()))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    #[test]
    fn csv_preserves_leading_zeros_as_string() {
        let records = read_csv_all("zip\n00501\n");
        assert_eq!(
            records[0].get("zip"),
            Some(&Value::String("00501".to_string()))
        );
    }

    #[test]
    fn csv_infers_plain_integer_without_leading_zero() {
        let records = read_csv_all("age\n25\n");
        assert_eq!(records[0].get("age"), Some(&Value::Integer(25)));
    }

    #[test]
    fn csv_infers_zero_as_integer_not_string() {
        let records = read_csv_all("n\n0\n");
        assert_eq!(records[0].get("n"), Some(&Value::Integer(0)));
    }

    #[test]
    fn csv_infers_negative_integer() {
        let records = read_csv_all("n\n-5\n");
        assert_eq!(records[0].get("n"), Some(&Value::Integer(-5)));
    }

    #[test]
    fn csv_preserves_double_leading_zero_as_string() {
        let records = read_csv_all("n\n00\n");
        assert_eq!(records[0].get("n"), Some(&Value::String("00".to_string())));
    }

    #[test]
    fn csv_still_infers_fixed_decimal_floats() {
        // Trailing-zero decimal formatting (e.g. prices) is common and should
        // still be treated as numeric, unlike the leading-zero integer case.
        let records = read_csv_all("price\n5.00\n");
        assert_eq!(records[0].get("price"), Some(&Value::Float(5.0)));
    }

    #[test]
    fn csv_infers_booleans_and_nulls() {
        let records = read_csv_all("active,note\ntrue,\n");
        assert_eq!(records[0].get("active"), Some(&Value::Boolean(true)));
        assert_eq!(records[0].get("note"), Some(&Value::Null));
    }

    #[test]
    fn write_json_stream_compact_by_default() {
        let mut rec = Record::new();
        rec.insert("a".to_string(), Value::Integer(1));
        let mut out = Vec::new();
        write_json_stream(&mut out, std::iter::once(Ok(rec)), false).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "{\"a\":1}\n");
    }

    #[test]
    fn write_json_stream_pretty_indents() {
        let mut rec = Record::new();
        rec.insert("a".to_string(), Value::Integer(1));
        let mut out = Vec::new();
        write_json_stream(&mut out, std::iter::once(Ok(rec)), true).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "{\n  \"a\": 1\n}\n");
    }

    fn rec(pairs: &[(&str, Value)]) -> Record {
        let mut r = Record::new();
        for (k, v) in pairs {
            r.insert(k.to_string(), v.clone());
        }
        r
    }

    fn table_output(records: Vec<Record>) -> String {
        let mut out = Vec::new();
        write_table_stream(&mut out, records.into_iter().map(Ok)).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn table_aligns_columns_by_max_width() {
        let out = table_output(vec![
            rec(&[
                ("name", Value::String("Alice".to_string())),
                ("age", Value::Integer(30)),
            ]),
            rec(&[
                ("name", Value::String("Bo".to_string())),
                ("age", Value::Integer(9)),
            ]),
        ]);
        assert_eq!(
            out,
            "name   age\n\
             -----  ---\n\
             Alice  30\n\
             Bo     9\n"
        );
    }

    #[test]
    fn table_unions_headers_across_records_missing_fields_blank() {
        let out = table_output(vec![
            rec(&[("a", Value::Integer(1)), ("b", Value::Integer(2))]),
            rec(&[("a", Value::Integer(10))]),
        ]);
        // Header union preserves first-seen order: a, b.
        assert!(out.starts_with("a   b\n"));
        assert!(out.contains("10"));
    }

    #[test]
    fn table_on_empty_stream_produces_no_output() {
        let out = table_output(vec![]);
        assert_eq!(out, "");
    }

    #[test]
    fn table_renders_complex_values_as_placeholder() {
        let out = table_output(vec![rec(&[(
            "tags",
            Value::Array(vec![Value::Integer(1)]),
        )])]);
        assert!(out.contains("[complex]"));
    }

    #[test]
    fn value_to_display_string_matches_csv_cell_rendering() {
        assert_eq!(value_to_display_string(&Value::Null), "");
        assert_eq!(value_to_display_string(&Value::Boolean(true)), "true");
        assert_eq!(value_to_display_string(&Value::Integer(5)), "5");
        assert_eq!(
            value_to_display_string(&Value::String("x".to_string())),
            "x"
        );
    }
}
