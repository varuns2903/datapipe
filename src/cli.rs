use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[arg(long, global = true)]
    pub in_csv: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    Filter { expression: String },
    Select { #[arg(value_delimiter = ',')] fields: Vec<String> },
    Limit { max: usize },
    Sort { field: String, #[arg(long)] desc: bool },
    Unique { field: String },
    Count,
    Sum { field: String },
    Avg { field: String },
    Min { field: String },
    Max { field: String },
    Schema,
    Inspect,
    Csv,
    Group {
        by: String,
        #[arg(long)]
        sum: Option<String>,
        #[arg(long)]
        count: bool,
    },
    /// Join the stream with another JSON/CSV file
    Join {
        /// The file to join with (must be .csv or .jsonl)
        file: String,
        /// The field to join on (must exist in both streams)
        #[arg(long)]
        on: String,
    }
}
