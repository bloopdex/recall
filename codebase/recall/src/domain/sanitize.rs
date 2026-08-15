//! Conservative secret detection and redaction for auto-captured shell and
//! git context (ADR-0018).
//!
//! Recall captures things the user did not type into a prompt (a failed
//! command line, piped error output, a commit subject). Those may embed
//! secrets: `--password=...`, `Bearer` tokens, AWS key ids, basic-auth
//! URLs, PEM blocks. Before anything is persisted, captured text runs
//! through this module: matches are replaced with `<redacted>`, the result
//! is shown to the user, and a confirmation gate (see `application::capture`)
//! requires explicit approval when anything was redacted.
//!
//! Detection is deliberately **conservative and explainable**: a fixed set
//! of high-signal patterns, not a heuristic classifier. It does NOT claim
//! to catch arbitrary secrets — that is impossible — and the documentation
//! states this limitation. The guarantee Recall makes is narrower: common
//! secret shapes never reach the database silently.

/// Maximum length of a command line stored from the shell snapshot.
/// Longer commands are truncated with a marker (ADR-0018).
pub const MAX_COMMAND_LEN: usize = 1000;

/// Maximum length of piped/auto-captured text (error output). Longer text
/// is truncated with a marker.
pub const MAX_CAPTURED_TEXT_LEN: usize = 10_000;

/// Key names recognized in `key=value` / `key value` assignments. A key
/// matches when it EQUALS one of these or ENDS with `_name`/`-name`, so
/// prefixed forms like `DB_PASSWORD` or `AWS_ACCESS_KEY_ID` are caught.
const SECRET_KEYS: &[&str] = &[
    "password",
    "passwd",
    "pwd",
    "token",
    "secret",
    "api_key",
    "apikey",
    "api-key",
    "access_key",
    "access-key",
    "access_key_id",
    "access-key-id",
    "access_token",
    "access-token",
    "auth_token",
    "auth-token",
    "client_secret",
    "client-secret",
    "private_key",
    "private-key",
];

/// Flag names recognized in `--flag value` / `--flag=value` form.
const SECRET_FLAGS: &[&str] = &[
    "password",
    "passwd",
    "token",
    "secret",
    "api-key",
    "apikey",
    "access-key",
    "access-token",
    "auth-token",
    "client-secret",
];

/// A word boundary is whitespace or one of these delimiters.
fn is_boundary(c: char) -> bool {
    c.is_whitespace()
        || c == '"'
        || c == '\''
        || c == ';'
        || c == '&'
        || c == '|'
        || c == '<'
        || c == '>'
        || c == '='
        || c == ','
        || c == '('
        || c == '['
}

fn is_value_end(c: char) -> bool {
    c.is_whitespace() || c == '"' || c == '\'' || c == ';' || c == '&' || c == '|'
}

/// Result of a sanitization pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitizeReport {
    /// The text with every detected secret replaced by `<redacted>`.
    pub sanitized: String,
    /// How many secret patterns were redacted (0 = nothing detected).
    pub redactions: usize,
}

/// Scan `text` for common secret patterns and replace matches with
/// `<redacted>`. Deterministic and side-effect free.
pub fn sanitize(text: &str) -> SanitizeReport {
    let mut out = String::with_capacity(text.len());
    let mut redactions = 0usize;
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < bytes.len() {
        let c = bytes[i];
        let start = i;
        let boundary_before = i == 0 || is_boundary(bytes[i - 1]);

        // PEM blocks: `-----BEGIN ... PRIVATE KEY-----` ... `-----END ...-----`
        if boundary_before
            && text[i..].starts_with("-----BEGIN")
            && text[i..].contains("PRIVATE KEY-----")
        {
            if let Some(end_rel) = text[i..].find("-----END") {
                // Redact from BEGIN to just after the END line's `-----`.
                let after_end = text[i + end_rel..]
                    .find("-----")
                    .map(|p| p + 5)
                    .unwrap_or(9);
                out.push_str("<redacted>");
                redactions += 1;
                i = i + end_rel + after_end;
                continue;
            }
        }

        // `Bearer <token>`
        if boundary_before && text[i..].len() >= 6 && text[i..][..6].eq_ignore_ascii_case("bearer")
        {
            let next = bytes[i + 6];
            if next == ' ' || next == '\t' {
                let mut j = i + 7;
                while j < bytes.len() && !is_value_end(bytes[j]) {
                    j += 1;
                }
                if j > i + 7 {
                    out.push_str("Bearer <redacted>");
                    redactions += 1;
                    i = j;
                    continue;
                }
            }
        }

        // AWS access key ids: `AKIA` + 16 uppercase alphanumerics.
        if text[i..].starts_with("AKIA") && i + 20 <= text.len() {
            let tail = &text[i + 4..i + 20];
            if tail
                .chars()
                .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
            {
                out.push_str("<redacted>");
                redactions += 1;
                i += 20;
                continue;
            }
        }

        // Basic-auth URLs: `scheme://user:pass@host` — redact the password.
        if c.is_ascii_alphabetic() && text[i..].contains("://") {
            if let Some(scheme_end) = text[i..].find("://") {
                let user_start = i + scheme_end + 3;
                let user_part = &text[user_start..];
                if let Some(colon) = user_part.find(':') {
                    let colon_abs = user_start + colon;
                    if colon_abs > user_start {
                        let pass_start = colon_abs + 1;
                        if let Some(at) = user_part[colon + 1..].find('@') {
                            let pass_end = pass_start + at;
                            if pass_end > pass_start && pass_end <= text.len() {
                                out.push_str(&text[i..colon_abs + 1]);
                                out.push_str("<redacted>");
                                redactions += 1;
                                i = pass_end;
                                continue;
                            }
                        }
                    }
                }
            }
        }

        // `Authorization:` headers — the value runs to the closing quote,
        // a newline, or the end of the text (header values contain spaces,
        // e.g. `Authorization: Bearer abc.def`).
        if boundary_before
            && text[i..].len() >= 15
            && text[i..][..14].eq_ignore_ascii_case("authorization:")
        {
            let mut j = i + 14;
            while j < bytes.len() && bytes[j].is_whitespace() {
                j += 1;
            }
            let value_start = j;
            while j < bytes.len()
                && bytes[j] != '"'
                && bytes[j] != '\''
                && bytes[j] != '\n'
                && bytes[j] != '\r'
            {
                j += 1;
            }
            if j > value_start {
                out.push_str("Authorization: <redacted>");
                redactions += 1;
                i = j;
                continue;
            }
        }

        // `--secret-flag value` / `--secret-flag=value`.
        if boundary_before && c == '-' && i + 1 < bytes.len() && bytes[i + 1] == '-' {
            let flag_text = &text[i + 2..];
            if let Some(flag_match) = SECRET_FLAGS.iter().find(|flag| {
                flag_text.len() >= flag.len() && flag_text[..flag.len()].eq_ignore_ascii_case(flag)
            }) {
                let after_flag = i + 2 + flag_match.len();
                let after = bytes.get(after_flag);
                if after == Some(&'=') {
                    let mut j = after_flag + 1;
                    while j < bytes.len() && !is_value_end(bytes[j]) {
                        j += 1;
                    }
                    if j > after_flag + 1 {
                        out.push_str(&text[i..=after_flag]);
                        out.push_str("<redacted>");
                        redactions += 1;
                        i = j;
                        continue;
                    }
                } else if after.is_some_and(|ch| ch.is_whitespace()) {
                    let mut j = after_flag;
                    while j < bytes.len() && bytes[j].is_whitespace() {
                        j += 1;
                    }
                    let value_start = j;
                    while j < bytes.len() && !is_value_end(bytes[j]) {
                        j += 1;
                    }
                    if j > value_start {
                        out.push_str(&text[i..after_flag]);
                        out.push_str(" <redacted>");
                        redactions += 1;
                        i = j;
                        continue;
                    }
                }
            }
        }

        // `key=value` / `key = value` / `key: value` with a secret key name.
        if boundary_before && (c.is_ascii_alphabetic() || c == '_') {
            let mut j = i + 1;
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric() || bytes[j] == '_' || bytes[j] == '-')
            {
                j += 1;
            }
            let key = &text[i..j];
            let key_lower = key.to_ascii_lowercase();
            let is_secret_key = SECRET_KEYS.iter().any(|k| {
                key_lower == *k
                    || key_lower.ends_with(&format!("_{k}"))
                    || key_lower.ends_with(&format!("-{k}"))
            });
            if is_secret_key {
                let mut k = j;
                while k < bytes.len() && bytes[k].is_whitespace() {
                    k += 1;
                }
                let sep = bytes.get(k).copied();
                if sep == Some('=') || sep == Some(':') {
                    let mut v = k + 1;
                    while v < bytes.len() && bytes[v].is_whitespace() {
                        v += 1;
                    }
                    let value_start = v;
                    while v < bytes.len() && !is_value_end(bytes[v]) {
                        v += 1;
                    }
                    if v > value_start {
                        out.push_str(&text[i..=k]);
                        out.push_str("<redacted>");
                        redactions += 1;
                        i = v;
                        continue;
                    }
                }
            }
        }

        out.push(c);
        i = start + 1;
    }

    SanitizeReport {
        sanitized: out,
        redactions,
    }
}

/// Truncate a captured command line to [`MAX_COMMAND_LEN`] with a marker.
pub fn truncate_command(command: &str) -> String {
    let trimmed = command.trim();
    if trimmed.chars().count() <= MAX_COMMAND_LEN {
        return trimmed.to_string();
    }
    let cut: String = trimmed.chars().take(MAX_COMMAND_LEN).collect();
    format!("{cut} ... (truncated)")
}

/// Truncate piped/auto-captured text to [`MAX_CAPTURED_TEXT_LEN`] with a
/// marker.
pub fn truncate_text(text: &str) -> String {
    if text.chars().count() <= MAX_CAPTURED_TEXT_LEN {
        return text.to_string();
    }
    let cut: String = text.chars().take(MAX_CAPTURED_TEXT_LEN).collect();
    format!("{cut}\n... (output truncated)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_password_value_is_redacted() {
        let r = sanitize("npm login --password hunter2 --registry x");
        assert_eq!(r.redactions, 1);
        assert_eq!(r.sanitized, "npm login --password <redacted> --registry x");
    }

    #[test]
    fn flag_token_equals_value_is_redacted() {
        let r = sanitize("gh auth --token=ghp_abc123xyz");
        assert_eq!(r.redactions, 1);
        assert_eq!(r.sanitized, "gh auth --token=<redacted>");
    }

    #[test]
    fn key_equals_value_with_secret_key_is_redacted() {
        let r = sanitize("export DB_PASSWORD=hunter2");
        assert_eq!(r.redactions, 1);
        assert_eq!(r.sanitized, "export DB_PASSWORD=<redacted>");
    }

    #[test]
    fn key_colon_value_is_redacted() {
        // The Authorization pattern wins the scan and redacts the whole
        // value, Bearer included — one redaction, one visible header.
        let r = sanitize("authorization: Bearer xyz");
        assert_eq!(r.redactions, 1);
        assert_eq!(r.sanitized, "Authorization: <redacted>");
    }

    #[test]
    fn bearer_token_is_redacted() {
        let r = sanitize("curl -H \"Authorization: Bearer abc.def.ghi\" https://api");
        assert_eq!(r.redactions, 1);
        assert_eq!(
            r.sanitized,
            "curl -H \"Authorization: <redacted>\" https://api"
        );
    }

    #[test]
    fn prefixed_secret_keys_are_redacted() {
        let r = sanitize("export DB_PASSWORD=hunter2 AWS_ACCESS_KEY_ID=x");
        assert_eq!(r.redactions, 2);
        assert_eq!(
            r.sanitized,
            "export DB_PASSWORD=<redacted> AWS_ACCESS_KEY_ID=<redacted>"
        );
    }

    #[test]
    fn aws_access_key_assignment_is_fully_redacted() {
        // The key=value rule wins and redacts the whole assignment.
        let r = sanitize("env AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE");
        assert_eq!(r.redactions, 1);
        assert_eq!(r.sanitized, "env AWS_ACCESS_KEY_ID=<redacted>");
    }

    #[test]
    fn bare_aws_access_key_is_redacted() {
        let r = sanitize("seen AKIAIOSFODNN7EXAMPLE in the log");
        assert_eq!(r.redactions, 1);
        assert!(!r.sanitized.contains("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn aws_access_key_after_equals_flag_is_redacted() {
        let r = sanitize("--access-key=AKIAIOSFODNN7EXAMPLE");
        assert_eq!(r.redactions, 1);
        assert!(!r.sanitized.contains("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn standalone_bearer_token_is_redacted() {
        let r = sanitize("curl -H Bearer abc.def.ghi https://api");
        assert_eq!(r.redactions, 1);
        assert_eq!(r.sanitized, "curl -H Bearer <redacted> https://api");
    }

    #[test]
    fn basic_auth_url_redacts_only_the_password() {
        let r = sanitize("git clone https://alice:hunter2@github.com/x/y.git");
        assert_eq!(r.redactions, 1);
        assert_eq!(
            r.sanitized,
            "git clone https://alice:<redacted>@github.com/x/y.git"
        );
    }

    #[test]
    fn pem_block_is_fully_redacted() {
        let text = "echo '-----BEGIN RSA PRIVATE KEY-----\nMIIE...\n-----END RSA PRIVATE KEY-----'";
        let r = sanitize(text);
        assert_eq!(r.redactions, 1);
        assert!(!r.sanitized.contains("MIIE"));
        assert!(r.sanitized.contains("<redacted>"));
    }

    #[test]
    fn clean_text_passes_through_untouched() {
        let r = sanitize("cargo test --release --lib");
        assert_eq!(r.redactions, 0);
        assert_eq!(r.sanitized, "cargo test --release --lib");
    }

    #[test]
    fn non_secret_key_value_pairs_survive() {
        let r = sanitize("npm run build --mode=production");
        assert_eq!(r.redactions, 0);
        assert_eq!(r.sanitized, "npm run build --mode=production");
    }

    #[test]
    fn password_word_alone_is_not_redacted() {
        // "password" without a value/assignment is content, not a secret.
        let r = sanitize("read the password from the vault");
        assert_eq!(r.redactions, 0);
    }

    #[test]
    fn command_truncation_keeps_a_marker() {
        let long = "x".repeat(2000);
        let t = truncate_command(&long);
        assert!(t.len() < 2000);
        assert!(t.ends_with("... (truncated)"));
    }

    #[test]
    fn command_truncation_passes_short_commands_through() {
        assert_eq!(truncate_command("  git status  "), "git status");
    }

    #[test]
    fn text_truncation_keeps_a_marker() {
        let long = "y".repeat(20_000);
        let t = truncate_text(&long);
        assert!(t.ends_with("\n... (output truncated)"));
        assert!(t.len() < 20_000);
    }
}
