use clap::{Parser, Subcommand};

/// DataPipe: A streaming-first structured data processor
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Read input as CSV instead of JSON
    #[arg(long, global = true)]
    pub in_csv: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    Filter {
        expression: String,
    },
    Select {
        #[arg(value_delimiter = ',')]
        fields: Vec<String>,
    },
    Limit {
        max: usize,
    },
    Sort {
        field: String,
        #[arg(long)]
        desc: bool,
    },
    Unique {
        field: String,
    },
    Count,
    Sum {
        field: String,
    },
    Avg {
        field: String,
    },
    Min {
        field: String,
    },
    Max {
        field: String,
    },
    Inspect,
    Csv,
    /// Infer and print the schema of the stream
    Schema,
}
