pub mod cli;
pub mod error;
pub mod expr;
pub mod io;
pub mod model;
pub mod par_iter;
pub mod pipeline;
pub mod stages;

use clap::{CommandFactory, Parser};
use cli::{Cli, Command};
use pipeline::Pipeline;
use stages::*;
use std::io::{stdin, stdout, BufReader, BufWriter};

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
        use std::io::Write;
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
        use std::io::Write;
        let _ = stdout().write_all(&buf);
        return Ok(());
    }

    let stdin_handle = stdin();
    let reader = BufReader::new(stdin_handle.lock());
    let stdout_handle = stdout();
    let writer = BufWriter::new(stdout_handle.lock());

    let records: crate::pipeline::RecordStream = if cli.in_csv {
        Box::new(crate::io::read_csv_stream(reader).map_err(|e| miette::miette!(e.to_string()))?)
    } else {
        Box::new(crate::io::read_json_stream(reader))
    };

    // By default, skip malformed records (with a warning) rather than letting
    // one bad line abort the whole pipeline. `--strict` restores fail-fast
    // behavior by leaving errors in the stream for the first consumer to bail on.
    let records: crate::pipeline::RecordStream = if cli.strict {
        records
    } else {
        Box::new(records.filter_map(|res| match res {
            Ok(rec) => Some(Ok(rec)),
            Err(e) => {
                eprintln!("Warning: skipping malformed record: {e}");
                None
            }
        }))
    };

    let mut pipeline = Pipeline::new();
    let is_csv_out = matches!(cli.command, Command::Csv);

    match cli.command {
        Command::Filter { expression } => {
            let ast = crate::expr::parse(&expression).map_err(|e| {
                if let Ok(diag) = e.downcast::<crate::error::DataPipeError>() {
                    diag.into()
                } else {
                    miette::miette!("Failed to parse expression")
                }
            })?;
            pipeline.add_stage(Box::new(FilterStage { ast }));
        }
        Command::Select { fields } => pipeline.add_stage(Box::new(SelectStage { fields })),
        Command::Limit { max } => pipeline.add_stage(Box::new(LimitStage { max })),
        Command::Sort { field, desc } => pipeline.add_stage(Box::new(SortStage { field, desc })),
        Command::Unique { field } => pipeline.add_stage(Box::new(UniqueStage { field })),
        Command::Count => pipeline.add_stage(Box::new(CountStage)),
        Command::Sum { field } => pipeline.add_stage(Box::new(SumStage { field })),
        Command::Avg { field } => pipeline.add_stage(Box::new(AvgStage { field })),
        Command::Min { field } => pipeline.add_stage(Box::new(MinStage { field })),
        Command::Max { field } => pipeline.add_stage(Box::new(MaxStage { field })),
        Command::Schema => pipeline.add_stage(Box::new(SchemaStage)),
        Command::Group { by, sum, count } => {
            pipeline.add_stage(Box::new(GroupStage { by, sum, count }))
        }
        Command::Explode { field } => pipeline.add_stage(Box::new(ExplodeStage { field })),
        Command::Rename { renames } => {
            let mut pairs = Vec::with_capacity(renames.len());
            for entry in renames {
                let (old, new) = entry.split_once(':').ok_or_else(|| {
                    miette::miette!("Invalid rename '{}': expected format old:new", entry)
                })?;
                pairs.push((old.to_string(), new.to_string()));
            }
            pipeline.add_stage(Box::new(RenameStage { renames: pairs }));
        }
        Command::Flatten { sep } => pipeline.add_stage(Box::new(FlattenStage { separator: sep })),
        Command::Sample { n } => pipeline.add_stage(Box::new(SampleStage { n })),
        Command::Map { field, expression } => {
            let ast = crate::expr::parse(&expression).map_err(|e| {
                if let Ok(diag) = e.downcast::<crate::error::DataPipeError>() {
                    diag.into()
                } else {
                    miette::miette!("Failed to parse expression")
                }
            })?;
            pipeline.add_stage(Box::new(MapStage { field, ast }));
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
                    Err(e) if cli.strict => {
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
            pipeline.add_stage(Box::new(JoinStage {
                hash_map: std::sync::Arc::new(hash_map),
                on,
                join_type,
            }));
        }
        Command::Inspect | Command::Csv => {}
        Command::Completions { .. } | Command::Man => {
            unreachable!("handled above before pipeline setup")
        }
    }

    let result_stream = pipeline.process(records);
    if is_csv_out {
        crate::io::write_csv_stream(writer, result_stream)
            .map_err(|e| miette::miette!(e.to_string()))?;
    } else {
        crate::io::write_json_stream(writer, result_stream)
            .map_err(|e| miette::miette!(e.to_string()))?;
    }
    Ok(())
}
