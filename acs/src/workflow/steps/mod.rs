// Step implementations (Shell, Script, Http, Match, SetVar, Agent) live here.

pub mod agent;
pub mod http;
pub mod script;
pub mod set_var;
pub mod shell;

use serde_json::Value;

use crate::workflow::step::StepContext;

/// Serialize a trigger `input` value for writing to a step's stdin.
///
/// Strings are written as their raw bytes (no JSON quoting). All other
/// JSON values are serialized as compact JSON.
pub(crate) fn serialize_input_for_stdin(input: &Value) -> Vec<u8> {
    match input {
        Value::String(s) => s.as_bytes().to_vec(),
        other => serde_json::to_vec(other).unwrap_or_default(),
    }
}

/// Serialize a prior step's stdout value for writing to a step's stdin.
///
/// Strings are written as their raw bytes (no JSON quoting). All other
/// JSON values are serialized as compact JSON.
pub(crate) fn serialize_value_for_stdin(value: &Value) -> Vec<u8> {
    match value {
        Value::String(s) => s.as_bytes().to_vec(),
        other => serde_json::to_vec(other).unwrap_or_default(),
    }
}

/// Resolve the bytes (if any) that should be piped to a step's stdin.
///
/// Precedence:
/// 1. If `ctx.target_step` matches `step_id`: serialize the trigger's input.
///    This OVERRIDES `pass_stdin` for the matching step.
/// 2. Else if `pass_stdin` is `true`: serialize the immediately-prior step's
///    stdout (last value inserted into `ctx.steps`).
/// 3. Else: no stdin routed.
///
/// Returns `None` when no stdin should be written, or the empty bytes case
/// (so callers can skip the actual write call).
pub(crate) fn resolve_stdin_source(
    ctx: &StepContext,
    step_id: &str,
    pass_stdin: bool,
) -> Option<Vec<u8>> {
    if ctx.target_step.as_deref() == Some(step_id) {
        let bytes = serialize_input_for_stdin(&ctx.input);
        if bytes.is_empty() {
            None
        } else {
            Some(bytes)
        }
    } else if pass_stdin {
        let prev_stdout = ctx.steps.values().last().and_then(|s| s.stdout.as_ref())?;
        let bytes = serialize_value_for_stdin(prev_stdout);
        if bytes.is_empty() {
            None
        } else {
            Some(bytes)
        }
    } else {
        None
    }
}
