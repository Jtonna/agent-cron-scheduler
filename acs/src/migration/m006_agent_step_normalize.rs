//! Migration 006 — normalize `AgentStep` records that still carry a
//! `command_template` field (written by v4.2.6 and earlier).
//!
//! # Background
//!
//! In v4.2.6 `AgentStep` stored CLI customisation as a single
//! `command_template` string (e.g.
//! `"claude --model X -p \"${prompt}\" --output-format stream-json …"`).
//! v4.2.7 replaces that field with two structured fields:
//!   - `model`       — the `--model <value>` override, or absent if default
//!   - `extra_args`  — any remaining non-standard flags as a JSON array
//!
//! This migration walks the `workflows` table, finds every `agent` step that
//! still has a `command_template` key, parses the template, extracts `model`
//! and `extra_args`, drops `command_template`, and writes the row back.
//!
//! # Detection + parsing
//!
//! For each step where `kind == "agent"` AND `command_template` is present:
//!   1. Tokenize the template string (same minimal shell tokenizer as m005).
//!   2. If the first token is not `claude`, emit a `tracing::warn!` and leave
//!      the step unchanged (conservative: we don't touch commands we don't
//!      recognise).
//!   3. If tokenisation fails (unterminated quote), emit a `tracing::warn!`
//!      and leave the step unchanged.
//!   4. Otherwise extract:
//!      - `--model <value>` or `--model=<value>` → `model` field
//!      - Remove baseline tokens: first `claude` token, `-p` + its value,
//!        `--output-format stream-json`, `--verbose`,
//!        `--dangerously-skip-permissions`
//!      - Whatever tokens remain → `extra_args` array (empty `[]` if none)
//!      - Drop `command_template`.
//!
//! # Recursion into MatchStep
//!
//! Same pattern as m005: recurses into `cases.<value>` and `default` arrays.
//!
//! # Idempotency
//!
//! After m006 runs, no `agent` step has `command_template`. Re-running sees
//! nothing to do and returns `Ok(false)`.

use std::path::Path;

use async_trait::async_trait;
use rusqlite::params;

use crate::errors::AcsError;
use crate::migration::Migration;
use crate::storage::sqlite;

pub struct AgentStepNormalize;

#[async_trait]
impl Migration for AgentStepNormalize {
    fn name(&self) -> &'static str {
        "m006_agent_step_normalize"
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

    let rows: Vec<(String, String, u32)> = {
        let mut stmt = tx
            .prepare("SELECT id, steps_json, version FROM workflows")
            .map_err(|e| AcsError::Storage(format!("Failed to prepare SELECT: {}", e)))?;
        let mapped = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, u32>(2)?,
                ))
            })
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

        for (workflow_id, steps_json, _version) in rows {
            let mut value: serde_json::Value = serde_json::from_str(&steps_json).map_err(|e| {
                AcsError::Storage(format!(
                    "Failed to parse steps_json for workflow {}: {}",
                    workflow_id, e
                ))
            })?;

            let rewritten = normalize_steps_array(&mut value);
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
                    "m006_agent_step_normalize: rollback after error also failed: {}",
                    rb
                );
            }
            return Err(e);
        }
    };

    if workflows_rewritten == 0 {
        if let Err(e) = tx.rollback() {
            tracing::warn!(
                "m006_agent_step_normalize: rollback of read-only transaction failed: {}",
                e
            );
        }
        return Ok(false);
    }

    tx.commit()
        .map_err(|e| AcsError::Storage(format!("COMMIT failed: {}", e)))?;

    tracing::info!(
        "Migration m006 complete: normalized {} agent steps across {} workflows",
        steps_rewritten,
        workflows_rewritten
    );
    Ok(true)
}

/// Walk a `steps_json` array, normalizing any agent steps that carry
/// `command_template`. Recurses into MatchStep branches. Returns the total
/// number of steps rewritten.
fn normalize_steps_array(value: &mut serde_json::Value) -> usize {
    let Some(items) = value.as_array_mut() else {
        return 0;
    };
    let mut count = 0;
    for item in items.iter_mut() {
        count += normalize_step(item);
    }
    count
}

/// Normalize a single step in place. Returns 1 if this step was rewritten,
/// 0 if skipped. Descends into MatchStep branches.
fn normalize_step(item: &mut serde_json::Value) -> usize {
    let Some(map) = item.as_object_mut() else {
        return 0;
    };

    let kind = map
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    match kind.as_str() {
        "agent" => {
            let template = match map.get("command_template").and_then(|v| v.as_str()) {
                Some(t) => t.to_string(),
                None => return 0, // no command_template — already normalized
            };

            match parse_command_template(&template) {
                ParseResult::Parsed { model, extra_args } => {
                    // Remove command_template
                    map.remove("command_template");

                    // Set model field only when extracted from command_template.
                    // If no --model flag was present in the template, leave any
                    // pre-existing `model` field untouched (defensive: don't
                    // destroy a value that arrived via another path).
                    if let Some(m) = model {
                        map.insert("model".to_string(), serde_json::Value::String(m));
                    }

                    // Always set extra_args (even empty array)
                    let args_json: serde_json::Value = extra_args
                        .into_iter()
                        .map(serde_json::Value::String)
                        .collect::<Vec<_>>()
                        .into();
                    map.insert("extra_args".to_string(), args_json);

                    1
                }
                ParseResult::Warn(msg) => {
                    tracing::warn!(
                        "m006_agent_step_normalize: leaving step unchanged — {}; template={}",
                        msg,
                        truncate(&template, 200)
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
                        nested += normalize_steps_array(branch);
                    }
                }
            }
            if let Some(default_val) = map.get_mut("default") {
                nested += normalize_steps_array(default_val);
            }
            nested
        }
        _ => 0,
    }
}

/// Result of attempting to parse a `command_template` string.
enum ParseResult {
    /// Successfully parsed: extracted model override and leftover args.
    Parsed {
        model: Option<String>,
        extra_args: Vec<String>,
    },
    /// Could not parse — emit this message as a warning and leave unchanged.
    Warn(String),
}

/// Parse a `command_template` string, extracting `model` and `extra_args`.
///
/// Canonical baseline tokens that are stripped (in any order after the first
/// token):
///   - `claude` (first token only)
///   - `-p <value>` or `-p=<value>` (the prompt placeholder)
///   - `--output-format stream-json`
///   - `--verbose`
///   - `--dangerously-skip-permissions`
///
/// `--model <value>` / `--model=<value>` is extracted into `model`.
/// Anything else is collected into `extra_args`.
fn parse_command_template(template: &str) -> ParseResult {
    let tokens = match tokenize(template) {
        Some(t) => t,
        None => {
            return ParseResult::Warn("unterminated quote in command_template".to_string());
        }
    };

    if tokens.is_empty() {
        return ParseResult::Warn("empty command_template".to_string());
    }

    // First token must be `claude`
    if tokens[0].unquoted != "claude" {
        return ParseResult::Warn(format!(
            "first token is not `claude` (got `{}`)",
            truncate(&tokens[0].unquoted, 80)
        ));
    }

    let mut model: Option<String> = None;
    let mut extra_args: Vec<String> = Vec::new();

    let mut i = 1; // skip `claude`
    while i < tokens.len() {
        let tok = &tokens[i];

        // --model <value> (space form)
        if tok.unquoted == "--model" {
            if i + 1 < tokens.len() {
                model = Some(tokens[i + 1].unquoted.clone());
                i += 2;
                continue;
            } else {
                // dangling --model with no value — treat as extra arg
                extra_args.push(tok.unquoted.clone());
                i += 1;
                continue;
            }
        }

        // --model=<value> (equals form)
        if let Some(val) = tok.unquoted.strip_prefix("--model=") {
            model = Some(val.to_string());
            i += 1;
            continue;
        }

        // -p <value> or -p=<value> — strip both flag and value
        if tok.unquoted == "-p" {
            // skip flag and next token (the prompt value)
            if i + 1 < tokens.len() {
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        if tok.unquoted.starts_with("-p=") {
            // equals form — single token, just skip it
            i += 1;
            continue;
        }

        // --output-format stream-json (two-token sequence)
        if tok.unquoted == "--output-format" {
            if i + 1 < tokens.len() && tokens[i + 1].unquoted == "stream-json" {
                i += 2;
                continue;
            }
            // --output-format with some other value — keep as extra_args
            extra_args.push(tok.raw.clone());
            i += 1;
            continue;
        }

        // Baseline single-token flags to strip
        if tok.unquoted == "--verbose" || tok.unquoted == "--dangerously-skip-permissions" {
            i += 1;
            continue;
        }

        // Anything else goes into extra_args
        extra_args.push(tok.raw.clone());
        i += 1;
    }

    ParseResult::Parsed { model, extra_args }
}

// ── Tokenizer (adapted from m005) ────────────────────────────────────────────

/// A single shell token captured during parsing.
#[derive(Debug, Clone)]
struct Token {
    /// The raw token slice from the input, including any quote characters.
    raw: String,
    /// The token with surrounding quotes stripped.
    unquoted: String,
}

/// Split a command into shell-style tokens (double-quoted, single-quoted,
/// bare). Returns `None` if the input contains an unterminated quoted string.
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

        while i < bytes.len() && bytes[i] != b' ' && bytes[i] != b'\t' {
            match bytes[i] {
                b'"' => {
                    i += 1;
                    while i < bytes.len() && bytes[i] != b'"' {
                        if bytes[i] == b'\\' && i + 1 < bytes.len() {
                            unquoted.push(bytes[i] as char);
                            unquoted.push(bytes[i + 1] as char);
                            i += 2;
                            continue;
                        }
                        unquoted.push(bytes[i] as char);
                        i += 1;
                    }
                    if i >= bytes.len() {
                        return None; // unterminated "
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
                        return None; // unterminated '
                    }
                    i += 1; // skip closing '
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

    // ── Helpers ───────────────────────────────────────────────────────────────

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

    fn read_workflow(db_path: &std::path::Path, id: Uuid) -> (u32, serde_json::Value) {
        let conn = sqlite::open_with_schema(db_path).expect("open");
        let (version, steps_json): (u32, String) = conn
            .query_row(
                "SELECT version, steps_json FROM workflows WHERE id = ?1",
                [id.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("query workflow");
        let value: serde_json::Value = serde_json::from_str(&steps_json).expect("parse steps_json");
        (version, value)
    }

    // ── parse_command_template unit tests ─────────────────────────────────────

    /// Test 1: default template only — no model, no extra_args
    #[test]
    fn parse_default_template_only() {
        let tmpl = r#"claude -p "${prompt}" --output-format stream-json --verbose --dangerously-skip-permissions"#;
        let result = parse_command_template(tmpl);
        let ParseResult::Parsed { model, extra_args } = result else {
            panic!("expected Parsed");
        };
        assert!(model.is_none(), "no model expected");
        assert!(extra_args.is_empty(), "no extra_args expected");
    }

    /// Test 2: model in space form
    #[test]
    fn parse_model_space_form() {
        let tmpl = r#"claude --model claude-haiku-4-5-20251001 -p "${prompt}" --output-format stream-json --verbose --dangerously-skip-permissions"#;
        let result = parse_command_template(tmpl);
        let ParseResult::Parsed { model, extra_args } = result else {
            panic!("expected Parsed");
        };
        assert_eq!(model.as_deref(), Some("claude-haiku-4-5-20251001"));
        assert!(extra_args.is_empty());
    }

    /// Test 3: model in equals form
    #[test]
    fn parse_model_equals_form() {
        let tmpl = r#"claude --model=claude-haiku-4-5-20251001 -p "${prompt}" --output-format stream-json --verbose --dangerously-skip-permissions"#;
        let result = parse_command_template(tmpl);
        let ParseResult::Parsed { model, extra_args } = result else {
            panic!("expected Parsed");
        };
        assert_eq!(model.as_deref(), Some("claude-haiku-4-5-20251001"));
        assert!(extra_args.is_empty());
    }

    /// Test 4: model + extra args
    #[test]
    fn parse_model_and_extra_args() {
        let tmpl = r#"claude --model X -p "${prompt}" --output-format stream-json --verbose --dangerously-skip-permissions --allowedTools WebSearch"#;
        let result = parse_command_template(tmpl);
        let ParseResult::Parsed { model, extra_args } = result else {
            panic!("expected Parsed");
        };
        assert_eq!(model.as_deref(), Some("X"));
        assert_eq!(extra_args, vec!["--allowedTools", "WebSearch"]);
    }

    /// Test 7: malformed template (unterminated quote) — leave unchanged
    #[test]
    fn parse_unterminated_quote_returns_warn() {
        // A string with a genuine unterminated double-quote
        let tmpl2 = "claude -p \"unterminated --output-format stream-json";
        let result = parse_command_template(tmpl2);
        assert!(
            matches!(result, ParseResult::Warn(_)),
            "unterminated quote must produce Warn"
        );
    }

    /// Test 8: non-claude first token — leave unchanged
    #[test]
    fn parse_non_claude_first_token_returns_warn() {
        let tmpl = r#"python script.py -p "${prompt}" --output-format stream-json"#;
        let result = parse_command_template(tmpl);
        assert!(
            matches!(result, ParseResult::Warn(_)),
            "non-claude template must produce Warn"
        );
    }

    // ── DB-level integration tests ─────────────────────────────────────────────

    /// Test 1 (DB): default template → model absent, extra_args = []
    #[tokio::test]
    async fn test_default_template_normalized() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        let wf_id = Uuid::now_v7();
        {
            let conn = sqlite::open_with_schema(&db_path).expect("open");
            let steps = serde_json::json!([{
                "id": "s0",
                "kind": "agent",
                "agent_type": "claude_code_cli",
                "prompt": "${prompt}",
                "command_template": r#"claude -p "${prompt}" --output-format stream-json --verbose --dangerously-skip-permissions"#
            }]);
            insert_workflow(&conn, wf_id, &steps.to_string());
        }

        let m = AgentStepNormalize;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(did_work, "expected Ok(true)");

        let (version, steps) = read_workflow(&db_path, wf_id);
        assert_eq!(version, 2, "version must be bumped");
        let s0 = &steps[0];
        assert!(
            s0.get("command_template").is_none(),
            "command_template must be gone"
        );
        assert!(s0.get("model").is_none(), "no model expected");
        assert_eq!(s0["extra_args"], serde_json::json!([]), "empty extra_args");
    }

    /// Test 2 (DB): model (space form) is extracted
    #[tokio::test]
    async fn test_model_space_form_normalized() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        let wf_id = Uuid::now_v7();
        {
            let conn = sqlite::open_with_schema(&db_path).expect("open");
            let steps = serde_json::json!([{
                "id": "s0",
                "kind": "agent",
                "agent_type": "claude_code_cli",
                "prompt": "${prompt}",
                "command_template": r#"claude --model claude-haiku-4-5-20251001 -p "${prompt}" --output-format stream-json --verbose --dangerously-skip-permissions"#
            }]);
            insert_workflow(&conn, wf_id, &steps.to_string());
        }

        let m = AgentStepNormalize;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(did_work);

        let (_v, steps) = read_workflow(&db_path, wf_id);
        let s0 = &steps[0];
        assert!(s0.get("command_template").is_none());
        assert_eq!(s0["model"], "claude-haiku-4-5-20251001");
        assert_eq!(s0["extra_args"], serde_json::json!([]));
    }

    /// Test 3 (DB): model (equals form) is extracted
    #[tokio::test]
    async fn test_model_equals_form_normalized() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        let wf_id = Uuid::now_v7();
        {
            let conn = sqlite::open_with_schema(&db_path).expect("open");
            let steps = serde_json::json!([{
                "id": "s0",
                "kind": "agent",
                "agent_type": "claude_code_cli",
                "prompt": "${prompt}",
                "command_template": r#"claude --model=claude-haiku-4-5-20251001 -p "${prompt}" --output-format stream-json --verbose --dangerously-skip-permissions"#
            }]);
            insert_workflow(&conn, wf_id, &steps.to_string());
        }

        let m = AgentStepNormalize;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(did_work);

        let (_v, steps) = read_workflow(&db_path, wf_id);
        let s0 = &steps[0];
        assert!(s0.get("command_template").is_none());
        assert_eq!(s0["model"], "claude-haiku-4-5-20251001");
        assert_eq!(s0["extra_args"], serde_json::json!([]));
    }

    /// Test 4 (DB): model + extra args
    #[tokio::test]
    async fn test_model_and_extra_args_normalized() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        let wf_id = Uuid::now_v7();
        {
            let conn = sqlite::open_with_schema(&db_path).expect("open");
            let steps = serde_json::json!([{
                "id": "s0",
                "kind": "agent",
                "agent_type": "claude_code_cli",
                "prompt": "${prompt}",
                "command_template": r#"claude --model X -p "${prompt}" --output-format stream-json --verbose --dangerously-skip-permissions --allowedTools WebSearch"#
            }]);
            insert_workflow(&conn, wf_id, &steps.to_string());
        }

        let m = AgentStepNormalize;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(did_work);

        let (_v, steps) = read_workflow(&db_path, wf_id);
        let s0 = &steps[0];
        assert!(s0.get("command_template").is_none());
        assert_eq!(s0["model"], "X");
        assert_eq!(
            s0["extra_args"],
            serde_json::json!(["--allowedTools", "WebSearch"])
        );
    }

    /// Test 5: MatchStep recursion — agent step inside cases.foo is rewritten
    #[tokio::test]
    async fn test_match_step_recursion() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        let wf_id = Uuid::now_v7();
        {
            let conn = sqlite::open_with_schema(&db_path).expect("open");
            let steps = serde_json::json!([{
                "id": "branch",
                "kind": "match",
                "expr": "${input.kind}",
                "cases": {
                    "foo": [{
                        "id": "agent-in-case",
                        "kind": "agent",
                        "agent_type": "claude_code_cli",
                        "prompt": "${prompt}",
                        "command_template": r#"claude --model claude-opus-4-5 -p "${prompt}" --output-format stream-json --verbose --dangerously-skip-permissions"#
                    }]
                },
                "default": [{
                    "id": "noop",
                    "kind": "shell",
                    "command": "echo hi"
                }]
            }]);
            insert_workflow(&conn, wf_id, &steps.to_string());
        }

        let m = AgentStepNormalize;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(did_work, "expected Ok(true)");

        let (version, steps) = read_workflow(&db_path, wf_id);
        assert_eq!(version, 2);
        let agent_step = &steps[0]["cases"]["foo"][0];
        assert!(agent_step.get("command_template").is_none());
        assert_eq!(agent_step["model"], "claude-opus-4-5");
        assert_eq!(agent_step["extra_args"], serde_json::json!([]));

        // default shell step untouched
        assert_eq!(steps[0]["default"][0]["kind"], "shell");
    }

    /// Test 6: idempotency — second run is a no-op
    #[tokio::test]
    async fn test_idempotent_second_run_is_noop() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        let wf_id = Uuid::now_v7();
        {
            let conn = sqlite::open_with_schema(&db_path).expect("open");
            let steps = serde_json::json!([{
                "id": "s0",
                "kind": "agent",
                "agent_type": "claude_code_cli",
                "prompt": "${prompt}",
                "command_template": r#"claude -p "${prompt}" --output-format stream-json --verbose --dangerously-skip-permissions"#
            }]);
            insert_workflow(&conn, wf_id, &steps.to_string());
        }

        let m = AgentStepNormalize;
        let first = m.run(tmp.path()).await.expect("first run");
        assert!(first, "first run must rewrite");

        let second = m.run(tmp.path()).await.expect("second run");
        assert!(!second, "second run must be a no-op");

        // version must not have been bumped a second time
        let (version, _) = read_workflow(&db_path, wf_id);
        assert_eq!(version, 2, "version must be 2, not 3");
    }

    /// Test 7 (DB): malformed template — step left unchanged
    #[tokio::test]
    async fn test_malformed_template_left_unchanged() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        let wf_id = Uuid::now_v7();
        let bad_template = "claude -p \"unterminated --output-format stream-json";
        {
            let conn = sqlite::open_with_schema(&db_path).expect("open");
            let steps = serde_json::json!([{
                "id": "s0",
                "kind": "agent",
                "agent_type": "claude_code_cli",
                "prompt": "${prompt}",
                "command_template": bad_template
            }]);
            insert_workflow(&conn, wf_id, &steps.to_string());
        }

        let m = AgentStepNormalize;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(
            !did_work,
            "malformed template — nothing should be rewritten"
        );

        let (version, steps) = read_workflow(&db_path, wf_id);
        assert_eq!(version, 1, "version must not be bumped");
        assert_eq!(
            steps[0]["command_template"], bad_template,
            "template must be preserved"
        );
    }

    /// Test 8 (DB): non-claude template — step left unchanged
    #[tokio::test]
    async fn test_non_claude_template_left_unchanged() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        let wf_id = Uuid::now_v7();
        let non_claude = r#"python run.py -p "${prompt}" --output-format stream-json"#;
        {
            let conn = sqlite::open_with_schema(&db_path).expect("open");
            let steps = serde_json::json!([{
                "id": "s0",
                "kind": "agent",
                "agent_type": "custom",
                "prompt": "${prompt}",
                "command_template": non_claude
            }]);
            insert_workflow(&conn, wf_id, &steps.to_string());
        }

        let m = AgentStepNormalize;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(
            !did_work,
            "non-claude template — nothing should be rewritten"
        );

        let (version, steps) = read_workflow(&db_path, wf_id);
        assert_eq!(version, 1, "version must not be bumped");
        assert_eq!(steps[0]["command_template"], non_claude);
    }

    /// Test 9: version bump + updated_at refresh on modification
    #[tokio::test]
    async fn test_version_bumped_on_modification() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        let wf_id = Uuid::now_v7();
        {
            let conn = sqlite::open_with_schema(&db_path).expect("open");
            let steps = serde_json::json!([{
                "id": "s0",
                "kind": "agent",
                "agent_type": "claude_code_cli",
                "prompt": "${prompt}",
                "command_template": r#"claude -p "${prompt}" --output-format stream-json --verbose --dangerously-skip-permissions"#
            }]);
            insert_workflow(&conn, wf_id, &steps.to_string());
        }

        let m = AgentStepNormalize;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(did_work);

        let (version, steps) = read_workflow(&db_path, wf_id);
        assert_eq!(version, 2, "version must be bumped from 1 to 2");
        assert!(steps[0].get("command_template").is_none());
        assert_eq!(steps[0]["extra_args"], serde_json::json!([]));
    }

    /// Fresh install: no acs.db → Ok(false)
    #[tokio::test]
    async fn test_no_db_file_returns_false() {
        let tmp = TempDir::new().unwrap();
        let m = AgentStepNormalize;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(!did_work, "missing acs.db is a no-op");
    }

    /// No workflows with command_template → Ok(false)
    #[tokio::test]
    async fn test_no_command_template_returns_false() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        let wf_id = Uuid::now_v7();
        {
            let conn = sqlite::open_with_schema(&db_path).expect("open");
            // Already-normalized agent step (no command_template)
            let steps = serde_json::json!([{
                "id": "s0",
                "kind": "agent",
                "agent_type": "claude_code_cli",
                "prompt": "${prompt}",
                "model": "claude-opus-4-5",
                "extra_args": []
            }]);
            insert_workflow(&conn, wf_id, &steps.to_string());
        }

        let m = AgentStepNormalize;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(
            !did_work,
            "already-normalized workflow must not trigger rewrite"
        );
    }

    /// Test: a step that has BOTH `command_template` (without --model) AND a
    /// pre-existing `model` field retains the pre-existing model value after
    /// migration. The old code would call `map.remove("model")` in the
    /// `None` branch, silently destroying the pre-existing value.
    #[tokio::test]
    async fn test_preexisting_model_preserved_when_no_model_in_template() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        let wf_id = Uuid::now_v7();
        {
            let conn = sqlite::open_with_schema(&db_path).expect("open");
            // command_template has no --model flag, but step already has model
            let steps = serde_json::json!([{
                "id": "s0",
                "kind": "agent",
                "agent_type": "claude_code_cli",
                "prompt": "${prompt}",
                "model": "preexisting-model",
                "command_template": r#"claude -p "${prompt}" --output-format stream-json --verbose --dangerously-skip-permissions"#
            }]);
            insert_workflow(&conn, wf_id, &steps.to_string());
        }

        let m = AgentStepNormalize;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(did_work, "step with command_template should be rewritten");

        let (_v, steps) = read_workflow(&db_path, wf_id);
        let s0 = &steps[0];
        assert!(
            s0.get("command_template").is_none(),
            "command_template must be removed"
        );
        assert_eq!(
            s0["model"], "preexisting-model",
            "pre-existing model must be preserved when template has no --model"
        );
        assert_eq!(
            s0["extra_args"],
            serde_json::json!([]),
            "extra_args should be empty"
        );
    }
}
