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
    // Phase 6: old job CLI commands removed; only daemon + workflow commands remain
    agentcronsystem_cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Agent Cron Scheduler"))
        .stdout(predicate::str::contains("start"))
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("stop"))
        .stdout(predicate::str::contains("workflows"));
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

// test_logs_help removed in Phase 6: `acs logs` command was deleted along with
// the old job-based CLI surface. Log viewing is now done via workflow run logs.

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

/// `acs workflows runs <id> --limit 10 --offset 5 --json` parses correctly.
#[test]
fn test_cli_workflows_runs_arg_parsing() {
    agentcronsystem_cmd()
        .args(["workflows", "runs", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--limit"))
        .stdout(predicate::str::contains("--offset"))
        .stdout(predicate::str::contains("--json"))
        .stdout(predicate::str::contains("NAME_OR_ID"));
}
