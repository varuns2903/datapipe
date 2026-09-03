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
    /// Group records by a field and compute aggregates
    Group {
        /// The field to group by
        by: String,
        /// Optional field to sum within each group
        #[arg(long)]
        sum: Option<String>,
        /// Whether to count the number of records in each group
        #[arg(long)]
        count: bool,
    }
}
