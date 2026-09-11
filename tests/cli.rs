use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn test_help() {
    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("high-performance"));
}

#[test]
fn test_filter_command() {
    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("filter")
        .arg(".age > 25")
        .write_stdin("{\"name\": \"Varun\", \"age\": 30}\n{\"name\": \"Alice\", \"age\": 20}\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Varun"))
        .stdout(predicate::str::contains("Alice").not());
}

#[test]
fn test_missing_command() {
    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}

#[test]
fn test_malformed_line_is_skipped_by_default_and_processing_continues() {
    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("count")
        .write_stdin("{\"a\":1}\nnot json\n{\"a\":2}\n{\"a\":3}\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"count\":3"))
        .stderr(predicate::str::contains("skipping malformed record"));
}

#[test]
fn test_strict_mode_aborts_on_malformed_line() {
    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("--strict")
        .arg("count")
        .write_stdin("{\"a\":1}\nnot json\n{\"a\":2}\n")
        .assert()
        .failure();
}

#[test]
fn test_strict_mode_aborts_sort_on_malformed_line() {
    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("--strict")
        .arg("sort")
        .arg("a")
        .write_stdin("{\"a\":1}\nnot json\n{\"a\":2}\n")
        .assert()
        .failure();
}

#[test]
fn test_strict_mode_aborts_join_merge_on_malformed_line() {
    let dir = tempfile::tempdir().unwrap();
    let join_file = dir.path().join("bad.jsonl");
    std::fs::write(&join_file, "{\"id\":1,\"x\":\"a\"}\nnot json\n{\"id\":2,\"x\":\"b\"}\n").unwrap();

    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("--strict")
        .arg("join")
        .arg(join_file.to_str().unwrap())
        .arg("--on")
        .arg("id")
        .arg("--merge")
        .write_stdin("{\"id\":1}\n{\"id\":2}\n")
        .assert()
        .failure();
}

#[test]
fn test_completions_registers_actual_binary_name() {
    for shell in ["bash", "zsh", "fish", "powershell", "elvish"] {
        let mut cmd = Command::cargo_bin("dp").unwrap();
        cmd.arg("completions")
            .arg(shell)
            .assert()
            .success()
            // Must reference the real binary name "dp", not the crate/package
            // name "datapipe-cli" - otherwise the generated script wouldn't
            // actually provide completions for what a user types.
            .stdout(predicate::str::contains("dp"))
            .stdout(predicate::str::contains("datapipe-cli").not());
    }
}

#[test]
fn test_completions_does_not_read_stdin() {
    // Completions shouldn't require piping any record input - regression
    // test for the pipeline setup previously running unconditionally before
    // checking which subcommand was requested.
    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("completions").arg("bash").assert().success();
}

#[test]
fn test_man_page_uses_actual_binary_name() {
    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("man")
        .assert()
        .success()
        .stdout(predicate::str::contains(".TH dp 1"))
        .stdout(predicate::str::contains("datapipe-cli").not());
}

#[test]
fn test_man_page_lists_subcommands() {
    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("man")
        .assert()
        .success()
        .stdout(predicate::str::contains("dp\\-filter(1)"))
        .stdout(predicate::str::contains("dp\\-completions(1)"));
}

#[test]
fn test_man_page_does_not_read_stdin() {
    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("man").assert().success();
}

#[test]
fn test_join_type_full_appends_unmatched_right_records() {
    let dir = tempfile::tempdir().unwrap();
    let right_path = dir.path().join("right.jsonl");
    std::fs::write(
        &right_path,
        "{\"id\":\"1\",\"name\":\"Alice\"}\n{\"id\":\"2\",\"name\":\"Bob\"}\n",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("join")
        .arg(right_path.to_str().unwrap())
        .arg("--on")
        .arg("id")
        .arg("--type")
        .arg("full")
        .write_stdin("{\"id\":\"1\",\"order\":100}\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Alice"))
        .stdout(predicate::str::contains("Bob"));
}

#[test]
fn test_join_type_inner_drops_unmatched_left() {
    let dir = tempfile::tempdir().unwrap();
    let right_path = dir.path().join("right.jsonl");
    std::fs::write(&right_path, "{\"id\":\"1\",\"name\":\"Alice\"}\n").unwrap();

    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("join")
        .arg(right_path.to_str().unwrap())
        .arg("--on")
        .arg("id")
        .arg("--type")
        .arg("inner")
        .write_stdin("{\"id\":\"1\",\"order\":100}\n{\"id\":\"999\",\"order\":200}\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Alice"))
        .stdout(predicate::str::contains("999").not());
}

#[test]
fn test_rename_command() {
    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("rename")
        .arg("old_name:name")
        .write_stdin("{\"old_name\":\"Alice\"}\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\":\"Alice\""))
        .stdout(predicate::str::contains("old_name").not());
}

#[test]
fn test_flatten_command() {
    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("flatten")
        .write_stdin("{\"user\":{\"name\":\"Alice\"}}\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"user.name\":\"Alice\""));
}

#[test]
fn test_sample_command_respects_n() {
    let stdin: String = (0..50).map(|i| format!("{{\"n\":{}}}\n", i)).collect();
    let mut cmd = Command::cargo_bin("dp").unwrap();
    let output = cmd
        .arg("sample")
        .arg("5")
        .write_stdin(stdin)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let line_count = String::from_utf8(output).unwrap().lines().count();
    assert_eq!(line_count, 5);
}

#[test]
fn test_run_multi_stage_pipeline_file() {
    let dir = tempfile::tempdir().unwrap();
    let pipeline_path = dir.path().join("pipeline.toml");
    std::fs::write(
        &pipeline_path,
        r#"
[[stages]]
type = "filter"
expression = ".age >= 21"

[[stages]]
type = "sort"
field = "age"
desc = true
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("run")
        .arg(pipeline_path.to_str().unwrap())
        .write_stdin("{\"name\":\"Bob\",\"age\":19}\n{\"name\":\"Alice\",\"age\":30}\n{\"name\":\"Dave\",\"age\":25}\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Alice"))
        .stdout(predicate::str::contains("Dave"))
        .stdout(predicate::str::contains("Bob").not());
}

#[test]
fn test_run_respects_out_csv_setting() {
    let dir = tempfile::tempdir().unwrap();
    let pipeline_path = dir.path().join("pipeline.toml");
    std::fs::write(
        &pipeline_path,
        r#"
out_csv = true

[[stages]]
type = "select"
fields = ["name"]
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("run")
        .arg(pipeline_path.to_str().unwrap())
        .write_stdin("{\"name\":\"Alice\"}\n")
        .assert()
        .success()
        .stdout("name\nAlice\n");
}

#[test]
fn test_run_missing_file_fails() {
    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("run")
        .arg("/nonexistent/pipeline.toml")
        .write_stdin("")
        .assert()
        .failure();
}

#[test]
fn test_run_malformed_toml_fails() {
    let dir = tempfile::tempdir().unwrap();
    let pipeline_path = dir.path().join("bad.toml");
    std::fs::write(&pipeline_path, "not [ valid toml").unwrap();

    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("run")
        .arg(pipeline_path.to_str().unwrap())
        .write_stdin("")
        .assert()
        .failure();
}

#[test]
fn test_pretty_flag_indents_json_output() {
    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("--pretty")
        .arg("inspect")
        .write_stdin("{\"a\":1}\n")
        .assert()
        .success()
        .stdout("{\n  \"a\": 1\n}\n");
}

#[test]
fn test_pretty_short_flag() {
    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("-p")
        .arg("inspect")
        .write_stdin("{\"a\":1}\n")
        .assert()
        .success()
        .stdout("{\n  \"a\": 1\n}\n");
}

#[test]
fn test_default_output_stays_compact() {
    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("inspect")
        .write_stdin("{\"a\":1}\n")
        .assert()
        .success()
        .stdout("{\"a\":1}\n");
}

#[test]
fn test_select_exclude_drops_named_field() {
    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("select")
        .arg("password")
        .arg("--exclude")
        .write_stdin("{\"name\":\"Alice\",\"password\":\"secret\"}\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Alice"))
        .stdout(predicate::str::contains("password").not());
}

#[test]
fn test_table_command_aligns_output() {
    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("table")
        .write_stdin("{\"name\":\"Alice\",\"age\":30}\n{\"name\":\"Bo\",\"age\":9}\n")
        .assert()
        .success()
        .stdout("name   age\n-----  ---\nAlice  30\nBo     9\n");
}

#[test]
fn test_table_command_on_empty_stream_produces_no_output() {
    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("table")
        .write_stdin("")
        .assert()
        .success()
        .stdout("");
}

#[test]
fn test_stats_command_computes_mean_and_stddev() {
    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("stats")
        .write_stdin("{\"x\":2}\n{\"x\":4}\n{\"x\":4}\n{\"x\":4}\n{\"x\":5}\n{\"x\":5}\n{\"x\":7}\n{\"x\":9}\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"mean\":5.0"))
        .stdout(predicate::str::contains("\"stddev\":2.0"));
}

#[test]
fn test_freq_command_sorts_by_count_with_limit() {
    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("freq")
        .arg("status")
        .arg("--limit")
        .arg("1")
        .write_stdin("{\"status\":\"active\"}\n{\"status\":\"active\"}\n{\"status\":\"banned\"}\n")
        .assert()
        .success()
        .stdout("{\"value\":\"active\",\"count\":2,\"percent\":66.66666666666666}\n");
}

#[test]
fn test_dedup_drops_exact_duplicates_only() {
    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("dedup")
        .write_stdin("{\"a\":1,\"b\":2}\n{\"a\":1,\"b\":2}\n{\"a\":1,\"b\":3}\n")
        .assert()
        .success()
        .stdout("{\"a\":1,\"b\":2}\n{\"a\":1,\"b\":3}\n");
}

#[test]
fn test_search_matches_any_field() {
    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("search")
        .arg("foo")
        .write_stdin("{\"a\":\"foo\",\"b\":\"bar\"}\n{\"a\":\"baz\",\"b\":\"foo\"}\n{\"a\":\"x\",\"b\":\"y\"}\n")
        .assert()
        .success()
        .stdout("{\"a\":\"foo\",\"b\":\"bar\"}\n{\"a\":\"baz\",\"b\":\"foo\"}\n");
}

#[test]
fn test_search_regex_mode() {
    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("search")
        .arg("--regex")
        .arg(r"^.+@example\.com$")
        .write_stdin("{\"email\":\"alice@example.com\"}\n{\"email\":\"bob@other.org\"}\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("alice"))
        .stdout(predicate::str::contains("bob").not());
}

#[test]
fn test_search_invalid_regex_fails() {
    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("search")
        .arg("--regex")
        .arg("[unclosed")
        .write_stdin("{}\n")
        .assert()
        .failure();
}

#[test]
fn test_join_merge_matches_hash_join_for_unique_keys() {
    let dir = tempfile::tempdir().unwrap();
    let right_path = dir.path().join("right.jsonl");
    std::fs::write(
        &right_path,
        "{\"id\":\"1\",\"name\":\"Alice\"}\n{\"id\":\"2\",\"name\":\"Bob\"}\n",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("dp").unwrap();
    cmd.arg("join")
        .arg(right_path.to_str().unwrap())
        .arg("--on")
        .arg("id")
        .arg("--merge")
        .write_stdin("{\"id\":\"1\",\"order\":100}\n{\"id\":\"2\",\"order\":200}\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Alice"))
        .stdout(predicate::str::contains("Bob"));
}

#[test]
fn test_join_merge_produces_cross_product_for_duplicate_keys() {
    // The documented behavioral difference from the default hash join,
    // which keeps only the last duplicate-key record.
    let dir = tempfile::tempdir().unwrap();
    let right_path = dir.path().join("right.jsonl");
    std::fs::write(
        &right_path,
        "{\"id\":\"1\",\"tag\":\"a\"}\n{\"id\":\"1\",\"tag\":\"b\"}\n",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("dp").unwrap();
    let output = cmd
        .arg("join")
        .arg(right_path.to_str().unwrap())
        .arg("--on")
        .arg("id")
        .arg("--merge")
        .arg("--type")
        .arg("inner")
        .write_stdin("{\"id\":\"1\"}\n")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let line_count = String::from_utf8(output).unwrap().lines().count();
    assert_eq!(line_count, 2);
}
