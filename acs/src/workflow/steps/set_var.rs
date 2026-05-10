/// `SetVarStep` implementation of the [`Step`] trait.
///
/// ## Export value parsing rule
///
/// After template substitution, each export value string is **first attempted
/// as JSON** (`serde_json::from_str`). If that succeeds, the parsed
/// `serde_json::Value` is used directly (so `"42"` → `Value::Number(42)`,
/// `"{\"k\":1}"` → `Value::Object`, `"\"A\""` → `Value::String("A")`, etc.).
/// If JSON parsing fails (e.g. `"bar"` is not valid JSON), the raw string is
/// wrapped in `Value::String`.
///
/// This rule maximises chaining utility: numeric literals become Numbers,
/// quoted strings lose the extra quotes, and objects/arrays are usable
/// directly by downstream template references.
pub use crate::models::workflow::SetVarStep;

use crate::workflow::step::{Step, StepContext, StepError, StepOutput};
use crate::workflow::template;

use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;

#[async_trait]
impl Step for SetVarStep {
    fn kind(&self) -> &'static str {
        "set_var"
    }

    async fn execute(&self, ctx: &mut StepContext) -> Result<StepOutput, StepError> {
        // 1. Emit step start marker.
        let log_byte_offset_start = ctx
            .log_sink
            .write_step_start(&self.common.id, Utc::now())
            .await
            .map_err(StepError::Io)?;

        // 2. For each (export_name, template_str): substitute then try-parse as JSON.
        let mut exports: HashMap<String, serde_json::Value> = HashMap::new();
        for (name, template_str) in &self.exports {
            let sub = template::substitute(template_str, &ctx.input, &ctx.steps);
            for warn in &sub.warnings {
                tracing::warn!(step_id = %self.common.id, "template warning: {}", warn);
            }
            let resolved = sub.output;

            // Try to parse the resolved string as JSON; fall back to Value::String.
            let value = match serde_json::from_str::<serde_json::Value>(&resolved) {
                Ok(v) => v,
                Err(_) => serde_json::Value::String(resolved),
            };
            exports.insert(name.clone(), value);
        }

        // 3. Emit step end marker.
        let log_byte_offset_end = ctx
            .log_sink
            .write_step_end(&self.common.id, Some(0), Utc::now())
            .await
            .map_err(StepError::Io)?;

        // 4. Return StepOutput.
        Ok(StepOutput {
            exit_code: Some(0),
            stdout: None,
            exports,
            cost: None,
            log_byte_offset_start: Some(log_byte_offset_start),
            log_byte_offset_end: Some(log_byte_offset_end),
        })
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use indexmap::IndexMap;
    use serde_json::{json, Value};
    use uuid::Uuid;

    use crate::models::workflow::{CaptureSpec, SetVarStep, StepDefCommon};
    use crate::workflow::step::{LogSink, Step, StepContext};

    // ── Mock LogSink ──────────────────────────────────────────────────────────

    #[derive(Clone, Default)]
    struct MockLogSink {
        events: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl LogSink for MockLogSink {
        async fn write_step_start(
            &self,
            step_id: &str,
            _started_at: DateTime<Utc>,
        ) -> std::io::Result<u64> {
            self.events
                .lock()
                .unwrap()
                .push(format!("start:{}", step_id));
            Ok(0)
        }

        async fn write_chunk(&self, _data: &[u8]) -> std::io::Result<()> {
            Ok(())
        }

        async fn write_step_end(
            &self,
            step_id: &str,
            exit_code: Option<i32>,
            _finished_at: DateTime<Utc>,
        ) -> std::io::Result<u64> {
            self.events.lock().unwrap().push(format!(
                "end:{}:{}",
                step_id,
                exit_code.map(|c| c.to_string()).unwrap_or("-1".to_string())
            ));
            Ok(0)
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn make_ctx(sink: Arc<dyn LogSink>) -> StepContext {
        StepContext {
            workflow_id: Uuid::now_v7(),
            workflow_version: 1,
            run_id: Uuid::now_v7(),
            step_index: 0,
            input: json!({}),
            steps: IndexMap::new(),
            log_sink: sink,
            working_dir: None,
            env: HashMap::new(),
            event_tx: None,
            kill_rx: None,
            target_step: None,
        }
    }

    fn make_step(id: &str, exports: HashMap<String, String>) -> SetVarStep {
        SetVarStep {
            common: StepDefCommon {
                id: id.to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: None,
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec::default(),
            },
            exports,
        }
    }

    // ── Test 1: Static literal string export ──────────────────────────────────

    #[tokio::test]
    async fn test_set_var_static_string_export() {
        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let mut exports = HashMap::new();
        exports.insert("foo".to_string(), "bar".to_string());
        let step = make_step("sv1", exports);

        let output = step
            .execute(&mut ctx)
            .await
            .expect("execute should succeed");

        assert_eq!(output.exit_code, Some(0));
        // "bar" is not valid JSON, so it stays as Value::String
        assert_eq!(
            output.exports.get("foo"),
            Some(&Value::String("bar".to_string()))
        );
    }

    // ── Test 2: Template substitution ─────────────────────────────────────────

    #[tokio::test]
    async fn test_set_var_template_substitution() {
        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);
        ctx.input = json!({"user": "alice"});

        let mut exports = HashMap::new();
        exports.insert("name".to_string(), "${input.user}".to_string());
        let step = make_step("sv2", exports);

        let output = step
            .execute(&mut ctx)
            .await
            .expect("execute should succeed");

        assert_eq!(
            output.exports.get("name"),
            Some(&Value::String("alice".to_string()))
        );
    }

    // ── Test 3: Numeric literal parses as JSON Number ─────────────────────────

    #[tokio::test]
    async fn test_set_var_numeric_literal_becomes_number() {
        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let mut exports = HashMap::new();
        exports.insert("count".to_string(), "42".to_string());
        let step = make_step("sv3", exports);

        let output = step
            .execute(&mut ctx)
            .await
            .expect("execute should succeed");

        // "42" is valid JSON → Value::Number(42)
        assert_eq!(output.exports.get("count"), Some(&json!(42)));
    }

    // ── Test 4: JSON object literal ───────────────────────────────────────────

    #[tokio::test]
    async fn test_set_var_json_object_literal() {
        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let mut exports = HashMap::new();
        exports.insert("meta".to_string(), r#"{"k":1}"#.to_string());
        let step = make_step("sv4", exports);

        let output = step
            .execute(&mut ctx)
            .await
            .expect("execute should succeed");

        // r#"{"k":1}"# is valid JSON → Value::Object
        assert_eq!(output.exports.get("meta"), Some(&json!({"k": 1})));
    }

    // ── Test 5: Multiple exports all present ──────────────────────────────────

    #[tokio::test]
    async fn test_set_var_multiple_exports() {
        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);
        ctx.input = json!({"env": "prod"});

        let mut exports = HashMap::new();
        exports.insert("a".to_string(), "hello".to_string());
        exports.insert("b".to_string(), "99".to_string());
        exports.insert("c".to_string(), "${input.env}".to_string());
        let step = make_step("sv5", exports);

        let output = step
            .execute(&mut ctx)
            .await
            .expect("execute should succeed");

        assert_eq!(
            output.exports.get("a"),
            Some(&Value::String("hello".to_string()))
        );
        assert_eq!(output.exports.get("b"), Some(&json!(99)));
        assert_eq!(
            output.exports.get("c"),
            Some(&Value::String("prod".to_string()))
        );
        assert_eq!(output.exports.len(), 3);
    }

    // ── Test 6: Empty exports map ─────────────────────────────────────────────

    #[tokio::test]
    async fn test_set_var_empty_exports() {
        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = make_step("sv6", HashMap::new());
        let output = step
            .execute(&mut ctx)
            .await
            .expect("execute should succeed");

        assert_eq!(output.exit_code, Some(0));
        assert!(output.exports.is_empty());
    }

    // ── Test 7: Quoted JSON string becomes String without extra quotes ─────────

    #[tokio::test]
    async fn test_set_var_quoted_json_string_unwraps() {
        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let mut exports = HashMap::new();
        // The literal template value is the four characters: "A" (with quotes)
        // JSON-parsing `"A"` yields Value::String("A"), not Value::String("\"A\"")
        exports.insert("choice".to_string(), r#""A""#.to_string());
        let step = make_step("sv7", exports);

        let output = step
            .execute(&mut ctx)
            .await
            .expect("execute should succeed");

        assert_eq!(
            output.exports.get("choice"),
            Some(&Value::String("A".to_string()))
        );
    }

    // ── Test 8: kind() returns "set_var" ─────────────────────────────────────

    #[test]
    fn test_set_var_kind() {
        let step = make_step("k", HashMap::new());
        assert_eq!(step.kind(), "set_var");
    }

    // ── Test 9: Log sink receives start and end events ─────────────────────────

    #[tokio::test]
    async fn test_set_var_log_markers_emitted() {
        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let mut exports = HashMap::new();
        exports.insert("x".to_string(), "1".to_string());
        let step = make_step("sv9", exports);

        step.execute(&mut ctx).await.expect("execute");

        let events = sink.events.lock().unwrap().clone();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], "start:sv9");
        assert_eq!(events[1], "end:sv9:0");
    }
}
