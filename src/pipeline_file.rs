use crate::cli::Command;
use crate::stages::JoinType;
use serde::Deserialize;

/// A declarative multi-stage pipeline definition, e.g.:
///
/// ```toml
/// in_csv = false
/// strict = false
/// out_csv = false
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
    Select {
        fields: Vec<String>,
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
    Group {
        by: String,
        #[serde(default)]
        sum: Option<String>,
        #[serde(default)]
        count: bool,
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
        StageSpec::Select { fields } => Command::Select { fields },
        StageSpec::Limit { max } => Command::Limit { max },
        StageSpec::Sort { field, desc } => Command::Sort { field, desc },
        StageSpec::Unique { field } => Command::Unique { field },
        StageSpec::Count => Command::Count,
        StageSpec::Sum { field } => Command::Sum { field },
        StageSpec::Avg { field } => Command::Avg { field },
        StageSpec::Min { field } => Command::Min { field },
        StageSpec::Max { field } => Command::Max { field },
        StageSpec::Schema => Command::Schema,
        StageSpec::Group { by, sum, count } => Command::Group { by, sum, count },
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
            stages = []
        "#;
        let file = parse(toml).unwrap();
        assert!(file.in_csv);
        assert!(file.strict);
        assert!(file.out_csv);
        assert_eq!(file.stages.len(), 0);
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
}
