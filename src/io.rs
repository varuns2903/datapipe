use crate::model::{Record, Value};
use anyhow::Result;
use serde_json::Deserializer;
use std::io::{BufRead, Write};

// JSON In
pub fn read_json_stream<'a, R: BufRead + 'a>(
    reader: R,
) -> impl Iterator<Item = Result<Record>> + 'a {
    let stream = Deserializer::from_reader(reader).into_iter::<Record>();
    stream.map(|res| res.map_err(|e| anyhow::anyhow!("JSON parse error: {}", e)))
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
