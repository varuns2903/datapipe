mod cli;
mod expr;
mod io;
mod model;
mod pipeline;
mod stages;

use clap::Parser;
use cli::{Cli, Command};
use pipeline::Pipeline;
use stages::{FilterStage, LimitStage, SelectStage, SortStage, UniqueStage};
use std::io::{stdin, stdout, BufReader, BufWriter};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let stdin_handle = stdin();
    let reader = BufReader::new(stdin_handle.lock());
    
    let stdout_handle = stdout();
    let writer = BufWriter::new(stdout_handle.lock());

    // IO In
    let records: crate::pipeline::RecordStream = if cli.in_csv {
        Box::new(crate::io::read_csv_stream(reader)?)
    } else {
        Box::new(crate::io::read_json_stream(reader))
    };

    // Build Pipeline
    let mut pipeline = Pipeline::new();
    
    let is_csv_out = matches!(cli.command, Command::Csv);

    match cli.command {
        Command::Filter { expression } => {
            let ast = crate::expr::parse(&expression)?;
            pipeline.add_stage(Box::new(FilterStage { ast }));
        }
        Command::Select { fields } => {
            pipeline.add_stage(Box::new(SelectStage { fields }));
        }
        Command::Limit { max } => {
            pipeline.add_stage(Box::new(LimitStage { max }));
        }
        Command::Sort { field, desc } => {
            pipeline.add_stage(Box::new(SortStage { field, desc }));
        }
        Command::Unique { field } => {
            pipeline.add_stage(Box::new(UniqueStage { field }));
        }
        Command::Inspect | Command::Csv => {
            // No transformation stages added
        }
    }

    // Process
    let result_stream = pipeline.process(records);

    // IO Out
    if is_csv_out {
        crate::io::write_csv_stream(writer, result_stream)?;
    } else {
        crate::io::write_json_stream(writer, result_stream)?;
    }

    Ok(())
}
