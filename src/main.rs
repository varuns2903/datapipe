mod cli;
mod io;
mod model;
mod pipeline;
mod stages;

use clap::Parser;
use cli::{Cli, Command};
use pipeline::Pipeline;
use stages::{LimitStage, SelectStage};
use std::io::{stdin, stdout, BufReader, BufWriter};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let stdin_handle = stdin();
    let reader = BufReader::new(stdin_handle.lock());
    
    let stdout_handle = stdout();
    let writer = BufWriter::new(stdout_handle.lock());

    // IO In
    let records = crate::io::read_json_stream(reader);

    // Build Pipeline
    let mut pipeline = Pipeline::new();

    match cli.command {
        Command::Filter { expression } => {
            println!("Filtering with expression: {}", expression);
            return Ok(());
        }
        Command::Select { fields } => {
            pipeline.add_stage(Box::new(SelectStage { fields }));
        }
        Command::Limit { max } => {
            pipeline.add_stage(Box::new(LimitStage { max }));
        }
        Command::Inspect => {
            // No stages added, just pass-through!
        }
    }

    // Process & IO Out
    let result_stream = pipeline.process(Box::new(records));
    crate::io::write_json_stream(writer, result_stream)?;

    Ok(())
}
