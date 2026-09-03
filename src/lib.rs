pub mod cli;
pub mod error;
pub mod expr;
pub mod io;
pub mod model;
pub mod pipeline;
pub mod stages;
pub mod par_iter;

use clap::Parser;
use cli::{Cli, Command};
use pipeline::Pipeline;
use stages::*;
use std::io::{stdin, stdout, BufReader, BufWriter};

pub fn run_cli() -> miette::Result<()> {
    let cli = Cli::parse();
    let stdin_handle = stdin();
    let reader = BufReader::new(stdin_handle.lock());
    let stdout_handle = stdout();
    let writer = BufWriter::new(stdout_handle.lock());

    let records: crate::pipeline::RecordStream = if cli.in_csv {
        Box::new(crate::io::read_csv_stream(reader).map_err(|e| miette::miette!(e.to_string()))?)
    } else {
        Box::new(crate::io::read_json_stream(reader))
    };

    let mut pipeline = Pipeline::new();
    let is_csv_out = matches!(cli.command, Command::Csv);

    match cli.command {
        Command::Filter { expression } => {
            let ast = crate::expr::parse(&expression).map_err(|e| {
                if let Ok(diag) = e.downcast::<crate::error::DataPipeError>() { diag.into() } else { miette::miette!("Failed to parse expression") }
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
        Command::Group { by, sum, count } => pipeline.add_stage(Box::new(GroupStage { by, sum, count })),
        Command::Join { file, on } => {
            let f = std::fs::File::open(&file).map_err(|e| miette::miette!("Failed to open join file: {}", e))?;
            let reader = BufReader::new(f);
            let join_records: crate::pipeline::RecordStream = if file.ends_with(".csv") {
                Box::new(crate::io::read_csv_stream(reader).map_err(|e| miette::miette!(e.to_string()))?)
            } else {
                Box::new(crate::io::read_json_stream(reader))
            };
            
            let mut hash_map = std::collections::HashMap::new();
            for res in join_records {
                if let Ok(rec) = res {
                    let key = match rec.get(&on) {
                        Some(crate::model::Value::String(s)) => s.clone(),
                        Some(val) => serde_json::to_string(val).unwrap_or_default(),
                        None => continue,
                    };
                    hash_map.insert(key, rec);
                }
            }
            pipeline.add_stage(Box::new(JoinStage { hash_map: std::sync::Arc::new(hash_map), on }));
        }
        Command::Inspect | Command::Csv => {}
    }

    let result_stream = pipeline.process(records);
    if is_csv_out {
        crate::io::write_csv_stream(writer, result_stream).map_err(|e| miette::miette!(e.to_string()))?;
    } else {
        crate::io::write_json_stream(writer, result_stream).map_err(|e| miette::miette!(e.to_string()))?;
    }
    Ok(())
}
