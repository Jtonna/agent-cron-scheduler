//! Minimal shell tokenizer shared by the m005 and m006 migrations.
//!
//! Recognises double-quoted, single-quoted, and bare tokens — enough to
//! cover every shell-claude command and command_template observed in
//! migrated workflows. Escape sequences inside double quotes are preserved
//! verbatim (de-quoted without de-escaping), matching what the runtime
//! passed to the shell. Input is scanned as characters, so non-ASCII
//! content (prompts in any language) round-trips intact.

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
    // (byte offset, char) pairs: chars drive the scan, byte offsets slice
    // the raw token text out of the original string.
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let n = chars.len();
    let mut tokens: Vec<Token> = Vec::new();
    let mut i = 0;

    while i < n {
        // Skip whitespace between tokens.
        while i < n && (chars[i].1 == ' ' || chars[i].1 == '\t') {
            i += 1;
        }
        if i >= n {
            break;
        }

        let start = chars[i].0;
        let mut unquoted = String::new();

        // Parse one token, which may be composed of multiple adjacent
        // quoted / bare runs (e.g. `prefix"quoted"suffix` collapses into a
        // single token). This mirrors POSIX-shell behaviour closely enough
        // for the commands these migrations rewrite.
        while i < n && chars[i].1 != ' ' && chars[i].1 != '\t' {
            match chars[i].1 {
                '"' => {
                    i += 1;
                    while i < n && chars[i].1 != '"' {
                        if chars[i].1 == '\\' && i + 1 < n {
                            // Preserve the escape character AND the next
                            // character in the unquoted string. The runtime
                            // never unescapes either, so neither do the
                            // migrations.
                            unquoted.push('\\');
                            unquoted.push(chars[i + 1].1);
                            i += 2;
                            continue;
                        }
                        unquoted.push(chars[i].1);
                        i += 1;
                    }
                    if i >= n {
                        return None; // unterminated "
                    }
                    i += 1; // skip closing "
                }
                '\'' => {
                    i += 1;
                    while i < n && chars[i].1 != '\'' {
                        unquoted.push(chars[i].1);
                        i += 1;
                    }
                    if i >= n {
                        return None; // unterminated '
                    }
                    i += 1; // skip closing '
                }
                c => {
                    unquoted.push(c);
                    i += 1;
                }
            }
        }

        let end = if i < n { chars[i].0 } else { s.len() };
        tokens.push(Token {
            raw: s[start..end].to_string(),
            unquoted,
        });
    }

    Some(tokens)
}

/// Truncate a string to at most `max` bytes for log-output safety, backing
/// up to the nearest character boundary so multi-byte characters are never
/// split (slicing mid-character would panic).
pub(crate) fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut cut = max;
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    let mut out = s[..cut].to_string();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_round_trips_non_ascii_content() {
        let cmd = "claude -p \"héllo wörld 日本語のプロンプト émoji 🎉\" --verbose";
        let tokens = tokenize(cmd).expect("must tokenize");
        assert_eq!(tokens[0].unquoted, "claude");
        assert_eq!(tokens[1].unquoted, "-p");
        assert_eq!(
            tokens[2].unquoted, "héllo wörld 日本語のプロンプト émoji 🎉",
            "non-ASCII prompt must round-trip intact, not be decoded byte-wise"
        );
        assert_eq!(
            tokens[2].raw, "\"héllo wörld 日本語のプロンプト émoji 🎉\"",
            "raw slice must carry the original text including quotes"
        );
        assert_eq!(tokens[3].unquoted, "--verbose");
    }

    #[test]
    fn tokenize_round_trips_non_ascii_in_single_quotes_and_bare_tokens() {
        let tokens = tokenize("claude -p 'ünïcode' --modèl über").expect("must tokenize");
        assert_eq!(tokens[2].unquoted, "ünïcode");
        assert_eq!(tokens[3].unquoted, "--modèl");
        assert_eq!(tokens[4].unquoted, "über");
        assert_eq!(tokens[4].raw, "über");
    }

    #[test]
    fn tokenize_preserves_escapes_verbatim_around_non_ascii() {
        let tokens = tokenize(r#"claude -p "línea1\nlínea2""#).expect("must tokenize");
        assert_eq!(tokens[2].unquoted, r"línea1\nlínea2");
    }

    #[test]
    fn truncate_backs_up_to_a_char_boundary() {
        // Each character here is 3 bytes; cutting at byte 4 lands mid-character
        // and must back up instead of panicking.
        let s = "日本語";
        assert_eq!(truncate(s, 4), "日…");
        // Exact boundary is kept as-is.
        assert_eq!(truncate(s, 6), "日本…");
        // No truncation needed.
        assert_eq!(truncate(s, 9), "日本語");
        // Degenerate: max smaller than the first character.
        assert_eq!(truncate(s, 2), "…");
        assert_eq!(truncate(s, 0), "…");
    }

    #[test]
    fn truncate_ascii_behaviour_unchanged() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 5), "hello…");
    }
}
