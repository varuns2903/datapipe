use crate::model::Record;
use anyhow::Result;
use serde_json::Deserializer;
use std::io::{BufRead, Write};

/// Reads a stream of JSON or NDJSON objects from a reader.
pub fn read_json_stream<'a, R: BufRead + 'a>(
    reader: R,
) -> impl Iterator<Item = Result<Record>> + 'a {
    let stream = Deserializer::from_reader(reader).into_iter::<Record>();
    stream.map(|res| res.map_err(|e| anyhow::anyhow!("JSON parse error: {}", e)))
}

/// Writes a stream of records as NDJSON to a writer.
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
