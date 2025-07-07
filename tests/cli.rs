use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn test_no_args() {
    let mut cmd = Command::cargo_bin("time_cli").unwrap();
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("The current time is:"));
}

#[test]
fn test_statistics_arg() {
    let mut cmd = Command::cargo_bin("time_cli").unwrap();
    cmd.arg("--statistics");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Time statistics"));
}

#[test]
fn test_invalid_date() {
    let mut cmd = Command::cargo_bin("time_cli").unwrap();
    cmd.args(["history", "-m", "4", "-d", "31"]);
    cmd.assert().failure().stderr(predicate::str::contains(
        "'04-31' is not a valid calendar date",
    ));
}