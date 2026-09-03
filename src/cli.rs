use clap::{Parser, Subcommand};

/// DataPipe: A streaming-first structured data processor
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Filter records based on an expression
    Filter {
        /// The expression to evaluate (e.g., '.age > 25')
        expression: String,
    },
}
