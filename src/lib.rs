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
use std::io::{stdin, stdout, BufReader, BufWriter, Write};

/// Builds the stage for a single `Command`, if it represents a pipeline
/// stage at all. Returns `Ok(None)` for `Csv`/`Table`/`Inspect`, which don't add a
/// stage (they're handled by the caller as output-format/no-op markers).
/// Shared by both direct CLI dispatch and `run`'s multi-stage pipeline
/// files, so there is exactly one place that knows how to turn a `Command`
/// into a `Stage`.
fn command_into_stage(command: Command, strict: bool) -> miette::Result<Option<Box<dyn Stage>>> {
    let stage: Box<dyn Stage> = match command {
        Command::Filter { expression } => {
            let ast = crate::expr::parse(&expression).map_err(|e| {
                if let Ok(diag) = e.downcast::<crate::error::DataPipeError>() {
                    diag.into()
                } else {
                    miette::miette!("Failed to parse expression")
                }
            })?;
            Box::new(FilterStage { ast })
        }
        Command::Select { fields, exclude } => Box::new(SelectStage { fields, exclude }),
        Command::Limit { max } => Box::new(LimitStage { max }),
        Command::Sort { field, desc } => Box::new(SortStage { field, desc }),
        Command::Unique { field } => Box::new(UniqueStage { field }),
        Command::Count => Box::new(CountStage),
        Command::Sum { field } => Box::new(SumStage { field }),
        Command::Avg { field } => Box::new(AvgStage { field }),
        Command::Min { field } => Box::new(MinStage { field }),
        Command::Max { field } => Box::new(MaxStage { field }),
        Command::Schema => Box::new(SchemaStage),
        Command::Group { by, sum, count } => Box::new(GroupStage { by, sum, count }),
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
        Command::Map { field, expression } => {
            let ast = crate::expr::parse(&expression).map_err(|e| {
                if let Ok(diag) = e.downcast::<crate::error::DataPipeError>() {
                    diag.into()
                } else {
                    miette::miette!("Failed to parse expression")
                }
            })?;
            Box::new(MapStage { field, ast })
        }
        Command::Join {
            file,
            on,
            join_type,
        } => {
            let f = std::fs::File::open(&file)
                .map_err(|e| miette::miette!("Failed to open join file: {}", e))?;
            let reader = BufReader::new(f);
            let join_records: crate::pipeline::RecordStream = if file.ends_with(".csv") {
                Box::new(
                    crate::io::read_csv_stream(reader)
                        .map_err(|e| miette::miette!(e.to_string()))?,
                )
            } else {
                Box::new(crate::io::read_json_stream(reader))
            };

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
                let key = match rec.get(&on) {
                    Some(crate::model::Value::String(s)) => s.clone(),
                    Some(val) => serde_json::to_string(val).unwrap_or_default(),
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
        Command::Inspect | Command::Csv | Command::Table => return Ok(None),
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

fn read_input(
    in_csv: bool,
    reader: BufReader<std::io::StdinLock>,
) -> miette::Result<crate::pipeline::RecordStream> {
    if in_csv {
        Ok(Box::new(
            crate::io::read_csv_stream(reader).map_err(|e| miette::miette!(e.to_string()))?,
        ))
    } else {
        Ok(Box::new(crate::io::read_json_stream(reader)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Json,
    Csv,
    Table,
}

fn write_output(
    writer: BufWriter<std::io::StdoutLock>,
    format: OutputFormat,
    pretty: bool,
    result_stream: crate::pipeline::RecordStream,
) -> miette::Result<()> {
    match format {
        OutputFormat::Csv => crate::io::write_csv_stream(writer, result_stream)
            .map_err(|e| miette::miette!(e.to_string())),
        OutputFormat::Table => crate::io::write_table_stream(writer, result_stream)
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

        let records = read_input(spec.in_csv, reader)?;
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
        let format = if spec.out_table {
            OutputFormat::Table
        } else if spec.out_csv {
            OutputFormat::Csv
        } else {
            OutputFormat::Json
        };
        return write_output(writer, format, pretty, result_stream);
    }

    let stdin_handle = stdin();
    let reader = BufReader::new(stdin_handle.lock());
    let stdout_handle = stdout();
    let writer = BufWriter::new(stdout_handle.lock());

    let records = read_input(cli.in_csv, reader)?;
    let records = apply_strict_policy(records, cli.strict);

    let mut pipeline = Pipeline::new();
    let format = match cli.command {
        Command::Csv => OutputFormat::Csv,
        Command::Table => OutputFormat::Table,
        _ => OutputFormat::Json,
    };
    let strict = cli.strict;

    if let Some(stage) = command_into_stage(cli.command, strict)? {
        pipeline.add_stage(stage);
    }

    let result_stream = pipeline.process(records);
    write_output(writer, format, cli.pretty, result_stream)
}
