//! CLI definition and dispatch (ADR-007).
//!
//! clap-derive keeps the surface declarative so future subcommands
//! (`edit`, `export`, ...) plug in without touching the dispatch core.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::application;
use crate::config::Config;
use crate::infrastructure::database::Db;

#[derive(Parser, Debug)]
#[command(
    name = "recall",
    version,
    about = "Personal engineering solution memory — a local-first CLI that remembers how you solved engineering problems.",
    after_help = "Examples:\n  recall capture\n  recall search \"postgres connection pool\"\n  echo \"sqlite database is locked\" | recall capture --solution \"set busy_timeout\"\n\nEverything stays local: no network, no telemetry."
)]
pub struct Cli {
    /// Path to the SQLite database file (overrides RECALL_DB_PATH).
    #[arg(long, global = true, value_name = "PATH")]
    pub db: Option<PathBuf>,

    /// Verbose logging.
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Capture a solution memory (interactive, by flags, or piped stdin).
    Capture(Box<CaptureArgs>),

    /// Edit user-provided fields of an existing memory.
    Edit(EditArgs),

    /// Search past solutions by keyword (SQLite FTS5).
    Search {
        /// Search terms; e.g. recall search "postgres connection pool".
        #[arg(trailing_var_arg = true, required = true, value_name = "QUERY")]
        query: Vec<String>,
    },

    /// List recent memories, newest first.
    List {
        /// Maximum entries to show.
        #[arg(long, default_value_t = 20, value_name = "N")]
        limit: usize,
    },
}

#[derive(Args, Debug, Clone, Default)]
pub struct CaptureArgs {
    /// The problem you hit (required one way or another).
    #[arg(long, value_name = "TEXT")]
    pub problem: Option<String>,

    /// How you solved it (required one way or another).
    #[arg(long, value_name = "TEXT")]
    pub solution: Option<String>,

    /// The exact symptom/error message.
    #[arg(long, value_name = "TEXT")]
    pub error: Option<String>,

    /// Environment/versions/state at the time of the incident.
    #[arg(long, value_name = "TEXT")]
    pub context: Option<String>,

    /// Commands run and files inspected while diagnosing.
    #[arg(long, value_name = "TEXT")]
    pub investigation: Option<String>,

    /// Why it happened.
    #[arg(long, value_name = "TEXT")]
    pub root_cause: Option<String>,

    /// How you verified the fix.
    #[arg(long, value_name = "TEXT")]
    pub verification: Option<String>,

    /// Environment metadata (versions, flags).
    #[arg(long, value_name = "TEXT")]
    pub environment: Option<String>,

    /// Free-form elaboration of the solution.
    #[arg(long, value_name = "TEXT")]
    pub explanation: Option<String>,

    /// Override automatic project detection.
    #[arg(long, value_name = "NAME")]
    pub project: Option<String>,

    /// Read the problem from stdin (instead of interactive prompts).
    #[arg(long)]
    pub stdin: bool,

    /// Capture even when a near-identical memory exists (overrides the
    /// deduplication skip, see ADR-0011).
    #[arg(long)]
    pub force: bool,
}

#[derive(Args, Debug, Clone, Default)]
pub struct EditArgs {
    /// Id of the memory to edit.
    pub id: i64,

    /// Replace the problem (required field — cannot be cleared).
    #[arg(long, value_name = "TEXT")]
    pub problem: Option<String>,

    /// Replace the solution (required field — cannot be cleared).
    #[arg(long, value_name = "TEXT")]
    pub solution: Option<String>,

    /// Replace the error text; empty text clears the field.
    #[arg(long, value_name = "TEXT")]
    pub error: Option<String>,

    /// Replace the context; empty text clears the field.
    #[arg(long, value_name = "TEXT")]
    pub context: Option<String>,

    /// Replace the investigation notes; empty text clears the field.
    #[arg(long, value_name = "TEXT")]
    pub investigation: Option<String>,

    /// Replace the root cause; empty text clears the field.
    #[arg(long, value_name = "TEXT")]
    pub root_cause: Option<String>,

    /// Replace the verification notes; empty text clears the field.
    #[arg(long, value_name = "TEXT")]
    pub verification: Option<String>,

    /// Replace the environment metadata; empty text clears the field.
    #[arg(long, value_name = "TEXT")]
    pub environment: Option<String>,

    /// Replace the explanation; empty text clears the field.
    #[arg(long, value_name = "TEXT")]
    pub explanation: Option<String>,
}

/// Parse, resolve configuration, and dispatch.
pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    crate::observability::init(cli.verbose);

    let config = Config::resolve(cli.db.clone())?;
    let mut db = Db::open(&config.db_path)?;
    let cwd = std::env::current_dir()?;

    match cli.command {
        Command::Capture(args) => {
            let outcome = application::capture::run(&mut db, args.as_ref(), &cwd)?;
            let project = outcome.project.unwrap_or_else(|| "no project".to_string());
            if outcome.skipped {
                println!(
                    "Skipped: near-identical memory #{} already exists (project: {project}). Use --force to capture anyway.",
                    outcome.id
                );
            } else {
                println!("Captured #{} (project: {project})", outcome.id);
            }
        }
        Command::Edit(args) => {
            application::edit::run(&mut db, &args)?;
            println!("Edited #{}", args.id);
        }
        Command::Search { query } => {
            application::search::run(&db, &query.join(" "), application::search::DEFAULT_LIMIT)?;
        }
        Command::List { limit } => {
            application::list::run(&db, limit)?;
        }
    }
    Ok(())
}
