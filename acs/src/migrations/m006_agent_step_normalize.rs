//! m006_agent_step_normalize — normalize `agent` steps that still carry a
//! `command_template` field.
//!
//! # Why this migration carries Rust logic
//!
//! Same reason as m005: the transform needs the shell tokenizer, CLI-flag
//! parsing, and recursion into nested `match` step branches, none of which
//! SQL alone can express. The body mixes SQL strings (via the framework's
//! [`MigrationTx`] API) with Rust-level JSON transformation, and is frozen
//! by construction (pure `serde_json::Value`, no live model types).
//!
//! # Background
//!
//! Older agent steps stored CLI customisation as a single `command_template`
//! string. That field was replaced with two structured fields:
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
//!   1. Tokenize the template string (shared minimal shell tokenizer).
//!   2. If the first token is not `claude`, warn and leave the step
//!      unchanged (conservative: unrecognised commands are not touched).
//!   3. If tokenisation fails (unterminated quote), warn and leave unchanged.
//!   4. Otherwise extract:
//!      - `--model <value>` or `--model=<value>` → `model` field
//!      - Remove baseline tokens: first `claude` token, `-p` + its value,
//!        `--output-format stream-json`, `--verbose`,
//!        `--dangerously-skip-permissions`
//!      - Whatever tokens remain → `extra_args` array (empty `[]` if none)
//!      - Drop `command_template`.

use milepost::{MigrateError, Migration, MigrationTx, SqlValue};

use super::shell_tokens::{tokenize, truncate};

pub(crate) struct AgentStepNormalize;

impl Migration for AgentStepNormalize {
    fn name(&self) -> &'static str {
        "m006_agent_step_normalize"
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

            let rewritten = normalize_steps_array(&mut value);
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
                "m006_agent_step_normalize: normalized {} agent steps across {} workflows",
                steps_rewritten,
                workflows_rewritten
            );
        }
        Ok(())
    }
}

/// Walk a `steps_json` array, normalizing any agent steps that carry
/// `command_template`. Recurses into `match` step branches. Returns the
/// total number of steps rewritten.
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
/// 0 if skipped. Descends into `match` step branches.
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
                    map.remove("command_template");

                    // Set model field only when extracted from
                    // command_template. If no --model flag was present in the
                    // template, leave any pre-existing `model` field untouched
                    // (defensive: don't destroy a value that arrived via
                    // another path).
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

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    // ── parse_command_template unit-level coverage ──────────────────────────

    fn parsed(template: &str) -> (Option<String>, Vec<String>) {
        match parse_command_template(template) {
            ParseResult::Parsed { model, extra_args } => (model, extra_args),
            ParseResult::Warn(msg) => panic!("expected Parsed, got Warn({})", msg),
        }
    }

    #[test]
    fn parse_default_template_yields_nothing() {
        let (model, extra) = parsed(
            r#"claude -p "${prompt}" --output-format stream-json --verbose --dangerously-skip-permissions"#,
        );
        assert!(model.is_none());
        assert!(extra.is_empty());
    }

    #[test]
    fn parse_model_space_form() {
        let (model, extra) = parsed(
            r#"claude --model claude-opus-4-5 -p "${prompt}" --output-format stream-json --verbose --dangerously-skip-permissions"#,
        );
        assert_eq!(model.as_deref(), Some("claude-opus-4-5"));
        assert!(extra.is_empty());
    }

    #[test]
    fn parse_model_equals_form() {
        let (model, extra) = parsed(
            r#"claude --model=claude-opus-4-5 -p "${prompt}" --output-format stream-json --verbose --dangerously-skip-permissions"#,
        );
        assert_eq!(model.as_deref(), Some("claude-opus-4-5"));
        assert!(extra.is_empty());
    }

    #[test]
    fn parse_extra_args_collected() {
        let (model, extra) = parsed(
            r#"claude -p "${prompt}" --output-format stream-json --verbose --dangerously-skip-permissions --session-id abc -c"#,
        );
        assert!(model.is_none());
        assert_eq!(extra, vec!["--session-id", "abc", "-c"]);
    }

    #[test]
    fn parse_non_claude_first_token_warns() {
        match parse_command_template("bash -c something") {
            ParseResult::Warn(msg) => assert!(msg.contains("not `claude`")),
            ParseResult::Parsed { .. } => panic!("must warn"),
        }
    }

    #[test]
    fn parse_unterminated_quote_warns() {
        match parse_command_template(r#"claude -p "oops"#) {
            ParseResult::Warn(msg) => assert!(msg.contains("unterminated")),
            ParseResult::Parsed { .. } => panic!("must warn"),
        }
    }

    // ── JSON-level rewrite coverage ─────────────────────────────────────────

    #[test]
    fn normalize_rewrites_agent_step_and_recurses_into_match() {
        let mut steps = serde_json::json!([
            {
                "id": "top",
                "kind": "agent",
                "agent_type": "claude_code_cli",
                "prompt": "hello",
                "command_template": r#"claude --model claude-opus-4-5 -p "${prompt}" --output-format stream-json --verbose --dangerously-skip-permissions --session-id abc"#
            },
            {
                "id": "branch",
                "kind": "match",
                "expr": "${input.kind}",
                "cases": {
                    "a": [
                        {
                            "id": "nested",
                            "kind": "agent",
                            "agent_type": "claude_code_cli",
                            "prompt": "nested",
                            "command_template": r#"claude -p "${prompt}" --output-format stream-json --verbose --dangerously-skip-permissions"#
                        }
                    ]
                }
            }
        ]);
        let n = normalize_steps_array(&mut steps);
        assert_eq!(n, 2);

        let top = &steps[0];
        assert!(top.get("command_template").is_none());
        assert_eq!(top["model"], "claude-opus-4-5");
        assert_eq!(
            top["extra_args"],
            serde_json::json!(["--session-id", "abc"])
        );

        let nested = &steps[1]["cases"]["a"][0];
        assert!(nested.get("command_template").is_none());
        assert!(nested.get("model").is_none(), "no --model → no model field");
        assert_eq!(nested["extra_args"], serde_json::json!([]));
    }

    #[test]
    fn normalize_leaves_unparseable_template_unchanged() {
        let mut steps = serde_json::json!([
            {
                "id": "odd",
                "kind": "agent",
                "agent_type": "claude_code_cli",
                "prompt": "hello",
                "command_template": "bash -c something"
            }
        ]);
        let n = normalize_steps_array(&mut steps);
        assert_eq!(n, 0);
        assert_eq!(steps[0]["command_template"], "bash -c something");
    }

    // ── Transaction-level test ──────────────────────────────────────────────

    #[test]
    fn up_normalizes_and_bumps_version() {
        let mut conn = Connection::open_in_memory().expect("open");
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
        let steps = serde_json::json!([
            {
                "id": "s0",
                "kind": "agent",
                "agent_type": "claude_code_cli",
                "prompt": "hello",
                "command_template": r#"claude --model claude-opus-4-5 -p "${prompt}" --output-format stream-json --verbose --dangerously-skip-permissions"#
            }
        ]);
        conn.execute(
            "INSERT INTO workflows (id, name, version, schedule, schedule_mode,
                 enabled, steps_json, allow_concurrent, on_failure, created_at, updated_at)
             VALUES ('wf1', 'wf', 3, '* * * * *', 'Cron', 1, ?1, 1, '\"abort\"',
                 '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
            [steps.to_string()],
        )
        .expect("insert");

        let tx = conn.transaction().expect("tx");
        AgentStepNormalize
            .up(&MigrationTx::new(&tx))
            .expect("migrate");
        tx.commit().expect("commit");

        let (version, steps_after): (u32, String) = conn
            .query_row(
                "SELECT version, steps_json FROM workflows WHERE id = 'wf1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("read");
        assert_eq!(version, 4, "version bumped");
        let parsed: serde_json::Value = serde_json::from_str(&steps_after).unwrap();
        assert!(parsed[0].get("command_template").is_none());
        assert_eq!(parsed[0]["model"], "claude-opus-4-5");
        assert_eq!(parsed[0]["extra_args"], serde_json::json!([]));
    }
}
