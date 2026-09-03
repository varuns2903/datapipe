mod cli;
mod io;
mod model;

use clap::Parser;
use cli::{Cli, Command};
use std::io::{stdin, stdout, BufReader, BufWriter};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Filter { expression } => {
            println!("Filtering with expression: {}", expression);
        }
        Command::Inspect => {
            let stdin_handle = stdin();
            let reader = BufReader::new(stdin_handle.lock());
            
            let stdout_handle = stdout();
            let writer = BufWriter::new(stdout_handle.lock());

            let records = crate::io::read_json_stream(reader);
            crate::io::write_json_stream(writer, records)?;
        }
    }

    Ok(())
}
