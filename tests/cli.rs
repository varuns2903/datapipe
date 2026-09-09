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
