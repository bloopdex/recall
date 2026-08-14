//! Configuration.
//!
//! Recall is local-first and intentionally has almost no configuration:
//! one environment variable (`RECALL_DB_PATH`) overrides the default
//! database location, and the CLI `--db` flag overrides both.

use std::path::PathBuf;

use crate::{Error, Result};

const ENV_DB_PATH: &str = "RECALL_DB_PATH";
const DEFAULT_DB_FILE: &str = "recall.db";

/// Runtime configuration resolved from CLI flags and the environment.
#[derive(Debug, Clone)]
pub struct Config {
    /// Path to the SQLite database file.
    pub db_path: PathBuf,
}

impl Config {
    /// Resolve configuration. Precedence: `--db` flag > `RECALL_DB_PATH`
    /// env var > platform data directory.
    pub fn resolve(db_flag: Option<PathBuf>) -> Result<Self> {
        let db_path = if let Some(path) = db_flag {
            path
        } else if let Ok(path) = std::env::var(ENV_DB_PATH) {
            if path.trim().is_empty() {
                return Err(Error::Config(format!("{ENV_DB_PATH} is set but empty")));
            }
            PathBuf::from(path)
        } else {
            default_db_path()?
        };
        Ok(Self { db_path })
    }
}

/// Default database location:
/// `%LOCALAPPDATA%\recall\recall.db` on Windows,
/// `~/.local/share/recall/recall.db` (XDG) elsewhere,
/// falling back to `~/.recall/recall.db`.
fn default_db_path() -> Result<PathBuf> {
    if cfg!(windows) {
        if let Some(base) = std::env::var_os("LOCALAPPDATA") {
            return Ok(PathBuf::from(base).join("recall").join(DEFAULT_DB_FILE));
        }
    }
    if let Some(base) = std::env::var_os("XDG_DATA_HOME") {
        return Ok(PathBuf::from(base).join("recall").join(DEFAULT_DB_FILE));
    }
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        return Ok(PathBuf::from(home).join(".recall").join(DEFAULT_DB_FILE));
    }
    Err(Error::Config(
        "cannot determine a home directory for the Recall database; set RECALL_DB_PATH".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_flag_wins_over_env() {
        // `--db` flag has highest precedence.
        std::env::set_var(ENV_DB_PATH, "C:\\fake\\env.db");
        let cfg = Config::resolve(Some(PathBuf::from("C:\\fake\\flag.db"))).unwrap();
        assert_eq!(cfg.db_path, PathBuf::from("C:\\fake\\flag.db"));
        std::env::remove_var(ENV_DB_PATH);
    }

    #[test]
    fn empty_env_value_is_a_config_error() {
        std::env::set_var(ENV_DB_PATH, "  ");
        let err = Config::resolve(None).unwrap_err();
        assert!(matches!(err, Error::Config(_)));
        std::env::remove_var(ENV_DB_PATH);
    }
}
