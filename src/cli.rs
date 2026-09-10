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

    /// Pretty-print JSON output (indented, one object may span multiple
    /// lines). Not valid JSONL - don't pipe this back into another `dp`.
    #[arg(long, short = 'p', global = true)]
    pub pretty: bool,

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
    /// Keep records where any field's value contains the given text,
    /// without needing to know the field names up front
    Search {
        text: String,
        /// Treat `text` as a regex instead of a literal substring
        #[arg(long)]
        regex: bool,
    },
    /// Keep only the specified comma-separated fields
    Select {
        #[arg(value_delimiter = ',')]
        fields: Vec<String>,
        /// Treat `fields` as an exclusion list: keep everything except these
        #[arg(long)]
        exclude: bool,
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
    /// Drop exact duplicate records (comparing all fields), keeping the
    /// first occurrence
    Dedup,
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
    /// Compute count/nulls/distinct/min/max/mean/stddev for every field in
    /// one pass, yielding one summary record per field
    Stats,
    /// Pass the stream through unchanged (useful for debugging a pipeline)
    Inspect,
    /// Output the stream as CSV instead of JSONL
    Csv,
    /// Output the stream as an aligned, human-readable table (buffers the
    /// whole stream to compute column widths)
    Table,
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
    /// Count occurrences of each distinct value in a field, sorted most-
    /// frequent first, with percentage of the stream
    Freq {
        field: String,
        /// Keep only the top N most frequent values
        #[arg(long)]
        limit: Option<usize>,
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
    /// Rename one or more fields, e.g. `old1:new1,old2:new2`
    Rename {
        #[arg(value_delimiter = ',')]
        renames: Vec<String>,
    },
    /// Flatten nested objects into dot-path keys, e.g. .user.age -> "user.age"
    Flatten {
        /// The separator to join path segments with
        #[arg(long, default_value = ".")]
        sep: String,
    },
    /// Take a uniform random sample of N records (buffers up to N records)
    Sample { n: usize },
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
    /// Run a multi-stage pipeline defined in a TOML file
    Run {
        /// Path to a pipeline definition file
        file: String,
    },
}
