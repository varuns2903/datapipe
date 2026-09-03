mod model;
mod cli;

use clap::Parser;
use cli::{Cli, Command};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Filter { expression } => {
            println!("Filtering with expression: {}", expression);
        }
    }

    Ok(())
}
