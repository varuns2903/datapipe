# DataPipe (`dp`)

DataPipe is a high-performance, streaming-first CLI tool for processing structured data (JSON, CSV, etc.) inspired by Unix pipelines. It elevates structured data to a first-class citizen in the terminal, allowing developers to filter, select, map, and aggregate records without the memory overhead of loading entire datasets into RAM.

## Usage (Planned)

```bash
cat data.jsonl | dp filter '.status == 500' | dp select path,user_id | dp table
```
