//! FTS5 query construction (ADR-005).
//!
//! User input is never concatenated into SQL. The query string is split
//! into whitespace-separated terms, each term is wrapped in a quoted FTS5
//! phrase-prefix literal (`"term"*` — embedded quotes doubled) and the
//! terms are joined with spaces, FTS5's implicit AND. The prefix star makes
//! `postgres` match `PostgreSQL` and keeps exact error fragments
//! searchable; malformed-query injection is impossible by construction.

use crate::{Error, Result};

/// Build a safe FTS5 MATCH expression from raw user input.
pub fn build_match_query(query: &str) -> Result<String> {
    let terms: Vec<String> = query
        .split_whitespace()
        .filter(|t| t.chars().any(|c| c.is_alphanumeric()))
        .map(|t| format!("\"{}\"*", t.replace('"', "\"\"")))
        .collect();

    if terms.is_empty() {
        return Err(Error::InvalidInput(
            "search query must contain at least one searchable word".into(),
        ));
    }
    Ok(terms.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_terms_as_quoted_and() {
        assert_eq!(
            build_match_query("postgres connection pool").unwrap(),
            "\"postgres\"* \"connection\"* \"pool\"*"
        );
    }

    #[test]
    fn quotes_are_escaped() {
        assert_eq!(
            build_match_query("relation \"orders\"").unwrap(),
            "\"relation\"* \"\"\"orders\"\"\"*"
        );
    }

    #[test]
    fn punctuation_only_query_is_rejected() {
        assert!(matches!(
            build_match_query(" *** :: ... "),
            Err(Error::InvalidInput(_))
        ));
    }

    #[test]
    fn empty_query_is_rejected() {
        assert!(matches!(
            build_match_query("   "),
            Err(Error::InvalidInput(_))
        ));
    }

    #[test]
    fn error_message_fragments_stay_searchable() {
        // Parentheses, colons and dots survive quoting unchanged.
        assert_eq!(
            build_match_query("ERROR: relation \"orders\" does not exist (line 42)").unwrap(),
            "\"ERROR:\"* \"relation\"* \"\"\"orders\"\"\"* \"does\"* \"not\"* \"exist\"* \"(line\"* \"42)\"*"
        );
    }
}
