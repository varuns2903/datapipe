use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn test_help() {
    let mut cmd = Command::cargo_bin("datapipe").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("high-performance"));
}

#[test]
fn test_filter_command() {
    let mut cmd = Command::cargo_bin("datapipe").unwrap();
    cmd.arg("filter")
        .arg(".age > 25")
        .assert()
        .success()
        .stdout(predicate::str::contains("Filtering with expression: .age > 25"));
}

#[test]
fn test_missing_command() {
    let mut cmd = Command::cargo_bin("datapipe").unwrap();
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}
