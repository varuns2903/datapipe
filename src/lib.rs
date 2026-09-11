pub mod cli;
pub mod error;
pub mod expr;
pub mod io;
pub mod model;
pub mod par_iter;
pub mod pipeline;
pub mod pipeline_file;
pub mod stages;

use clap::{CommandFactory, Parser};
use cli::{Cli, Command};
use pipeline::{Pipeline, Stage};
use stages::*;
use std::io::{stdin, stdout, BufRead, BufReader, BufWriter, Write};

/// Parses a `filter`/`map` expression, converting the parser's error into
/// a `miette`-rendered diagnostic. Shared by every place an expression
/// string needs to become an `Expr` AST, so there's exactly one spot that
/// knows how to translate a parse failure into user-facing output.
fn parse_expr(expression: &str) -> miette::Result<crate::expr::Expr> {
    crate::expr::parse(expression).map_err(|e| {
        if let Ok(diag) = e.downcast::<crate::error::DataPipeError>() {
            diag.into()
        } else {
            miette::miette!("Failed to parse expression")
        }
    })
}

/// Builds the stage for a single `Command`, if it represents a pipeline
/// stage at all. Returns `Ok(None)` for `Csv`/`Table`/`Inspect`, which don't add a
/// stage (they're handled by the caller as output-format/no-op markers).
/// Shared by both direct CLI dispatch and `run`'s multi-stage pipeline
/// files, so there is exactly one place that knows how to turn a `Command`
/// into a `Stage`.
fn command_into_stage(command: Command, strict: bool) -> miette::Result<Option<Box<dyn Stage>>> {
    let stage: Box<dyn Stage> = match command {
        Command::Filter { expression } => {
            let ast = parse_expr(&expression)?;
            Box::new(FilterStage { ast })
        }
        Command::Search { text, regex } => {
            if regex {
                let re = regex::Regex::new(&text)
                    .map_err(|e| miette::miette!("Invalid regex pattern '{}': {}", text, e))?;
                Box::new(SearchStage {
                    literal: None,
                    regex: Some(re),
                })
            } else {
                Box::new(SearchStage {
                    literal: Some(text),
                    regex: None,
                })
            }
        }
        Command::Select { fields, exclude } => Box::new(SelectStage { fields, exclude }),
        Command::Limit { max } => Box::new(LimitStage { max }),
        Command::Sort { fields } => {
            let fields = stages::parse_sort_spec(&fields)
                .map_err(|e| miette::miette!("Invalid sort spec: {e}"))?;
            Box::new(SortStage { fields })
        }
        Command::TopN { fields, n } => {
            let fields = stages::parse_sort_spec(&fields)
                .map_err(|e| miette::miette!("Invalid sort spec: {e}"))?;
            Box::new(TopNStage { fields, n })
        }
        Command::Unique { fields } => {
            let fields = stages::parse_field_list(&fields)
                .map_err(|e| miette::miette!("Invalid unique fields: {e}"))?;
            Box::new(UniqueStage { fields })
        }
        Command::Dedup => Box::new(DedupStage),
        Command::Count => Box::new(CountStage),
        Command::Sum { field } => Box::new(SumStage { field }),
        Command::Avg { field } => Box::new(AvgStage { field }),
        Command::Min { field } => Box::new(MinStage { field }),
        Command::Max { field } => Box::new(MaxStage { field }),
        Command::Schema => Box::new(SchemaStage),
        Command::Stats => Box::new(StatsStage),
        Command::Group { by, sum, count } => {
            let by = stages::parse_field_list(&by)
                .map_err(|e| miette::miette!("Invalid group by fields: {e}"))?;
            Box::new(GroupStage { by, sum, count })
        }
        Command::Freq { field, limit } => Box::new(FreqStage { field, limit }),
        Command::Explode { field } => Box::new(ExplodeStage { field }),
        Command::Rename { renames } => {
            let mut pairs = Vec::with_capacity(renames.len());
            for entry in renames {
                let (old, new) = entry.split_once(':').ok_or_else(|| {
                    miette::miette!("Invalid rename '{}': expected format old:new", entry)
                })?;
                pairs.push((old.to_string(), new.to_string()));
            }
            Box::new(RenameStage { renames: pairs })
        }
        Command::Flatten { sep } => Box::new(FlattenStage { separator: sep }),
        Command::Sample { n } => Box::new(SampleStage { n }),
        Command::Map {
            field,
            expression,
            set,
        } => {
            let mut assignments = Vec::with_capacity(1 + set.len());
            assignments.push((field, parse_expr(&expression)?));
            for entry in set {
                let (f, e) = entry.split_once('=').ok_or_else(|| {
                    miette::miette!("Invalid --set '{}': expected FIELD=EXPRESSION", entry)
                })?;
                assignments.push((f.to_string(), parse_expr(e)?));
            }
            Box::new(MapStage { assignments })
        }
        Command::Join {
            file,
            on,
            join_type,
            merge,
        } => {
            let on = stages::parse_field_list(&on)
                .map_err(|e| miette::miette!("Invalid join on fields: {e}"))?;
            let f = std::fs::File::open(&file)
                .map_err(|e| miette::miette!("Failed to open join file: {}", e))?;
            // A join file this large is exactly the case --merge exists
            // for, and exactly the case worth shipping compressed - so
            // `.gz`/`.zst` are decompressed transparently rather than
            // requiring the caller to pre-decompress to a temp file.
            // `.csv`/JSONL detection looks at the name with a trailing
            // compression suffix stripped, so `sales.csv.gz` is still
            // recognized as CSV.
            let (base_name, compression) = if let Some(stripped) = file.strip_suffix(".gz") {
                (stripped, Some("gz"))
            } else if let Some(stripped) = file.strip_suffix(".zst") {
                (stripped, Some("zst"))
            } else {
                (file.as_str(), None)
            };
            let is_csv = base_name.ends_with(".csv");
            // 'static: every branch here is fully owned (no borrows), so
            // this stream doesn't need to be tied to this function's
            // lifetime - needed for the --merge path below, which stores
            // the pre-sorted right-hand stream inside MergeJoinStage across
            // the whole pipeline's execution, not just this construction step.
            let reader: Box<dyn std::io::BufRead> = match compression {
                Some("gz") => Box::new(BufReader::new(flate2::read::MultiGzDecoder::new(f))),
                Some("zst") => Box::new(BufReader::new(
                    ruzstd::decoding::StreamingDecoder::new(f)
                        .map_err(|e| miette::miette!("Invalid zstd join file: {e}"))?,
                )),
                _ => Box::new(BufReader::new(f)),
            };
            let join_records: crate::pipeline::RecordStream<'static> = if is_csv {
                Box::new(
                    crate::io::read_csv_stream(reader, crate::io::CSV_DELIMITER)
                        .map_err(|e| miette::miette!(e.to_string()))?,
                )
            } else {
                crate::io::read_json_stream(reader)
            };

            if merge {
                let join_records = apply_strict_policy(join_records, strict);
                let sort_fields: Vec<(String, bool)> =
                    on.iter().map(|f| (f.clone(), false)).collect();
                let right_sorted = stages::external_sort(join_records, sort_fields);
                Box::new(MergeJoinStage::new(on, join_type, right_sorted))
            } else {
                let mut hash_map = std::collections::HashMap::new();
                for res in join_records {
                    let rec = match res {
                        Ok(rec) => rec,
                        Err(e) if strict => {
                            return Err(miette::miette!("Malformed record in join file: {e}"));
                        }
                        Err(e) => {
                            eprintln!("Warning: skipping malformed record in join file: {e}");
                            continue;
                        }
                    };
                    let key = match stages::join_key_for(&rec, &on) {
                        Some(k) => k,
                        None => continue,
                    };
                    hash_map.insert(key, rec);
                }
                Box::new(JoinStage {
                    hash_map: std::sync::Arc::new(hash_map),
                    on,
                    join_type,
                })
            }
        }
        Command::Inspect | Command::Csv | Command::Tsv | Command::Table => return Ok(None),
        Command::Completions { .. } | Command::Man | Command::Run { .. } => {
            unreachable!("handled before pipeline setup")
        }
    };
    Ok(Some(stage))
}

/// Wraps a raw record stream so a malformed record is skipped with a
/// warning by default, or left in the stream (for the first consumer to
/// bail on) under `--strict`.
fn apply_strict_policy(
    records: crate::pipeline::RecordStream,
    strict: bool,
) -> crate::pipeline::RecordStream {
    if strict {
        records
    } else {
        Box::new(records.filter_map(|res| match res {
            Ok(rec) => Some(Ok(rec)),
            Err(e) => {
                eprintln!("Warning: skipping malformed record: {e}");
                None
            }
        }))
    }
}

/// Gzip's two-byte magic number (RFC 1952). Sniffed rather than requiring
/// a `--gzip` flag: stdin has no filename to check an extension against,
/// and these two bytes can't start a valid JSONL/CSV stream (0x1f is a
/// control character), so detection is unambiguous.
const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

/// Zstandard frame magic number (little-endian 0xFD2FB528), equally
/// unambiguous against JSON/CSV/TSV's first byte.
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];

/// Peeks at stdin's first few bytes (without consuming them - `fill_buf`
/// only fills the internal buffer) and transparently decompresses the
/// stream if they match the gzip or zstd magic number. Shared by both
/// direct CLI dispatch and `run`, so `cat data.jsonl.gz | dp count` and
/// `cat data.jsonl.zst | dp run pipeline.toml` both just work.
fn maybe_decompress_stdin(
    mut reader: BufReader<std::io::StdinLock>,
) -> miette::Result<Box<dyn std::io::BufRead + '_>> {
    let peeked = reader
        .fill_buf()
        .map_err(|e| miette::miette!("Failed to read stdin: {e}"))?;
    if peeked.starts_with(&GZIP_MAGIC) {
        Ok(Box::new(BufReader::new(
            flate2::bufread::MultiGzDecoder::new(reader),
        )))
    } else if peeked.starts_with(&ZSTD_MAGIC) {
        // Unlike gzip's decoder, ruzstd's StreamingDecoder::new eagerly
        // reads and validates the frame header during construction, so a
        // corrupt zstd stream fails right here rather than lazily on
        // first read - hence the Result return type this function has
        // (gzip's decoder never fails at construction time).
        let decoder = ruzstd::decoding::StreamingDecoder::new(reader)
            .map_err(|e| miette::miette!("Invalid zstd stream: {e}"))?;
        Ok(Box::new(BufReader::new(decoder)))
    } else {
        Ok(Box::new(reader))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputFormat {
    Json,
    Csv,
    Tsv,
}

fn read_input(
    format: InputFormat,
    reader: BufReader<std::io::StdinLock>,
) -> miette::Result<crate::pipeline::RecordStream> {
    let reader = maybe_decompress_stdin(reader)?;
    match format {
        InputFormat::Csv => Ok(Box::new(
            crate::io::read_csv_stream(reader, crate::io::CSV_DELIMITER)
                .map_err(|e| miette::miette!(e.to_string()))?,
        )),
        InputFormat::Tsv => Ok(Box::new(
            crate::io::read_csv_stream(reader, crate::io::TSV_DELIMITER)
                .map_err(|e| miette::miette!(e.to_string()))?,
        )),
        InputFormat::Json => Ok(crate::io::read_json_stream(reader)),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Json,
    Csv,
    Tsv,
    Table,
}

fn write_output(
    writer: BufWriter<std::io::StdoutLock>,
    format: OutputFormat,
    pretty: bool,
    raw: bool,
    result_stream: crate::pipeline::RecordStream,
) -> miette::Result<()> {
    match format {
        OutputFormat::Csv => {
            crate::io::write_csv_stream(writer, result_stream, crate::io::CSV_DELIMITER)
                .map_err(|e| miette::miette!(e.to_string()))
        }
        OutputFormat::Tsv => {
            crate::io::write_csv_stream(writer, result_stream, crate::io::TSV_DELIMITER)
                .map_err(|e| miette::miette!(e.to_string()))
        }
        OutputFormat::Table => crate::io::write_table_stream(writer, result_stream)
            .map_err(|e| miette::miette!(e.to_string())),
        // --raw only means anything for JSON output - csv/tsv/table
        // already render scalar cells unquoted, so there's nothing extra
        // for --raw to do there, and it's silently ignored rather than
        // erroring (matching --pretty's same "no-op outside JSON" policy).
        OutputFormat::Json if raw => crate::io::write_raw_stream(writer, result_stream)
            .map_err(|e| miette::miette!(e.to_string())),
        OutputFormat::Json => crate::io::write_json_stream(writer, result_stream, pretty)
            .map_err(|e| miette::miette!(e.to_string())),
    }
}

pub fn run_cli() -> miette::Result<()> {
    let cli = Cli::parse();

    // Handled before any stdin/stdout pipeline setup, since it doesn't
    // consume records at all. Matched by reference so `cli.command` is still
    // usable below for the rest of the pipeline dispatch.
    if let Command::Completions { shell } = &cli.command {
        let mut cmd = Cli::command();
        let name = cmd.get_name().to_string();
        // Generate into an in-memory buffer rather than writing straight to
        // stdout: clap_complete panics internally on a write error, which
        // would otherwise crash on something as ordinary as piping into
        // `head`. Writing the buffered result ourselves lets us just ignore
        // a closed pipe, matching how well-behaved Unix CLIs handle it.
        let mut buf = Vec::new();
        clap_complete::generate(*shell, &mut cmd, name, &mut buf);
        let _ = stdout().write_all(&buf);
        return Ok(());
    }

    // Same rationale as Completions above: no input records needed, and we
    // buffer first so a closed downstream pipe doesn't turn into an error.
    if matches!(cli.command, Command::Man) {
        let cmd = Cli::command();
        let man = clap_mangen::Man::new(cmd);
        let mut buf = Vec::new();
        man.render(&mut buf)
            .map_err(|e| miette::miette!("Failed to render man page: {e}"))?;
        let _ = stdout().write_all(&buf);
        return Ok(());
    }

    if let Command::Run { file } = &cli.command {
        let contents = std::fs::read_to_string(file)
            .map_err(|e| miette::miette!("Failed to read pipeline file '{}': {}", file, e))?;
        let spec = crate::pipeline_file::parse(&contents).map_err(|e| miette::miette!("{e}"))?;

        let stdin_handle = stdin();
        let reader = BufReader::new(stdin_handle.lock());
        let stdout_handle = stdout();
        let writer = BufWriter::new(stdout_handle.lock());

        let in_format = if spec.in_tsv {
            InputFormat::Tsv
        } else if spec.in_csv {
            InputFormat::Csv
        } else {
            InputFormat::Json
        };
        let records = read_input(in_format, reader)?;
        // A pipeline file's `strict` setting combines with the global CLI
        // flag - either one asking for strict mode is enough, matching the
        // intuition that --strict on the command line should never be
        // silently overridden by a file that doesn't mention it.
        let strict = spec.strict || cli.strict;
        let records = apply_strict_policy(records, strict);

        let mut pipeline = Pipeline::new();
        for stage_spec in spec.stages {
            let command = crate::pipeline_file::into_command(stage_spec);
            if let Some(stage) = command_into_stage(command, strict)? {
                pipeline.add_stage(stage);
            }
        }

        let result_stream = pipeline.process(records);
        let pretty = spec.pretty || cli.pretty;
        let raw = spec.raw || cli.raw;
        let format = if spec.out_table {
            OutputFormat::Table
        } else if spec.out_tsv {
            OutputFormat::Tsv
        } else if spec.out_csv {
            OutputFormat::Csv
        } else {
            OutputFormat::Json
        };
        return write_output(writer, format, pretty, raw, result_stream);
    }

    let stdin_handle = stdin();
    let reader = BufReader::new(stdin_handle.lock());
    let stdout_handle = stdout();
    let writer = BufWriter::new(stdout_handle.lock());

    let in_format = if cli.in_tsv {
        InputFormat::Tsv
    } else if cli.in_csv {
        InputFormat::Csv
    } else {
        InputFormat::Json
    };
    let records = read_input(in_format, reader)?;
    let records = apply_strict_policy(records, cli.strict);

    let mut pipeline = Pipeline::new();
    let format = match cli.command {
        Command::Csv => OutputFormat::Csv,
        Command::Tsv => OutputFormat::Tsv,
        Command::Table => OutputFormat::Table,
        _ => OutputFormat::Json,
    };
    let strict = cli.strict;

    if let Some(stage) = command_into_stage(cli.command, strict)? {
        pipeline.add_stage(stage);
    }

    let result_stream = pipeline.process(records);
    write_output(writer, format, cli.pretty, cli.raw, result_stream)
}
