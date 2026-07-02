//! m005_shell_claude_to_agent — rewrite legacy `shell` steps that wrap a
//! `claude -p ...` invocation as proper `agent` steps so the streaming cost
//! parser captures `cost_usd`.
//!
//! # Why this migration carries Rust logic
//!
//! The rewrite needs a shell tokenizer (double/single quotes, backslash
//! escapes), `-p` prompt extraction across three flag syntaxes, residual-flag
//! template reconstruction, and recursion into arbitrarily-nested `match`
//! step branches — none of which SQL alone can express. The body therefore
//! mixes SQL strings (via the framework's [`MigrationTx`] API) with
//! Rust-level JSON transformation.
//!
//! The logic is frozen by construction: it operates purely on
//! `serde_json::Value`, never on live model types.
//!
//! # Detection criterion
//!
//! A shell step is treated as shell-claude when ALL of the following hold:
//!   - `kind == "shell"`
//!   - the command (trimmed of leading whitespace) starts with the literal
//!     token `claude`, either followed by a space, or being exactly the
//!     word `claude`
//!   - the command contains the substring `--output-format stream-json`
//!
//! # Prompt extraction
//!
//! The prompt is extracted from the first `-p` flag's value. Supported
//! syntaxes: `-p "double-quoted"`, `-p 'single-quoted'`, `-p=value`.
//! Embedded escape sequences inside the quoted prompt are preserved verbatim
//! — de-quoted without de-escaping. Commands without a `-p` flag (stdin-fed
//! prompt) are skipped with a warning.
//!
//! # command_template
//!
//! If the command, after stripping the `-p` flag and its value, exactly
//! matches the agent default template's flag tail, `command_template` is
//! omitted (the agent step inherits the default). Otherwise the residual
//! flags are preserved in a full `command_template`.

use milepost::{MigrateError, Migration, MigrationTx, SqlValue};

use super::shell_tokens::{tokenize, truncate};

/// The exact flag tail that follows `-p "<prompt>"` in the default
/// `claude_code_cli` command template. When the residual flags of a migrated
/// step match this string the agent step inherits the default template.
const DEFAULT_TAIL: &str = "--output-format stream-json --verbose --dangerously-skip-permissions";

/// Marker substring required for a shell step to be considered shell-claude.
const STREAM_JSON_MARKER: &str = "--output-format stream-json";

pub(crate) struct ShellClaudeToAgent;

impl Migration for ShellClaudeToAgent {
    fn name(&self) -> &'static str {
        "m005_shell_claude_to_agent"
    }

    fn up(&self, tx: &MigrationTx<'_>) -> Result<(), MigrateError> {
        let rows = tx.query("SELECT id, steps_json FROM workflows", &[])?;

        let mut workflows_rewritten: usize = 0;
        let mut steps_rewritten: usize = 0;

        for row in rows {
            let workflow_id = row[0]
                .as_str()
                .ok_or_else(|| MigrateError::Db("workflows.id is not text".to_string()))?
                .to_string();
            let steps_json = row[1].as_str().ok_or_else(|| {
                MigrateError::Db(format!(
                    "workflows.steps_json is not text for workflow {}",
                    workflow_id
                ))
            })?;

            let mut value: serde_json::Value = serde_json::from_str(steps_json).map_err(|e| {
                MigrateError::Db(format!(
                    "Failed to parse steps_json for workflow {}: {}",
                    workflow_id, e
                ))
            })?;

            let rewritten = rewrite_steps_array(&mut value);
            if rewritten == 0 {
                continue;
            }

            let new_json = serde_json::to_string(&value).map_err(|e| {
                MigrateError::Db(format!(
                    "Failed to re-serialise steps_json for workflow {}: {}",
                    workflow_id, e
                ))
            })?;

            let now = chrono::Utc::now().to_rfc3339();
            tx.execute(
                "UPDATE workflows
                 SET steps_json = ?1,
                     version = version + 1,
                     updated_at = ?2
                 WHERE id = ?3",
                &[
                    SqlValue::from(new_json),
                    SqlValue::from(now),
                    SqlValue::from(workflow_id),
                ],
            )?;

            workflows_rewritten += 1;
            steps_rewritten += rewritten;
        }

        if workflows_rewritten > 0 {
            tracing::info!(
                "m005_shell_claude_to_agent: rewrote {} shell-claude steps across {} workflows",
                steps_rewritten,
                workflows_rewritten
            );
        }
        Ok(())
    }
}

/// Walk a `steps_json` array, rewriting any shell-claude steps in place and
/// recursing into `match` step branches. Returns the total number of step
/// objects rewritten across this array and its nested match branches.
fn rewrite_steps_array(value: &mut serde_json::Value) -> usize {
    let Some(items) = value.as_array_mut() else {
        return 0;
    };
    let mut count = 0;
    for item in items.iter_mut() {
        count += rewrite_step(item);
    }
    count
}

/// Rewrite a single step in place. Returns 1 if this step was rewritten as
/// an agent step; otherwise descends into `match` step branches and returns
/// the count of rewrites that happened down there.
fn rewrite_step(item: &mut serde_json::Value) -> usize {
    let Some(map) = item.as_object_mut() else {
        return 0;
    };

    let kind = map
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    match kind.as_str() {
        "shell" => {
            let command = match map.get("command").and_then(|v| v.as_str()) {
                Some(c) => c.to_string(),
                None => return 0,
            };
            if !is_shell_claude(&command) {
                return 0;
            }
            match parse_shell_claude(&command) {
                Some(parsed) => {
                    map.insert(
                        "kind".to_string(),
                        serde_json::Value::String("agent".to_string()),
                    );
                    map.remove("command");
                    map.remove("pass_stdin");

                    map.insert(
                        "agent_type".to_string(),
                        serde_json::Value::String("claude_code_cli".to_string()),
                    );
                    map.insert(
                        "prompt".to_string(),
                        serde_json::Value::String(parsed.prompt),
                    );
                    match parsed.command_template {
                        Some(tmpl) => {
                            map.insert(
                                "command_template".to_string(),
                                serde_json::Value::String(tmpl),
                            );
                        }
                        None => {
                            map.remove("command_template");
                        }
                    }
                    1
                }
                None => {
                    // Matched the criterion but had no `-p` flag — stdin-fed
                    // prompt. Skip with a warning so the operator can rewrite
                    // by hand if they want cost capture.
                    tracing::warn!(
                        "m005_shell_claude_to_agent: shell-claude step has no `-p` flag (stdin-fed prompt); leaving as shell. command={}",
                        truncate(&command, 200)
                    );
                    0
                }
            }
        }
        "match" => {
            let mut nested = 0;
            if let Some(cases_val) = map.get_mut("cases") {
                if let Some(cases_obj) = cases_val.as_object_mut() {
                    for (_k, branch) in cases_obj.iter_mut() {
                        nested += rewrite_steps_array(branch);
                    }
                }
            }
            if let Some(default_val) = map.get_mut("default") {
                nested += rewrite_steps_array(default_val);
            }
            nested
        }
        _ => 0,
    }
}

/// Quick screen: does `command` look like it invokes `claude -p ... stream-json`?
fn is_shell_claude(command: &str) -> bool {
    let trimmed = command.trim_start();
    let starts_with_claude =
        trimmed == "claude" || trimmed.starts_with("claude ") || trimmed.starts_with("claude\t");
    starts_with_claude && trimmed.contains(STREAM_JSON_MARKER)
}

/// Parser output for a successfully-detected shell-claude command.
struct ShellClaudeParts {
    /// The literal prompt string extracted from `-p "..."` / `-p '...'` /
    /// `-p=...`. Verbatim — escape sequences are not unescaped.
    prompt: String,
    /// `None` if the residual flags exactly match the default template's
    /// tail, otherwise `Some(template)` carrying the full agent command
    /// template with `${prompt}` substituted in for the original prompt
    /// argument.
    command_template: Option<String>,
}

/// Parse a shell-claude command, extracting the prompt and computing the
/// `command_template` value for the new agent step. Returns `None` when the
/// command does not carry a `-p` flag (stdin-fed prompt case).
fn parse_shell_claude(command: &str) -> Option<ShellClaudeParts> {
    let tokens = tokenize(command)?;

    // Locate the `-p` flag and its prompt value.
    let mut prompt: Option<String> = None;
    let mut p_index: Option<usize> = None;
    let mut p_value_index: Option<usize> = None;

    let mut i = 0;
    while i < tokens.len() {
        let tok = &tokens[i];
        if tok.unquoted == "-p" {
            // `-p <next-token>`
            if i + 1 < tokens.len() {
                prompt = Some(tokens[i + 1].unquoted.clone());
                p_index = Some(i);
                p_value_index = Some(i + 1);
            } else {
                // dangling `-p` with no value — treat as stdin-fed.
                return None;
            }
            break;
        } else if let Some(rest) = tok.unquoted.strip_prefix("-p=") {
            prompt = Some(rest.to_string());
            p_index = Some(i);
            p_value_index = None;
            break;
        }
        i += 1;
    }

    let prompt = prompt?;
    let p_index = p_index?;

    // Build the residual template by replacing `-p <value>` (or `-p=<value>`)
    // with `-p "${prompt}"`. Preserve every other token verbatim by emitting
    // its original raw slice.
    let mut residual: Vec<String> = Vec::with_capacity(tokens.len());
    for (idx, tok) in tokens.iter().enumerate() {
        if idx == p_index {
            residual.push("-p".to_string());
            residual.push("\"${prompt}\"".to_string());
            continue;
        }
        if Some(idx) == p_value_index {
            continue; // already emitted above
        }
        residual.push(tok.raw.clone());
    }
    let residual_template = residual.join(" ");

    // Compare against the canonical default template to decide whether to
    // keep `command_template` unset.
    let default_template = format!("claude -p \"${{prompt}}\" {}", DEFAULT_TAIL);
    let command_template = if residual_template == default_template {
        None
    } else {
        Some(residual_template)
    };

    Some(ShellClaudeParts {
        prompt,
        command_template,
    })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, Connection};

    // ── parse_shell_claude unit-level coverage ─────────────────────────────

    #[test]
    fn parse_default_command() {
        let cmd = r#"claude -p "say hi" --output-format stream-json --verbose --dangerously-skip-permissions"#;
        let parts = parse_shell_claude(cmd).expect("must parse");
        assert_eq!(parts.prompt, "say hi");
        assert!(
            parts.command_template.is_none(),
            "default tail must yield None"
        );
    }

    #[test]
    fn parse_extra_flag_keeps_template() {
        let cmd = r#"claude -p "do a thing" --output-format stream-json --verbose --dangerously-skip-permissions --model claude-opus-4-5"#;
        let parts = parse_shell_claude(cmd).expect("must parse");
        assert_eq!(parts.prompt, "do a thing");
        let tmpl = parts.command_template.expect("non-default → Some");
        assert!(tmpl.contains("--model claude-opus-4-5"), "tmpl={}", tmpl);
        assert!(tmpl.contains(r#"-p "${prompt}""#), "tmpl={}", tmpl);
    }

    #[test]
    fn parse_single_quoted_prompt() {
        let cmd = r#"claude -p 'singled prompt' --output-format stream-json --verbose --dangerously-skip-permissions"#;
        let parts = parse_shell_claude(cmd).expect("must parse");
        assert_eq!(parts.prompt, "singled prompt");
        assert!(parts.command_template.is_none());
    }

    #[test]
    fn parse_equals_form_prompt() {
        let cmd = r#"claude -p="inline prompt" --output-format stream-json --verbose --dangerously-skip-permissions"#;
        let parts = parse_shell_claude(cmd).expect("must parse");
        assert_eq!(parts.prompt, "inline prompt");
        assert!(
            parts.command_template.is_none(),
            "equals form normalises to the default tail"
        );
    }

    #[test]
    fn parse_no_dash_p_returns_none() {
        let cmd = r#"claude --output-format stream-json --verbose --dangerously-skip-permissions"#;
        assert!(parse_shell_claude(cmd).is_none());
    }

    #[test]
    fn parse_multiline_escape_preserved() {
        let cmd = r#"claude -p "line1\nline2\nline3" --output-format stream-json --verbose --dangerously-skip-permissions"#;
        let parts = parse_shell_claude(cmd).expect("must parse");
        assert_eq!(parts.prompt, r"line1\nline2\nline3");
    }

    #[test]
    fn tokenize_unterminated_quote_returns_none() {
        assert!(tokenize(r#"claude -p "oops"#).is_none());
        assert!(tokenize("claude -p 'oops").is_none());
    }

    #[test]
    fn is_shell_claude_rejects_non_claude() {
        assert!(!is_shell_claude("bash some-script.sh"));
        assert!(!is_shell_claude(
            "python script.py --output-format stream-json"
        ));
    }

    #[test]
    fn is_shell_claude_requires_stream_json() {
        assert!(!is_shell_claude(r#"claude -p "hi""#));
        assert!(is_shell_claude(
            r#"claude -p "hi" --output-format stream-json"#
        ));
    }

    // ── JSON-level rewrite coverage ─────────────────────────────────────────

    #[test]
    fn rewrite_recurses_into_match_branches() {
        let mut steps = serde_json::json!([
            {
                "id": "branch",
                "kind": "match",
                "expr": "${input.kind}",
                "cases": {
                    "review": [
                        {
                            "id": "nested",
                            "kind": "shell",
                            "command": r#"claude -p "review it" --output-format stream-json --verbose --dangerously-skip-permissions"#,
                            "pass_stdin": false
                        }
                    ]
                },
                "default": [
                    {
                        "id": "fallback",
                        "kind": "shell",
                        "command": r#"claude -p "default path" --output-format stream-json --verbose --dangerously-skip-permissions"#,
                        "pass_stdin": false
                    }
                ]
            }
        ]);
        let n = rewrite_steps_array(&mut steps);
        assert_eq!(n, 2, "both nested steps must be rewritten");
        assert_eq!(steps[0]["cases"]["review"][0]["kind"], "agent");
        assert_eq!(steps[0]["cases"]["review"][0]["prompt"], "review it");
        assert_eq!(steps[0]["default"][0]["kind"], "agent");
        assert_eq!(steps[0]["default"][0]["prompt"], "default path");
    }

    // ── Transaction-level tests ──────────────────────────────────────────────

    /// Minimal pre-m005-era workflows table for exercising `up`.
    fn setup_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch(
            "CREATE TABLE workflows (
                 id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE,
                 version INTEGER NOT NULL, schedule TEXT NOT NULL,
                 timezone TEXT, schedule_mode TEXT NOT NULL,
                 enabled INTEGER NOT NULL, steps_json TEXT NOT NULL,
                 default_input TEXT, working_dir TEXT, env_vars TEXT,
                 allow_concurrent INTEGER NOT NULL, on_failure TEXT NOT NULL,
                 last_run_at TEXT, last_run_status TEXT, last_run_id TEXT,
                 created_at TEXT NOT NULL, updated_at TEXT NOT NULL
             );",
        )
        .expect("schema");
        conn
    }

    fn insert_workflow(conn: &Connection, id: &str, steps_json: &str) {
        conn.execute(
            "INSERT INTO workflows (id, name, version, schedule, schedule_mode,
                 enabled, steps_json, allow_concurrent, on_failure, created_at, updated_at)
             VALUES (?1, ?2, 1, '* * * * *', 'Cron', 1, ?3, 1, '\"abort\"',
                 '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
            params![id, format!("wf-{}", id), steps_json],
        )
        .expect("insert workflow");
    }

    fn read_workflow(conn: &Connection, id: &str) -> (u32, String) {
        conn.query_row(
            "SELECT version, steps_json FROM workflows WHERE id = ?1",
            [id],
            |r| Ok((r.get::<_, u32>(0)?, r.get::<_, String>(1)?)),
        )
        .expect("query workflow")
    }

    #[test]
    fn up_rewrites_shell_claude_and_bumps_version() {
        let mut conn = setup_conn();
        let steps = serde_json::json!([
            {
                "id": "s0",
                "kind": "shell",
                "command": r#"claude -p "hello world" --output-format stream-json --verbose --dangerously-skip-permissions"#,
                "pass_stdin": false
            }
        ]);
        insert_workflow(&conn, "wf1", &steps.to_string());

        let tx = conn.transaction().expect("tx");
        ShellClaudeToAgent
            .up(&MigrationTx::new(&tx))
            .expect("migrate");
        tx.commit().expect("commit");

        let (version, steps_after) = read_workflow(&conn, "wf1");
        assert_eq!(version, 2, "version must be bumped to 2");
        let parsed: serde_json::Value = serde_json::from_str(&steps_after).unwrap();
        let s0 = &parsed[0];
        assert_eq!(s0["kind"], "agent");
        assert_eq!(s0["agent_type"], "claude_code_cli");
        assert_eq!(s0["prompt"], "hello world");
        assert!(s0.get("command_template").is_none());
        assert!(s0.get("command").is_none(), "command must be removed");
        assert!(s0.get("pass_stdin").is_none(), "pass_stdin must be removed");
    }

    #[test]
    fn up_leaves_non_claude_and_stdin_fed_steps_untouched() {
        let mut conn = setup_conn();
        let steps = serde_json::json!([
            { "id": "b", "kind": "shell", "command": "bash build.sh", "pass_stdin": false },
            {
                "id": "stdin_fed",
                "kind": "shell",
                "command": r#"claude --output-format stream-json --verbose --dangerously-skip-permissions"#,
                "pass_stdin": true
            }
        ]);
        insert_workflow(&conn, "wf1", &steps.to_string());

        let tx = conn.transaction().expect("tx");
        ShellClaudeToAgent
            .up(&MigrationTx::new(&tx))
            .expect("migrate");
        tx.commit().expect("commit");

        let (version, steps_after) = read_workflow(&conn, "wf1");
        assert_eq!(version, 1, "version must NOT be bumped");
        let parsed: serde_json::Value = serde_json::from_str(&steps_after).unwrap();
        assert_eq!(parsed[0]["kind"], "shell");
        assert_eq!(parsed[1]["kind"], "shell");
    }

    #[test]
    fn up_is_idempotent() {
        let mut conn = setup_conn();
        let steps = serde_json::json!([
            {
                "id": "s0",
                "kind": "shell",
                "command": r#"claude -p "hello" --output-format stream-json --verbose --dangerously-skip-permissions"#,
                "pass_stdin": false
            }
        ]);
        insert_workflow(&conn, "wf1", &steps.to_string());

        let tx = conn.transaction().expect("tx");
        ShellClaudeToAgent
            .up(&MigrationTx::new(&tx))
            .expect("first");
        tx.commit().expect("commit");
        let (_, after_first) = read_workflow(&conn, "wf1");

        let tx = conn.transaction().expect("tx");
        ShellClaudeToAgent
            .up(&MigrationTx::new(&tx))
            .expect("second");
        tx.commit().expect("commit");
        let (version, after_second) = read_workflow(&conn, "wf1");

        assert_eq!(after_first, after_second, "second run must be a no-op");
        assert_eq!(version, 2, "version bumped exactly once");
    }

    #[test]
    fn up_fails_on_corrupt_steps_json() {
        let mut conn = setup_conn();
        insert_workflow(&conn, "wf1", "not valid json {{{");

        let tx = conn.transaction().expect("tx");
        let err = ShellClaudeToAgent
            .up(&MigrationTx::new(&tx))
            .expect_err("corrupt JSON must fail the migration");
        assert!(err.to_string().contains("wf1"), "error must name the row");
    }
}
