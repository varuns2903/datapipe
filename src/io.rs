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
) -> Result<()> {
    for record in records {
        let rec = record?;
        serde_json::to_writer(&mut writer, &rec)?;
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

            // Try to infer numbers, otherwise treat as string
            let value = if let Ok(n) = field.parse::<i64>() {
                Value::Integer(n)
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
        let val_str = match record.get(header).unwrap_or(&Value::Null) {
            Value::Null => "".to_string(),
            Value::Boolean(b) => b.to_string(),
            Value::Integer(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            Value::String(s) => s.clone(),
            Value::Array(_) | Value::Object(_) => "[complex]".to_string(), // Simplify complex structures for CSV
        };
        row.push(val_str);
    }
    csv_writer.write_record(&row)?;
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
}
