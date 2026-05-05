use indexmap::IndexMap;

use crate::workflow::step::StepOutput;

#[derive(Debug, Default)]
pub struct SubstitutionResult {
    pub output: String,
    pub warnings: Vec<String>,
}

/// Substitute `${...}` references in `template` against `input` and `steps`.
///
/// # Namespaces
/// - `input.<dotted.path>` — looks up the path in `input` (a `serde_json::Value`).
/// - `steps.<step_id>.stdout` — the step's stdout value; strings rendered raw,
///   structured values rendered as compact JSON.
/// - `steps.<step_id>.exit_code` — the step's exit code as a decimal string.
/// - `steps.<step_id>.exports.<name>` — a named export value.
///
/// Single-pass. Behavior for unresolved references:
/// - **Unknown top-level namespace** (e.g. `${prompt}`, `${foo.bar}`): the token
///   is left intact in the output, no warning. This lets layered substitution
///   passes (e.g. the agent step's `${prompt}` pass) handle their own tokens.
/// - **Known namespace, missing field** (e.g. `${input.missing}`,
///   `${steps.x.bad_accessor}`): substituted with empty string + warning.
pub fn substitute(
    template: &str,
    input: &serde_json::Value,
    steps: &IndexMap<String, StepOutput>,
) -> SubstitutionResult {
    let mut result = SubstitutionResult::default();
    let bytes = template.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Look for '${'
        if i + 1 < len && bytes[i] == b'$' && bytes[i + 1] == b'{' {
            // Find the matching '}'
            let start = i + 2; // after '${'
            let mut j = start;
            while j < len && bytes[j] != b'}' {
                j += 1;
            }

            if j >= len {
                // No closing '}' — treat as literal and warn
                result
                    .warnings
                    .push(format!("unclosed '${{{}'", &template[start..]));
                result.output.push_str(&template[i..]);
                break;
            }

            let path = &template[start..j];

            if path.is_empty() {
                result
                    .warnings
                    .push("empty substitution path '${}' found".to_string());
                // Leave as literal text
                result.output.push_str("${");
                result.output.push('}');
            } else if !path.starts_with("input.") && !path.starts_with("steps.") {
                // Unknown top-level namespace — pass through unchanged so layered
                // substitution passes (e.g. agent step's ${prompt}) can handle it.
                result.output.push_str("${");
                result.output.push_str(path);
                result.output.push('}');
            } else {
                let resolved = resolve_path(path, input, steps);
                match resolved {
                    Some(value) => result.output.push_str(&value),
                    None => {
                        result.warnings.push(format!(
                            "substitution '${{{path}}}' did not resolve; substituting empty string"
                        ));
                        // empty string substitution — nothing to push
                    }
                }
            }

            i = j + 1; // move past '}'
        } else {
            // Regular character
            result.output.push(bytes[i] as char);
            i += 1;
        }
    }

    result
}

/// Resolve a dotted path expression against `input` and `steps`.
/// Returns `None` if the reference cannot be resolved.
fn resolve_path(
    path: &str,
    input: &serde_json::Value,
    steps: &IndexMap<String, StepOutput>,
) -> Option<String> {
    if let Some(rest) = path.strip_prefix("input.") {
        // Convert dotted path to JSON pointer (/a/b/c)
        if rest.is_empty() {
            return None;
        }
        let pointer = format!("/{}", rest.replace('.', "/"));
        input.pointer(&pointer).map(value_to_string)
    } else if let Some(rest) = path.strip_prefix("steps.") {
        resolve_step_path(rest, steps)
    } else {
        None
    }
}

/// Resolve a path of the form `<step_id>.<accessor>` against the steps map.
fn resolve_step_path(path: &str, steps: &indexmap::IndexMap<String, StepOutput>) -> Option<String> {
    // Split into step_id and the rest after the first '.'
    let dot = path.find('.')?;
    let step_id = &path[..dot];
    let accessor = &path[dot + 1..];

    let output = steps.get(step_id)?;

    if accessor == "stdout" {
        output.stdout.as_ref().map(value_to_string)
    } else if accessor == "exit_code" {
        output.exit_code.map(|c| c.to_string())
    } else if let Some(export_name) = accessor.strip_prefix("exports.") {
        output.exports.get(export_name).map(value_to_string)
    } else {
        None
    }
}

/// Convert a serde_json::Value to a display string:
/// - String values: use the raw string content (no quotes).
/// - All other values: compact JSON.
fn value_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use serde_json::json;

    fn no_steps() -> IndexMap<String, StepOutput> {
        IndexMap::new()
    }

    // ── input substitution ────────────────────────────────────────────────────

    #[test]
    fn test_basic_input_substitution() {
        let input = json!({"name": "world"});
        let res = substitute("hello ${input.name}", &input, &no_steps());
        assert_eq!(res.output, "hello world");
        assert!(res.warnings.is_empty());
    }

    #[test]
    fn test_nested_input_substitution() {
        let input = json!({"user": {"email": "alice@example.com"}});
        let res = substitute("${input.user.email}", &input, &no_steps());
        assert_eq!(res.output, "alice@example.com");
        assert!(res.warnings.is_empty());
    }

    #[test]
    fn test_missing_input_produces_empty_and_warning() {
        let input = json!({"name": "world"});
        let res = substitute("${input.missing}", &input, &no_steps());
        assert_eq!(res.output, "");
        assert_eq!(res.warnings.len(), 1);
        assert!(res.warnings[0].contains("missing"));
    }

    #[test]
    fn test_multiple_substitutions_in_one_template() {
        let input = json!({"a": "foo", "b": "bar"});
        let res = substitute("a=${input.a} b=${input.b}", &input, &no_steps());
        assert_eq!(res.output, "a=foo b=bar");
        assert!(res.warnings.is_empty());
    }

    // ── step substitution ─────────────────────────────────────────────────────

    #[test]
    fn test_step_stdout_string() {
        let mut steps = IndexMap::new();
        steps.insert(
            "fetch".to_string(),
            StepOutput {
                stdout: Some(json!("data")),
                ..Default::default()
            },
        );
        let res = substitute("${steps.fetch.stdout}", &json!({}), &steps);
        assert_eq!(res.output, "data");
        assert!(res.warnings.is_empty());
    }

    #[test]
    fn test_step_stdout_structured() {
        let mut steps = IndexMap::new();
        steps.insert(
            "fetch".to_string(),
            StepOutput {
                stdout: Some(json!({"x": 1})),
                ..Default::default()
            },
        );
        let res = substitute("${steps.fetch.stdout}", &json!({}), &steps);
        // compact JSON — key order may vary but value should be present
        assert!(res.output.contains("\"x\""));
        assert!(res.output.contains("1"));
        assert!(res.warnings.is_empty());
    }

    #[test]
    fn test_step_exit_code() {
        let mut steps = IndexMap::new();
        steps.insert(
            "fetch".to_string(),
            StepOutput {
                exit_code: Some(0),
                ..Default::default()
            },
        );
        let res = substitute("${steps.fetch.exit_code}", &json!({}), &steps);
        assert_eq!(res.output, "0");
        assert!(res.warnings.is_empty());
    }

    #[test]
    fn test_step_exports() {
        let mut steps = IndexMap::new();
        let mut exports = HashMap::new();
        exports.insert("session_id".to_string(), json!("abc"));
        steps.insert(
            "fetch".to_string(),
            StepOutput {
                exports,
                ..Default::default()
            },
        );
        let res = substitute("${steps.fetch.exports.session_id}", &json!({}), &steps);
        assert_eq!(res.output, "abc");
        assert!(res.warnings.is_empty());
    }

    // ── edge cases ────────────────────────────────────────────────────────────

    #[test]
    fn test_unclosed_brace_leaves_literal_and_warns() {
        let res = substitute("before ${unclosed", &json!({}), &no_steps());
        // The unclosed part should appear literally in the output
        assert!(res.output.contains("${") || res.output.contains("unclosed"));
        assert_eq!(res.warnings.len(), 1);
    }

    #[test]
    fn test_empty_path_leaves_literal_and_warns() {
        let res = substitute("${}", &json!({}), &no_steps());
        assert_eq!(res.output, "${}");
        assert_eq!(res.warnings.len(), 1);
        assert!(res.warnings[0].contains("empty"));
    }

    #[test]
    fn test_no_substitutions_passthrough() {
        let res = substitute("just a plain string", &json!({}), &no_steps());
        assert_eq!(res.output, "just a plain string");
        assert!(res.warnings.is_empty());
    }

    #[test]
    fn test_missing_step_produces_empty_and_warning() {
        let res = substitute("${steps.nonexistent.stdout}", &json!({}), &no_steps());
        assert_eq!(res.output, "");
        assert_eq!(res.warnings.len(), 1);
    }

    #[test]
    fn test_input_number_value_renders_as_json() {
        let input = json!({"count": 42});
        let res = substitute("count=${input.count}", &input, &no_steps());
        assert_eq!(res.output, "count=42");
        assert!(res.warnings.is_empty());
    }

    #[test]
    fn test_literal_dollar_without_brace_passthrough() {
        let res = substitute("cost is $10", &json!({}), &no_steps());
        assert_eq!(res.output, "cost is $10");
        assert!(res.warnings.is_empty());
    }

    #[test]
    fn test_unknown_namespace_preserved_as_literal() {
        // Tokens whose top-level segment is neither `input.` nor `steps.` are
        // passed through unchanged so layered substitution (e.g. the agent
        // step's ${prompt} pass) can handle them.
        let res = substitute("claude -p \"${prompt}\" --flag", &json!({}), &no_steps());
        assert_eq!(res.output, "claude -p \"${prompt}\" --flag");
        assert!(res.warnings.is_empty());
    }

    #[test]
    fn test_unknown_namespace_with_dots_preserved() {
        let res = substitute("${env.HOME}/log", &json!({}), &no_steps());
        assert_eq!(res.output, "${env.HOME}/log");
        assert!(res.warnings.is_empty());
    }

    #[test]
    fn test_known_namespace_miss_still_warns() {
        // Make sure the unknown-namespace pass-through doesn't accidentally
        // mask real misses inside `input.` or `steps.`.
        let res = substitute("${input.missing}", &json!({}), &no_steps());
        assert_eq!(res.output, "");
        assert_eq!(res.warnings.len(), 1);
    }
}
