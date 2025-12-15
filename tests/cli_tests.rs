//! CLI integration tests using assert_cmd.
//!
//! These tests invoke the actual `acs` binary and verify its output.

use assert_cmd::Command;
use predicates::prelude::*;

#[allow(deprecated)]
fn acs_cmd() -> Command {
    Command::cargo_bin("acs").expect("binary should exist")
}

#[test]
fn test_version_flag() {
    acs_cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("0.1.0"));
}

#[test]
fn test_help_flag() {
    acs_cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Agent Cron Scheduler"))
        .stdout(predicate::str::contains("start"))
        .stdout(predicate::str::contains("add"))
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("remove"))
        .stdout(predicate::str::contains("enable"))
        .stdout(predicate::str::contains("disable"))
        .stdout(predicate::str::contains("trigger"))
        .stdout(predicate::str::contains("logs"))
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("stop"));
}

#[test]
fn test_add_help_shows_options() {
    acs_cmd()
        .args(["add", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--name"))
        .stdout(predicate::str::contains("--schedule"))
        .stdout(predicate::str::contains("--cmd"))
        .stdout(predicate::str::contains("--script"))
        .stdout(predicate::str::contains("--timezone"))
        .stdout(predicate::str::contains("--env"));
}

#[test]
fn test_start_help() {
    acs_cmd()
        .args(["start", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--foreground"))
        .stdout(predicate::str::contains("--config"));
}

#[test]
fn test_logs_help() {
    acs_cmd()
        .args(["logs", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--follow"))
        .stdout(predicate::str::contains("--tail"))
        .stdout(predicate::str::contains("--run"));
}

#[test]
fn test_no_subcommand_shows_help() {
    // When no subcommand is provided, should print help
    acs_cmd()
        .assert()
        .success()
        .stdout(predicate::str::contains("Agent Cron Scheduler"));
}
