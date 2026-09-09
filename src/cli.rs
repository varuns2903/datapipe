use crate::stages::JoinType;
use clap::{Parser, Subcommand};
use clap_complete::Shell;

#[derive(Parser, Debug)]
#[command(name = "dp", author, version, about, long_about = None)]
pub struct Cli {
    /// Read the input as CSV instead of JSONL
    #[arg(long, global = true)]
    pub in_csv: bool,

    /// Abort on the first malformed record instead of skipping it with a warning
    #[arg(long, global = true)]
    pub strict: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Keep only records where the expression evaluates to true
    Filter {
        /// A boolean expression, e.g. `.age > 25 && .admin == true`
        expression: String,
    },
    /// Keep only the specified comma-separated fields
    Select {
        #[arg(value_delimiter = ',')]
        fields: Vec<String>,
    },
    /// Halt the stream after yielding N records
    Limit { max: usize },
    /// Sort records by the specified field (buffers the full stream)
    Sort {
        field: String,
        /// Sort in descending order
        #[arg(long)]
        desc: bool,
    },
    /// Keep only the first record for each distinct value of a field
    Unique { field: String },
    /// Consume the stream and yield the total record count
    Count,
    /// Compute the sum of a numeric field
    Sum { field: String },
    /// Compute the average of a numeric field
    Avg { field: String },
    /// Find the minimum value of a field
    Min { field: String },
    /// Find the maximum value of a field
    Max { field: String },
    /// Infer and print the data type(s) of every field in the stream
    Schema,
    /// Pass the stream through unchanged (useful for debugging a pipeline)
    Inspect,
    /// Output the stream as CSV instead of JSONL
    Csv,
    /// Group records by a field, optionally summing another field and/or counting
    Group {
        by: String,
        /// A numeric field to sum within each group
        #[arg(long)]
        sum: Option<String>,
        /// Include a count of records in each group
        #[arg(long)]
        count: bool,
    },
    /// Join each record with a matching record from another file
    Join {
        /// Path to a JSONL or CSV file to join against
        file: String,
        /// The field to join on (must exist in both streams)
        #[arg(long)]
        on: String,
        /// The kind of join to perform
        #[arg(long = "type", value_enum, default_value = "left")]
        join_type: JoinType,
    },
    /// Explode an array field into multiple records
    Explode { field: String },
    /// Compute a new field or overwrite an existing one using an expression
    Map { field: String, expression: String },
    /// Generate a shell completion script and print it to stdout
    Completions {
        /// The shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
    /// Generate a troff man page and print it to stdout
    Man,
}
