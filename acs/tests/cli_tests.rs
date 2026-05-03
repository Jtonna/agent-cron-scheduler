//! CLI integration tests using assert_cmd.
//!
//! These tests invoke the actual `agentcronsystem` binary and verify its output.

use assert_cmd::Command;
use predicates::prelude::*;

#[allow(deprecated)]
fn agentcronsystem_cmd() -> Command {
    Command::cargo_bin("agentcronsystem").expect("binary should exist")
}

#[test]
fn test_version_flag() {
    agentcronsystem_cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn test_help_flag() {
    agentcronsystem_cmd()
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
    agentcronsystem_cmd()
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
    agentcronsystem_cmd()
        .args(["start", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--foreground"))
        .stdout(predicate::str::contains("--config"));
}

#[test]
fn test_logs_help() {
    agentcronsystem_cmd()
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
    agentcronsystem_cmd()
        .assert()
        .success()
        .stdout(predicate::str::contains("Agent Cron Scheduler"));
}

// ===========================================================================
// ACS-18 Stage C: workflows subcommand integration tests
// ===========================================================================

/// `acs workflows list --json` is accepted by clap (exit 0, help output includes flag)
#[test]
fn test_cli_workflows_list_parses() {
    agentcronsystem_cmd()
        .args(["workflows", "list", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--json"))
        .stdout(predicate::str::contains("--enabled"))
        .stdout(predicate::str::contains("--disabled"));
}

/// `--file` and `--json` are mutually exclusive for `workflows create`
#[test]
fn test_cli_workflows_create_with_file_and_inline_conflict() {
    agentcronsystem_cmd()
        .args([
            "workflows",
            "create",
            "--file",
            "/tmp/wf.json",
            "--json",
            r#"{"name":"test"}"#,
        ])
        .assert()
        .failure();
}

/// `acs workflows trigger --input '...' --env KEY=VALUE` parses correctly
#[test]
fn test_cli_workflows_trigger_with_input_and_env() {
    agentcronsystem_cmd()
        .args([
            "workflows",
            "trigger",
            "my-workflow",
            "--input",
            r#"{"prompt":"hello"}"#,
            "--env",
            "MODEL=claude",
            "--help",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("--input"))
        .stdout(predicate::str::contains("--env"))
        .stdout(predicate::str::contains("--follow"));
}

/// `--enable` and `--disable` are mutually exclusive for `workflows update`
#[test]
fn test_cli_workflows_update_with_enable_and_disable_conflict() {
    agentcronsystem_cmd()
        .args([
            "workflows",
            "update",
            "my-workflow",
            "--enable",
            "--disable",
        ])
        .assert()
        .failure();
}
