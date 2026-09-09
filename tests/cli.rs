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
