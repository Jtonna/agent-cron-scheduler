//! Minimal shell tokenizer shared by the m005 and m006 code migrations.
//!
//! Recognises double-quoted, single-quoted, and bare tokens — enough to
//! cover every shell-claude command and command_template observed in
//! migrated workflows. Escape sequences inside double quotes are preserved
//! verbatim (de-quoted without de-escaping), matching what the runtime
//! passed to the shell.

/// A single shell token captured during parsing.
#[derive(Debug, Clone)]
pub(crate) struct Token {
    /// The raw token slice from the input, including any quote characters.
    /// Used to rebuild command templates so untouched tokens round-trip
    /// byte-for-byte.
    pub(crate) raw: String,
    /// The token with surrounding quotes stripped.
    pub(crate) unquoted: String,
}

/// Split a command into shell-style tokens. Returns `None` if the input has
/// an unterminated quoted string — those commands are malformed and callers
/// fail closed (leave the step unmigrated).
pub(crate) fn tokenize(s: &str) -> Option<Vec<Token>> {
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
        // for the commands these migrations rewrite.
        while i < bytes.len() && bytes[i] != b' ' && bytes[i] != b'\t' {
            match bytes[i] {
                b'"' => {
                    i += 1;
                    while i < bytes.len() && bytes[i] != b'"' {
                        if bytes[i] == b'\\' && i + 1 < bytes.len() {
                            // Preserve the escape byte AND the next byte in
                            // the unquoted string. The runtime never
                            // unescapes either, so neither do the migrations.
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
pub(crate) fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut out = s[..max].to_string();
    out.push('…');
    out
}
