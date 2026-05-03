use crate::workflow::agents::{AgentImpl, AgentOutput, AgentOutputParser};
use crate::workflow::step::CostFragment;

pub struct ClaudeCodeCli;

impl AgentImpl for ClaudeCodeCli {
    fn default_command_template(&self) -> &str {
        // The prompt placeholder is ${prompt}. The substitute() function in template.rs
        // handles ${input.*} and ${steps.*.*}, but for agent steps we ALSO want to support
        // ${prompt} as a special token that AgentStep::execute() substitutes after the
        // input/steps pass.
        r#"claude -p "${prompt}" --output-format stream-json --verbose --dangerously-skip-permissions"#
    }

    fn output_parser(&self) -> Box<dyn AgentOutputParser> {
        Box::new(ClaudeStreamParser::new())
    }
}

/// Streaming NDJSON parser for Claude Code CLI's stream-json output format.
///
/// Aggregates cost/usage across multiple invocations in the same run (e.g.,
/// chained `claude -p ...` calls within a single shell command).
pub struct ClaudeStreamParser {
    buffer: Vec<u8>,
    current_model: Option<String>,
    total_cost_usd: f64,
    total_duration_ms: u64,
    total_num_turns: u32,
    saw_any_result: bool,
    aggregated_usage: Option<serde_json::Value>,
    final_message: Option<String>,
}

impl ClaudeStreamParser {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            current_model: None,
            total_cost_usd: 0.0,
            total_duration_ms: 0,
            total_num_turns: 0,
            saw_any_result: false,
            aggregated_usage: None,
            final_message: None,
        }
    }

    fn process_line(&mut self, line: &str) {
        let trimmed = line.trim();
        // Skip empty lines or lines that don't look like JSON objects.
        if trimmed.is_empty() || !trimmed.starts_with('{') {
            return;
        }

        let val: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => return, // silently skip non-JSON lines
        };

        match val.get("type").and_then(|t| t.as_str()) {
            Some("system") => {
                // Keep only the first model seen across all invocations.
                if self.current_model.is_none() {
                    if let Some(model) = val.get("model").and_then(|m| m.as_str()) {
                        self.current_model = Some(model.to_string());
                    }
                }
            }
            Some("result") => {
                if let Some(cost) = val.get("total_cost_usd").and_then(|v| v.as_f64()) {
                    self.total_cost_usd += cost;
                }
                if let Some(ms) = val.get("duration_ms").and_then(|v| v.as_u64()) {
                    self.total_duration_ms += ms;
                }
                if let Some(turns) = val.get("num_turns").and_then(|v| v.as_u64()) {
                    self.total_num_turns += turns as u32;
                }
                // Extract result message (the final assistant reply).
                if let Some(msg) = val.get("result").and_then(|v| v.as_str()) {
                    self.final_message = Some(msg.to_string());
                }
                // Merge usage token fields across invocations.
                if let Some(serde_json::Value::Object(new_usage)) = val.get("usage").cloned() {
                    let merged = self
                        .aggregated_usage
                        .get_or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
                    if let serde_json::Value::Object(ref mut acc) = merged {
                        for (k, v) in new_usage {
                            if let Some(n) = v.as_f64() {
                                let entry =
                                    acc.entry(k).or_insert(serde_json::Value::Number(
                                        serde_json::Number::from(0u64),
                                    ));
                                let existing = entry.as_f64().unwrap_or(0.0);
                                let sum = existing + n;
                                // Preserve integer representation where possible.
                                *entry = if sum.fract() == 0.0 && sum >= 0.0 {
                                    serde_json::Value::Number(serde_json::Number::from(
                                        sum as u64,
                                    ))
                                } else {
                                    serde_json::Value::Number(
                                        serde_json::Number::from_f64(sum)
                                            .unwrap_or(serde_json::Number::from(0u64)),
                                    )
                                };
                            }
                        }
                    }
                }
                self.saw_any_result = true;
            }
            _ => {}
        }
    }
}

impl AgentOutputParser for ClaudeStreamParser {
    fn parse_chunk(&mut self, chunk: &[u8]) {
        self.buffer.extend_from_slice(chunk);

        // Process all complete lines in the buffer.
        while let Some(pos) = self.buffer.iter().position(|&b| b == b'\n') {
            // Extract the complete line (without the newline).
            let line_bytes = self.buffer[..pos].to_vec();
            // Remove the processed line + newline from the buffer.
            self.buffer.drain(..=pos);

            if let Ok(line) = std::str::from_utf8(&line_bytes) {
                self.process_line(line);
            }
        }
    }

    fn finalize(self: Box<Self>) -> AgentOutput {
        // Process any remaining bytes in the buffer (partial last line without trailing newline).
        let mut this = *self;
        if !this.buffer.is_empty() {
            // Copy buffer so we can release the borrow before calling process_line.
            let remaining = this.buffer.clone();
            this.buffer.clear();
            if let Ok(line) = std::str::from_utf8(&remaining) {
                this.process_line(line);
            }
        }

        if !this.saw_any_result {
            // No structured Claude output — nothing to report.
            return AgentOutput {
                cost: None,
                final_message: None,
            };
        }

        let cost = CostFragment {
            total_cost_usd: Some(this.total_cost_usd),
            duration_ms: Some(this.total_duration_ms),
            num_turns: Some(this.total_num_turns),
            model: this.current_model,
            usage: this.aggregated_usage,
        };
        AgentOutput {
            cost: Some(cost),
            final_message: this.final_message,
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── Test 1: Single invocation NDJSON ──────────────────────────────────────

    #[test]
    fn test_single_invocation_ndjson() {
        let ndjson = concat!(
            r#"{"type":"system","subtype":"init","session_id":"s1","model":"claude-opus-4-5","tools":[]}"#,
            "\n",
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hello!"}]}}"#,
            "\n",
            r#"{"type":"result","subtype":"success","total_cost_usd":0.0025,"duration_ms":1200,"num_turns":1,"result":"Hello!"}"#,
            "\n",
        );

        let mut parser = Box::new(ClaudeStreamParser::new());
        parser.parse_chunk(ndjson.as_bytes());
        let output = parser.finalize();

        let cost = output.cost.expect("should have cost");
        assert!((cost.total_cost_usd.unwrap() - 0.0025).abs() < 1e-9);
        assert_eq!(cost.model.as_deref(), Some("claude-opus-4-5"));
        assert_eq!(cost.duration_ms, Some(1200));
        assert_eq!(cost.num_turns, Some(1));
        assert_eq!(output.final_message.as_deref(), Some("Hello!"));
    }

    // ── Test 2: Multiple invocations — costs summed ──────────────────────────

    #[test]
    fn test_multiple_invocations_costs_summed() {
        let ndjson = concat!(
            // First invocation
            r#"{"type":"system","subtype":"init","model":"claude-sonnet-4-5"}"#,
            "\n",
            r#"{"type":"result","total_cost_usd":0.001,"duration_ms":500,"num_turns":1,"result":"first"}"#,
            "\n",
            // Second invocation
            r#"{"type":"system","subtype":"init","model":"claude-opus-4-5"}"#,
            "\n",
            r#"{"type":"result","total_cost_usd":0.002,"duration_ms":700,"num_turns":2,"result":"second"}"#,
            "\n",
        );

        let mut parser = Box::new(ClaudeStreamParser::new());
        parser.parse_chunk(ndjson.as_bytes());
        let output = parser.finalize();

        let cost = output.cost.expect("should have cost");
        // Costs summed
        assert!((cost.total_cost_usd.unwrap() - 0.003).abs() < 1e-9);
        // Duration summed
        assert_eq!(cost.duration_ms, Some(1200));
        // Turns summed
        assert_eq!(cost.num_turns, Some(3));
        // Model is from first system.init
        assert_eq!(cost.model.as_deref(), Some("claude-sonnet-4-5"));
        // final_message is from the LAST result
        assert_eq!(output.final_message.as_deref(), Some("second"));
    }

    // ── Test 3: Mixed shell output — non-JSON lines are ignored ──────────────

    #[test]
    fn test_mixed_shell_output_ignored() {
        let ndjson = concat!(
            "Starting claude...\n",
            "Some debug output\n",
            r#"{"type":"system","subtype":"init","model":"claude-opus-4-5"}"#,
            "\n",
            "Another shell line\n",
            r#"{"type":"result","total_cost_usd":0.001,"duration_ms":300,"num_turns":1,"result":"done"}"#,
            "\n",
            "Exit code: 0\n",
        );

        let mut parser = Box::new(ClaudeStreamParser::new());
        // Should not panic even with mixed content
        parser.parse_chunk(ndjson.as_bytes());
        let output = parser.finalize();

        let cost = output.cost.expect("should have cost");
        assert!((cost.total_cost_usd.unwrap() - 0.001).abs() < 1e-9);
        assert_eq!(cost.model.as_deref(), Some("claude-opus-4-5"));
        assert_eq!(output.final_message.as_deref(), Some("done"));
    }

    // ── Test 4: Partial chunks — split across two parse_chunk calls ──────────

    #[test]
    fn test_partial_chunks_split_across_calls() {
        let full_line = r#"{"type":"system","subtype":"init","model":"claude-opus-4-5"}"#;
        let result_line = r#"{"type":"result","total_cost_usd":0.005,"duration_ms":2000,"num_turns":3,"result":"partial"}"#;

        // Split the first line in the middle
        let mid = full_line.len() / 2;
        let chunk1 = &full_line.as_bytes()[..mid];
        let chunk2 = format!("{}\n{}\n", &full_line[mid..], result_line);

        let mut parser = Box::new(ClaudeStreamParser::new());
        parser.parse_chunk(chunk1);
        parser.parse_chunk(chunk2.as_bytes());
        let output = parser.finalize();

        let cost = output.cost.expect("should have cost");
        assert_eq!(cost.model.as_deref(), Some("claude-opus-4-5"));
        assert!((cost.total_cost_usd.unwrap() - 0.005).abs() < 1e-9);
        assert_eq!(cost.num_turns, Some(3));
        assert_eq!(output.final_message.as_deref(), Some("partial"));
    }

    // ── Test 5: No result events — cost is None ───────────────────────────────

    #[test]
    fn test_no_result_events_cost_none() {
        let ndjson = concat!(
            r#"{"type":"system","subtype":"init","model":"claude-opus-4-5"}"#,
            "\n",
            "Just some shell output\n",
            "More text\n",
        );

        let mut parser = Box::new(ClaudeStreamParser::new());
        parser.parse_chunk(ndjson.as_bytes());
        let output = parser.finalize();

        assert!(
            output.cost.is_none(),
            "cost should be None when no result events seen"
        );
        assert!(output.final_message.is_none());
    }

    // ── Test 6: Result with usage object — aggregated ─────────────────────────

    #[test]
    fn test_usage_aggregated_across_results() {
        let ndjson = concat!(
            r#"{"type":"system","subtype":"init","model":"claude-opus-4-5"}"#,
            "\n",
            r#"{"type":"result","total_cost_usd":0.001,"duration_ms":100,"num_turns":1,"result":"r1","usage":{"input_tokens":100,"output_tokens":50}}"#,
            "\n",
            r#"{"type":"result","total_cost_usd":0.002,"duration_ms":200,"num_turns":2,"result":"r2","usage":{"input_tokens":100,"output_tokens":50}}"#,
            "\n",
        );

        let mut parser = Box::new(ClaudeStreamParser::new());
        parser.parse_chunk(ndjson.as_bytes());
        let output = parser.finalize();

        let cost = output.cost.expect("should have cost");
        let usage = cost.usage.expect("should have usage");
        assert_eq!(usage["input_tokens"], json!(200));
        assert_eq!(usage["output_tokens"], json!(100));
    }
}
