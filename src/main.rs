mod cli;
mod error;
mod expr;
mod io;
mod model;
mod pipeline;
mod stages;

use clap::Parser;
use cli::{Cli, Command};
use pipeline::Pipeline;
use stages::*;
use std::io::{stdin, stdout, BufReader, BufWriter};

fn main() -> miette::Result<()> {
    let cli = Cli::parse();

    let stdin_handle = stdin();
    let reader = BufReader::new(stdin_handle.lock());
    
    let stdout_handle = stdout();
    let writer = BufWriter::new(stdout_handle.lock());

    // IO In
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
                // If it's our pretty error, bubble it up. Otherwise fallback.
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
