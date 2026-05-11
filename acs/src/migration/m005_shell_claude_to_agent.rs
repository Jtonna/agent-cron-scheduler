//! Migration 005 — rewrite legacy `shell` steps that wrap a `claude -p ...`
//! invocation as proper `agent` steps so the streaming cost parser captures
//! `cost_usd`.
//!
//! # Background
//!
//! The streaming NDJSON cost parser (see
//! [`crate::workflow::agents::claude_code_cli::ClaudeStreamParser`]) only runs
//! on `AgentStep`. Workflows authored against the older `shell` step kind
//! that invoke `claude -p "<prompt>" --output-format stream-json …` execute
//! correctly but their `cost_usd` is always null because the parser never
//! sees the stream. m005 walks the `workflows` table, detects shell steps
//! that match the shell-claude pattern, and rewrites them in place as
//! `agent` steps of type `claude_code_cli`.
//!
//! # Detection criterion
//!
//! A shell step is treated as shell-claude when ALL of the following hold:
//!   - `kind == "shell"`
//!   - the command (trimmed of leading whitespace) starts with the literal
//!     token `claude`, either followed by a space, or being exactly the
//!     word `claude`
//!   - the command contains the substring `--output-format stream-json`
//!     (the parsing hook used by the cost parser)
//!
//! # Prompt extraction
//!
//! The prompt is extracted from the first `-p` flag's value. Supported
//! syntaxes:
//!   - `-p "double-quoted"` (most common)
//!   - `-p 'single-quoted'`
//!   - `-p=value` / `-p="value"` / `-p='value'`
//!
//! Embedded `\n`, `\t`, escaped quotes, and `\\` sequences inside the quoted
//! prompt are preserved verbatim — we de-quote without de-escaping. This
//! matches what the runtime would have passed to `claude` on the shell.
//!
//! If the command has no `-p` flag, the prompt is read from stdin in the
//! legacy shell-claude pattern. Migrating those automatically would lose
//! the stdin source, so we emit a `tracing::warn!` and SKIP that step
//! (leave it as `shell`).
//!
//! # command_template
//!
//! If the command, after stripping the `-p` flag and its value, exactly
//! matches the agent default template's flag tail
//! (`--output-format stream-json --verbose --dangerously-skip-permissions`)
//! then `command_template` is set to `None` (the agent step inherits the
//! default). Otherwise the residual flags are preserved by emitting a full
//! `command_template: Some(...)` so callers retain any custom flags
//! (`--model`, `--session-id`, `-c`, etc.).
//!
//! # Recursion into MatchStep
//!
//! `MatchStep` carries `cases.<value>: [StepDef]` and an optional `default:
//! [StepDef]`. Both arrays are walked recursively — claude-shell steps
//! nested inside a match branch are migrated the same way as top-level
//! steps. Nested `MatchStep`s recurse further.
//!
//! # Idempotency
//!
//! Re-running m005 on a workflow whose shell-claude steps were already
//! rewritten is a no-op: the detection criterion no longer matches (the
//! step is now `agent`, not `shell`). Returns `Ok(false)` when no rows
//! were modified, `Ok(true)` when at least one workflow was rewritten.

use std::path::Path;

use async_trait::async_trait;
use rusqlite::params;

use crate::errors::AcsError;
use crate::migration::Migration;
use crate::storage::sqlite;

/// The exact flag tail that follows `-p "<prompt>"` in the default
/// `claude_code_cli` command template. When the residual flags of a
/// migrated step match this string the agent step inherits the default
/// template (we leave `command_template = None`).
const DEFAULT_TAIL: &str = "--output-format stream-json --verbose --dangerously-skip-permissions";

/// Marker substring required for a shell step to be considered shell-claude.
/// This is also the cost-parser hook — any command without this flag would
/// not produce streaming NDJSON, so there is nothing for the parser to read.
const STREAM_JSON_MARKER: &str = "--output-format stream-json";

pub struct ShellClaudeToAgent;

#[async_trait]
impl Migration for ShellClaudeToAgent {
    fn name(&self) -> &'static str {
        "m005_shell_claude_to_agent"
    }

    async fn run(&self, data_dir: &Path) -> Result<bool, AcsError> {
        let db_path = data_dir.join("acs.db");
        if !db_path.exists() {
            return Ok(false);
        }
        let db_path_clone = db_path.clone();
        tokio::task::spawn_blocking(move || run_blocking(&db_path_clone))
            .await
            .map_err(|e| AcsError::Internal(format!("blocking task failed: {}", e)))?
    }
}

fn run_blocking(db_path: &Path) -> Result<bool, AcsError> {
    let mut conn = sqlite::open_with_schema(db_path)?;

    let tx = conn
        .transaction()
        .map_err(|e| AcsError::Storage(format!("Failed to start transaction: {}", e)))?;

    let rows: Vec<(String, String)> = {
        let mut stmt = tx
            .prepare("SELECT id, steps_json FROM workflows")
            .map_err(|e| AcsError::Storage(format!("Failed to prepare SELECT: {}", e)))?;
        let mapped = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| AcsError::Storage(format!("Failed to execute SELECT: {}", e)))?;
        let mut out = Vec::new();
        for r in mapped {
            out.push(r.map_err(|e| AcsError::Storage(format!("Row read failed: {}", e)))?);
        }
        out
    };

    let result: Result<(usize, usize), AcsError> = (|| {
        let mut workflows_rewritten: usize = 0;
        let mut steps_rewritten: usize = 0;

        for (workflow_id, steps_json) in rows {
            let mut value: serde_json::Value = serde_json::from_str(&steps_json).map_err(|e| {
                AcsError::Storage(format!(
                    "Failed to parse steps_json for workflow {}: {}",
                    workflow_id, e
                ))
            })?;

            let rewritten = rewrite_steps_array(&mut value);
            if rewritten == 0 {
                continue;
            }

            let new_json = serde_json::to_string(&value).map_err(|e| {
                AcsError::Storage(format!(
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
                params![new_json, now, workflow_id],
            )
            .map_err(|e| {
                AcsError::Storage(format!("UPDATE for workflow {} failed: {}", workflow_id, e))
            })?;

            workflows_rewritten += 1;
            steps_rewritten += rewritten;
        }
        Ok((workflows_rewritten, steps_rewritten))
    })();

    let (workflows_rewritten, steps_rewritten) = match result {
        Ok(v) => v,
        Err(e) => {
            if let Err(rb) = tx.rollback() {
                tracing::warn!(
                    "m005_shell_claude_to_agent: rollback after error also failed: {}",
                    rb
                );
            }
            return Err(e);
        }
    };

    if workflows_rewritten == 0 {
        if let Err(e) = tx.rollback() {
            tracing::warn!(
                "m005_shell_claude_to_agent: rollback of read-only transaction failed: {}",
                e
            );
        }
        return Ok(false);
    }

    tx.commit()
        .map_err(|e| AcsError::Storage(format!("COMMIT failed: {}", e)))?;

    tracing::info!(
        "Migration m005 complete: rewrote {} shell-claude steps across {} workflows",
        steps_rewritten,
        workflows_rewritten
    );
    Ok(true)
}

/// Walk a `steps_json` array, rewriting any shell-claude steps in place and
/// recursing into `MatchStep` branches. Returns the total number of step
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
/// an agent step; otherwise descends into `MatchStep` branches and returns
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
                    // Mutate the step object: change kind + replace
                    // shell-specific fields with agent fields.
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
            // Recurse into cases.<value> arrays and default array.
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
/// `command_template` value for the new agent step.
///
/// Signature: `fn parse_shell_claude(command: &str) -> Option<ShellClaudeParts>`.
///
/// Returns `None` when the command does not carry a `-p` flag (stdin-fed
/// prompt case — caller skips with a warning).
fn parse_shell_claude(command: &str) -> Option<ShellClaudeParts> {
    // We need to split the command into shell-style tokens so we can find
    // the `-p` flag and replace its value with `${prompt}` when building the
    // residual template. The token splitter here is intentionally minimal —
    // it handles double-quoted, single-quoted, and bare tokens, which covers
    // every shell-claude command we have observed in migrated workflows.
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
        } else if tok.unquoted == "claude" {
            // skip — also matches first token
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
            // Emit `-p "${prompt}"` regardless of which form was used.
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

    // Compare against the canonical default template
    // `claude -p "${prompt}" <DEFAULT_TAIL>` to decide whether to keep
    // `command_template` as `None`.
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

/// A single shell token captured during parsing.
#[derive(Debug, Clone)]
struct Token {
    /// The raw token slice from the input, including any quote characters.
    /// Used to rebuild the command_template so non-`-p` tokens round-trip
    /// byte-for-byte.
    raw: String,
    /// The token with surrounding quotes stripped and `-p=...` left intact.
    /// Used for flag detection and for extracting the prompt value.
    unquoted: String,
}

/// Split a command into shell-style tokens. Recognises double-quoted,
/// single-quoted, and bare tokens. Returns `None` if the input has an
/// unterminated quoted string — those commands are malformed and we leave
/// the migration to fail-closed on them (caller treats this as "do not
/// migrate" because `parse_shell_claude` itself returns `None`).
fn tokenize(s: &str) -> Option<Vec<Token>> {
    let bytes = s.as_bytes();
    let mut tokens: Vec<Token> = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        // Skip whitespace between tokens.
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }

        let start = i;
        let mut unquoted = String::new();

        // Parse one token, which may be composed of multiple adjacent
        // quoted / bare runs (e.g. `prefix"quoted"suffix` collapses into a
        // single token). This mirrors POSIX-shell behaviour closely enough
        // for the commands m005 is asked to migrate.
        while i < bytes.len() && bytes[i] != b' ' && bytes[i] != b'\t' {
            match bytes[i] {
                b'"' => {
                    i += 1;
                    while i < bytes.len() && bytes[i] != b'"' {
                        if bytes[i] == b'\\' && i + 1 < bytes.len() {
                            // Preserve the escape byte AND the next byte
                            // into the unquoted string. The runtime never
                            // unescapes either, so neither does m005.
                            unquoted.push(bytes[i] as char);
                            unquoted.push(bytes[i + 1] as char);
                            i += 2;
                            continue;
                        }
                        unquoted.push(bytes[i] as char);
                        i += 1;
                    }
                    if i >= bytes.len() {
                        // Unterminated `"` — give up.
                        return None;
                    }
                    i += 1; // skip closing "
                }
                b'\'' => {
                    i += 1;
                    while i < bytes.len() && bytes[i] != b'\'' {
                        unquoted.push(bytes[i] as char);
                        i += 1;
                    }
                    if i >= bytes.len() {
                        return None;
                    }
                    i += 1;
                }
                _ => {
                    unquoted.push(bytes[i] as char);
                    i += 1;
                }
            }
        }

        let raw = s[start..i].to_string();
        tokens.push(Token { raw, unquoted });
    }

    Some(tokens)
}

/// Truncate a string to at most `max` bytes for log-output safety.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut out = s[..max].to_string();
    out.push('…');
    out
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use tempfile::TempDir;
    use uuid::Uuid;

    /// Insert a workflow row whose `steps_json` is the given literal.
    fn insert_workflow(conn: &rusqlite::Connection, id: Uuid, steps_json: &str) {
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO workflows (
                id, name, version, schedule, timezone, schedule_mode, enabled,
                steps_json, default_input, working_dir, env_vars,
                allow_concurrent, on_failure, last_run_at, last_run_status,
                last_run_id, created_at, updated_at
            ) VALUES (
                ?1, ?2, 1, '* * * * *', NULL, 'Cron', 1,
                ?3, NULL, NULL, NULL,
                1, '\"abort\"', NULL, NULL,
                NULL, ?4, ?4
            )",
            params![id.to_string(), format!("wf-{}", id), steps_json, now],
        )
        .expect("insert workflow");
    }

    fn read_workflow(db_path: &Path, id: Uuid) -> (u32, String) {
        let conn = sqlite::open_with_schema(db_path).expect("open");
        conn.query_row(
            "SELECT version, steps_json FROM workflows WHERE id = ?1",
            [id.to_string()],
            |r| Ok((r.get::<_, u32>(0)?, r.get::<_, String>(1)?)),
        )
        .expect("query workflow")
    }

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

    // ── DB-level tests ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_migrate_simple_claude_shell_step() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        let wf_id = Uuid::now_v7();
        {
            let conn = sqlite::open_with_schema(&db_path).expect("open");
            let steps = serde_json::json!([
                {
                    "id": "s0",
                    "kind": "shell",
                    "command": r#"claude -p "hello world" --output-format stream-json --verbose --dangerously-skip-permissions"#,
                    "pass_stdin": false
                }
            ]);
            insert_workflow(&conn, wf_id, &steps.to_string());
        }

        let m = ShellClaudeToAgent;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(did_work, "expected Ok(true)");

        let (version, steps_after) = read_workflow(&db_path, wf_id);
        assert_eq!(version, 2, "version must be bumped to 2");

        let parsed: serde_json::Value = serde_json::from_str(&steps_after).unwrap();
        let s0 = &parsed[0];
        assert_eq!(s0["kind"], "agent");
        assert_eq!(s0["agent_type"], "claude_code_cli");
        assert_eq!(s0["prompt"], "hello world");
        assert!(
            s0.get("command_template").is_none(),
            "default tail → no command_template field; got {}",
            s0
        );
        assert!(s0.get("command").is_none(), "command must be removed");
    }

    #[tokio::test]
    async fn test_migrate_preserves_extra_flags() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        let wf_id = Uuid::now_v7();
        {
            let conn = sqlite::open_with_schema(&db_path).expect("open");
            let steps = serde_json::json!([
                {
                    "id": "s0",
                    "kind": "shell",
                    "command": r#"claude -p "review" --output-format stream-json --verbose --dangerously-skip-permissions --model claude-opus-4-5"#,
                    "pass_stdin": false
                }
            ]);
            insert_workflow(&conn, wf_id, &steps.to_string());
        }

        let m = ShellClaudeToAgent;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(did_work);

        let (_v, steps_after) = read_workflow(&db_path, wf_id);
        let parsed: serde_json::Value = serde_json::from_str(&steps_after).unwrap();
        let s0 = &parsed[0];
        assert_eq!(s0["kind"], "agent");
        let tmpl = s0["command_template"]
            .as_str()
            .expect("command_template must be Some for extra flags");
        assert!(tmpl.contains("--model claude-opus-4-5"), "tmpl={}", tmpl);
    }

    #[tokio::test]
    async fn test_skip_shell_step_without_dash_p() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        let wf_id = Uuid::now_v7();
        {
            let conn = sqlite::open_with_schema(&db_path).expect("open");
            let steps = serde_json::json!([
                {
                    "id": "s0",
                    "kind": "shell",
                    "command": r#"claude --output-format stream-json --verbose --dangerously-skip-permissions"#,
                    "pass_stdin": true
                }
            ]);
            insert_workflow(&conn, wf_id, &steps.to_string());
        }

        let m = ShellClaudeToAgent;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(
            !did_work,
            "no-`-p` step must be skipped; expected Ok(false)"
        );

        let (version, steps_after) = read_workflow(&db_path, wf_id);
        assert_eq!(version, 1, "version must NOT be bumped");
        let parsed: serde_json::Value = serde_json::from_str(&steps_after).unwrap();
        assert_eq!(parsed[0]["kind"], "shell", "step kind must be unchanged");
    }

    #[tokio::test]
    async fn test_leave_non_claude_shell_untouched() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        let wf_id = Uuid::now_v7();
        {
            let conn = sqlite::open_with_schema(&db_path).expect("open");
            let steps = serde_json::json!([
                {
                    "id": "s0",
                    "kind": "shell",
                    "command": "bash some-script.sh",
                    "pass_stdin": false
                }
            ]);
            insert_workflow(&conn, wf_id, &steps.to_string());
        }

        let m = ShellClaudeToAgent;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(!did_work);
        let (version, steps_after) = read_workflow(&db_path, wf_id);
        assert_eq!(version, 1);
        let parsed: serde_json::Value = serde_json::from_str(&steps_after).unwrap();
        assert_eq!(parsed[0]["kind"], "shell");
        assert_eq!(parsed[0]["command"], "bash some-script.sh");
    }

    #[tokio::test]
    async fn test_migrate_mixed_workflow() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        let wf_id = Uuid::now_v7();
        {
            let conn = sqlite::open_with_schema(&db_path).expect("open");
            let steps = serde_json::json!([
                {
                    "id": "build",
                    "kind": "shell",
                    "command": "bash build.sh",
                    "pass_stdin": false
                },
                {
                    "id": "agent",
                    "kind": "shell",
                    "command": r#"claude -p "summarise" --output-format stream-json --verbose --dangerously-skip-permissions"#,
                    "pass_stdin": false
                }
            ]);
            insert_workflow(&conn, wf_id, &steps.to_string());
        }

        let m = ShellClaudeToAgent;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(did_work);

        let (version, steps_after) = read_workflow(&db_path, wf_id);
        assert_eq!(version, 2);
        let parsed: serde_json::Value = serde_json::from_str(&steps_after).unwrap();
        // bash step preserved
        assert_eq!(parsed[0]["kind"], "shell");
        assert_eq!(parsed[0]["command"], "bash build.sh");
        // claude step rewritten
        assert_eq!(parsed[1]["kind"], "agent");
        assert_eq!(parsed[1]["prompt"], "summarise");
    }

    #[tokio::test]
    async fn test_idempotent_on_already_migrated() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        let wf_id = Uuid::now_v7();
        {
            let conn = sqlite::open_with_schema(&db_path).expect("open");
            let steps = serde_json::json!([
                {
                    "id": "s0",
                    "kind": "shell",
                    "command": r#"claude -p "hello" --output-format stream-json --verbose --dangerously-skip-permissions"#,
                    "pass_stdin": false
                }
            ]);
            insert_workflow(&conn, wf_id, &steps.to_string());
        }

        let m = ShellClaudeToAgent;
        let first = m.run(tmp.path()).await.expect("first");
        assert!(first, "first run must rewrite");
        let second = m.run(tmp.path()).await.expect("second");
        assert!(!second, "second run must be a no-op");
    }

    #[tokio::test]
    async fn test_migrate_match_step_children() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        let wf_id = Uuid::now_v7();
        {
            let conn = sqlite::open_with_schema(&db_path).expect("open");
            let steps = serde_json::json!([
                {
                    "id": "branch",
                    "kind": "match",
                    "expr": "${input.kind}",
                    "cases": {
                        "review": [
                            {
                                "id": "do-review",
                                "kind": "shell",
                                "command": r#"claude -p "review the diff" --output-format stream-json --verbose --dangerously-skip-permissions"#,
                                "pass_stdin": false
                            }
                        ]
                    },
                    "default": [
                        {
                            "id": "noop",
                            "kind": "shell",
                            "command": "echo nothing to do",
                            "pass_stdin": false
                        }
                    ]
                }
            ]);
            insert_workflow(&conn, wf_id, &steps.to_string());
        }

        let m = ShellClaudeToAgent;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(did_work);

        let (version, steps_after) = read_workflow(&db_path, wf_id);
        assert_eq!(version, 2);
        let parsed: serde_json::Value = serde_json::from_str(&steps_after).unwrap();
        let branch = &parsed[0]["cases"]["review"][0];
        assert_eq!(branch["kind"], "agent");
        assert_eq!(branch["prompt"], "review the diff");

        let default_step = &parsed[0]["default"][0];
        assert_eq!(
            default_step["kind"], "shell",
            "non-claude default left as-is"
        );
    }

    // ── Fresh-install: no acs.db means no-op ───────────────────────────────
    #[tokio::test]
    async fn test_no_db_file_returns_false() {
        let tmp = TempDir::new().unwrap();
        let m = ShellClaudeToAgent;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(!did_work);
    }
}
