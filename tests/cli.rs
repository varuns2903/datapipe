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
        .write_stdin("{\"name\": \"Varun\", \"age\": 30}\n{\"name\": \"Alice\", \"age\": 20}\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Varun"))
        .stdout(predicate::str::contains("Alice").not());
}

#[test]
fn test_missing_command() {
    let mut cmd = Command::cargo_bin("datapipe").unwrap();
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}
