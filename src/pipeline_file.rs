use crate::cli::Command;
use crate::stages::JoinType;
use serde::Deserialize;

/// A declarative multi-stage pipeline definition, e.g.:
///
/// ```toml
/// in_csv = false
/// strict = false
/// out_csv = false
/// out_table = false
/// pretty = false
///
/// [[stages]]
/// type = "filter"
/// expression = ".age >= 21"
///
/// [[stages]]
/// type = "sort"
/// field = "age"
/// desc = true
/// ```
#[derive(Debug, Deserialize)]
pub struct PipelineFile {
    #[serde(default)]
    pub in_csv: bool,
    #[serde(default)]
    pub strict: bool,
    #[serde(default)]
    pub out_csv: bool,
    #[serde(default)]
    pub out_table: bool,
    #[serde(default)]
    pub pretty: bool,
    pub stages: Vec<StageSpec>,
}

/// Mirrors the subset of `Command` that represents an actual pipeline stage
/// (excludes format/utility commands like `csv`, `completions`, `man`, `run`
/// itself, which don't make sense - or would be ambiguous - inside a
/// pipeline file; `out_csv` on `PipelineFile` covers the `csv` case instead).
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StageSpec {
    Filter {
        expression: String,
    },
    Search {
        text: String,
        #[serde(default)]
        regex: bool,
    },
    Select {
        fields: Vec<String>,
        #[serde(default)]
        exclude: bool,
    },
    Limit {
        max: usize,
    },
    Sort {
        field: String,
        #[serde(default)]
        desc: bool,
    },
    Unique {
        field: String,
    },
    Dedup,
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
    Schema,
    Stats,
    Group {
        by: String,
        #[serde(default)]
        sum: Option<String>,
        #[serde(default)]
        count: bool,
    },
    Freq {
        field: String,
        #[serde(default)]
        limit: Option<usize>,
    },
    Explode {
        field: String,
    },
    Rename {
        renames: Vec<String>,
    },
    Flatten {
        #[serde(default = "default_flatten_sep")]
        sep: String,
    },
    Sample {
        n: usize,
    },
    Map {
        field: String,
        expression: String,
    },
    Join {
        file: String,
        on: String,
        #[serde(default = "default_join_type")]
        join_type: JoinType,
    },
}

fn default_flatten_sep() -> String {
    ".".to_string()
}

fn default_join_type() -> JoinType {
    JoinType::Left
}

pub fn parse(contents: &str) -> anyhow::Result<PipelineFile> {
    toml::from_str(contents).map_err(|e| anyhow::anyhow!("Invalid pipeline file: {e}"))
}

/// Converts a stage spec into the equivalent `Command`, so pipeline-file
/// stages and direct CLI subcommands share the exact same stage-construction
/// logic in `run_cli` rather than duplicating it.
pub fn into_command(spec: StageSpec) -> Command {
    match spec {
        StageSpec::Filter { expression } => Command::Filter { expression },
        StageSpec::Search { text, regex } => Command::Search { text, regex },
        StageSpec::Select { fields, exclude } => Command::Select { fields, exclude },
        StageSpec::Limit { max } => Command::Limit { max },
        StageSpec::Sort { field, desc } => Command::Sort { field, desc },
        StageSpec::Unique { field } => Command::Unique { field },
        StageSpec::Dedup => Command::Dedup,
        StageSpec::Count => Command::Count,
        StageSpec::Sum { field } => Command::Sum { field },
        StageSpec::Avg { field } => Command::Avg { field },
        StageSpec::Min { field } => Command::Min { field },
        StageSpec::Max { field } => Command::Max { field },
        StageSpec::Schema => Command::Schema,
        StageSpec::Stats => Command::Stats,
        StageSpec::Group { by, sum, count } => Command::Group { by, sum, count },
        StageSpec::Freq { field, limit } => Command::Freq { field, limit },
        StageSpec::Explode { field } => Command::Explode { field },
        StageSpec::Rename { renames } => Command::Rename { renames },
        StageSpec::Flatten { sep } => Command::Flatten { sep },
        StageSpec::Sample { n } => Command::Sample { n },
        StageSpec::Map { field, expression } => Command::Map { field, expression },
        StageSpec::Join {
            file,
            on,
            join_type,
        } => Command::Join {
            file,
            on,
            join_type,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_pipeline() {
        let toml = r#"
            [[stages]]
            type = "filter"
            expression = ".age >= 21"
        "#;
        let file = parse(toml).unwrap();
        assert!(!file.in_csv);
        assert!(!file.strict);
        assert!(!file.out_csv);
        assert_eq!(file.stages.len(), 1);
        assert!(matches!(file.stages[0], StageSpec::Filter { .. }));
    }

    #[test]
    fn parses_global_settings() {
        let toml = r#"
            in_csv = true
            strict = true
            out_csv = true
            pretty = true
            stages = []
        "#;
        let file = parse(toml).unwrap();
        assert!(file.in_csv);
        assert!(file.strict);
        assert!(file.out_csv);
        assert!(file.pretty);
        assert_eq!(file.stages.len(), 0);
    }

    #[test]
    fn pretty_defaults_to_false() {
        let toml = r#"
            stages = []
        "#;
        let file = parse(toml).unwrap();
        assert!(!file.pretty);
    }

    #[test]
    fn parses_multiple_stage_types() {
        let toml = r#"
            [[stages]]
            type = "filter"
            expression = ".age >= 21"

            [[stages]]
            type = "sort"
            field = "age"
            desc = true

            [[stages]]
            type = "select"
            fields = ["name", "age"]

            [[stages]]
            type = "count"
        "#;
        let file = parse(toml).unwrap();
        assert_eq!(file.stages.len(), 4);
        assert!(matches!(file.stages[0], StageSpec::Filter { .. }));
        assert!(matches!(file.stages[1], StageSpec::Sort { desc: true, .. }));
        assert!(matches!(file.stages[2], StageSpec::Select { .. }));
        assert!(matches!(file.stages[3], StageSpec::Count));
    }

    #[test]
    fn sort_desc_defaults_to_false() {
        let toml = r#"
            [[stages]]
            type = "sort"
            field = "age"
        "#;
        let file = parse(toml).unwrap();
        assert!(matches!(
            file.stages[0],
            StageSpec::Sort { desc: false, .. }
        ));
    }

    #[test]
    fn flatten_sep_defaults_to_dot() {
        let toml = r#"
            [[stages]]
            type = "flatten"
        "#;
        let file = parse(toml).unwrap();
        match &file.stages[0] {
            StageSpec::Flatten { sep } => assert_eq!(sep, "."),
            _ => panic!("expected Flatten"),
        }
    }

    #[test]
    fn join_type_defaults_to_left() {
        let toml = r#"
            [[stages]]
            type = "join"
            file = "right.jsonl"
            on = "id"
        "#;
        let file = parse(toml).unwrap();
        match &file.stages[0] {
            StageSpec::Join { join_type, .. } => assert_eq!(*join_type, JoinType::Left),
            _ => panic!("expected Join"),
        }
    }

    #[test]
    fn join_type_can_be_specified() {
        let toml = r#"
            [[stages]]
            type = "join"
            file = "right.jsonl"
            on = "id"
            join_type = "inner"
        "#;
        let file = parse(toml).unwrap();
        match &file.stages[0] {
            StageSpec::Join { join_type, .. } => assert_eq!(*join_type, JoinType::Inner),
            _ => panic!("expected Join"),
        }
    }

    #[test]
    fn rejects_unknown_stage_type() {
        let toml = r#"
            [[stages]]
            type = "not_a_real_stage"
        "#;
        assert!(parse(toml).is_err());
    }

    #[test]
    fn rejects_missing_required_field() {
        let toml = r#"
            [[stages]]
            type = "filter"
        "#;
        assert!(parse(toml).is_err());
    }

    #[test]
    fn rejects_malformed_toml() {
        assert!(parse("this is not [ valid toml").is_err());
    }

    #[test]
    fn into_command_round_trips_filter() {
        let spec = StageSpec::Filter {
            expression: ".a > 1".to_string(),
        };
        match into_command(spec) {
            Command::Filter { expression } => assert_eq!(expression, ".a > 1"),
            _ => panic!("expected Command::Filter"),
        }
    }

    #[test]
    fn select_exclude_defaults_to_false() {
        let toml = r#"
            [[stages]]
            type = "select"
            fields = ["a"]
        "#;
        let file = parse(toml).unwrap();
        assert!(matches!(
            file.stages[0],
            StageSpec::Select { exclude: false, .. }
        ));
    }

    #[test]
    fn select_exclude_can_be_specified() {
        let toml = r#"
            [[stages]]
            type = "select"
            fields = ["password"]
            exclude = true
        "#;
        let file = parse(toml).unwrap();
        assert!(matches!(
            file.stages[0],
            StageSpec::Select { exclude: true, .. }
        ));
    }

    #[test]
    fn out_table_defaults_to_false() {
        let toml = r#"
            stages = []
        "#;
        let file = parse(toml).unwrap();
        assert!(!file.out_table);
    }

    #[test]
    fn out_table_can_be_specified() {
        let toml = r#"
            out_table = true
            stages = []
        "#;
        let file = parse(toml).unwrap();
        assert!(file.out_table);
    }

    #[test]
    fn parses_stats_stage() {
        let toml = r#"
            [[stages]]
            type = "stats"
        "#;
        let file = parse(toml).unwrap();
        assert!(matches!(file.stages[0], StageSpec::Stats));
        assert!(matches!(into_command(StageSpec::Stats), Command::Stats));
    }

    #[test]
    fn parses_freq_stage_with_optional_limit() {
        let toml = r#"
            [[stages]]
            type = "freq"
            field = "status"
            limit = 5
        "#;
        let file = parse(toml).unwrap();
        match &file.stages[0] {
            StageSpec::Freq { field, limit } => {
                assert_eq!(field, "status");
                assert_eq!(*limit, Some(5));
            }
            _ => panic!("expected StageSpec::Freq"),
        }
    }

    #[test]
    fn freq_limit_defaults_to_none() {
        let toml = r#"
            [[stages]]
            type = "freq"
            field = "status"
        "#;
        let file = parse(toml).unwrap();
        assert!(matches!(
            file.stages[0],
            StageSpec::Freq { limit: None, .. }
        ));
    }

    #[test]
    fn parses_dedup_stage() {
        let toml = r#"
            [[stages]]
            type = "dedup"
        "#;
        let file = parse(toml).unwrap();
        assert!(matches!(file.stages[0], StageSpec::Dedup));
        assert!(matches!(into_command(StageSpec::Dedup), Command::Dedup));
    }

    #[test]
    fn parses_search_stage_with_regex_flag() {
        let toml = r#"
            [[stages]]
            type = "search"
            text = "foo"
            regex = true
        "#;
        let file = parse(toml).unwrap();
        match &file.stages[0] {
            StageSpec::Search { text, regex } => {
                assert_eq!(text, "foo");
                assert!(*regex);
            }
            _ => panic!("expected StageSpec::Search"),
        }
    }

    #[test]
    fn search_regex_defaults_to_false() {
        let toml = r#"
            [[stages]]
            type = "search"
            text = "foo"
        "#;
        let file = parse(toml).unwrap();
        assert!(matches!(
            file.stages[0],
            StageSpec::Search { regex: false, .. }
        ));
    }
}
