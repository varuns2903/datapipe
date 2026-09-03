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
    /// Filter records based on an expression
    Filter {
        /// The expression to evaluate (e.g., '.age > 25')
        expression: String,
    },
    /// Keep only the specified fields from each record
    Select {
        /// Comma-separated list of fields to keep (e.g., name,age)
        #[arg(value_delimiter = ',')]
        fields: Vec<String>,
    },
    /// Limit the stream to the first N records
    Limit {
        max: usize,
    },
    /// Sort the stream by a field
    Sort {
        field: String,
        #[arg(long)]
        desc: bool,
    },
    /// Keep only the first record for each unique value of a field
    Unique {
        field: String,
    },
    /// Stream and inspect data (Pass-through test)
    Inspect,
    /// Output the stream as CSV
    Csv,
}
