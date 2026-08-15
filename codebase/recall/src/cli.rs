//! CLI definition and dispatch (ADR-007).
//!
//! clap-derive keeps the surface declarative so future subcommands
//! (`edit`, `export`, ...) plug in without touching the dispatch core.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::application;
use crate::config::Config;
use crate::infrastructure::database::Db;
use crate::infrastructure::shell::Shell;

#[derive(Parser, Debug)]
#[command(
    name = "recall",
    version,
    about = "Personal engineering solution memory — a local-first CLI that remembers how you solved engineering problems.",
    after_help = "Examples:\n  recall capture\n  recall search \"postgres connection pool\"\n  echo \"sqlite database is locked\" | recall capture --solution \"set busy_timeout\"\n  recall shell install\n  recall git install\n\nEverything stays local: no network, no telemetry."
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

    /// Search past solutions (hybrid: keyword + semantic).
    Search {
        /// Search terms; e.g. recall search "postgres connection pool".
        /// Trailing-var-arg: place flags such as --explain BEFORE the query.
        #[arg(trailing_var_arg = true, required = true, value_name = "QUERY")]
        query: Vec<String>,

        /// Show the per-engine ranking signals behind each result
        /// (put this flag before the query).
        #[arg(long)]
        explain: bool,
    },

    /// Manage the semantic-search layer (model + embeddings).
    #[command(subcommand)]
    Embeddings(EmbeddingsCommand),

    /// List recent memories, newest first.
    List {
        /// Maximum entries to show.
        #[arg(long, default_value_t = 20, value_name = "N")]
        limit: usize,
    },

    /// Manage the shell integration (failure-context capture + the
    /// `recall` dispatch function).
    #[command(subcommand)]
    Shell(ShellCommand),

    /// Manage the git post-commit hook integration.
    #[command(subcommand)]
    Git(GitCommand),
}

/// `recall shell <subcommand>`; no subcommand defaults to `status`.
#[derive(Subcommand, Debug)]
pub enum ShellCommand {
    /// Print the integration snippet for a shell (append it yourself, or
    /// use `recall shell install`).
    Init {
        /// Which shell to generate the snippet for
        /// (default: auto-detected from the environment).
        #[arg(long, value_name = "SHELL")]
        shell: Option<Shell>,
    },
    /// Append the recall block to the shell startup file (idempotent,
    /// reversible — never touches content outside the marked block).
    Install {
        #[arg(long, value_name = "SHELL")]
        shell: Option<Shell>,
    },
    /// Remove the recall block from the shell startup file.
    Uninstall {
        #[arg(long, value_name = "SHELL")]
        shell: Option<Shell>,
    },
    /// Report whether the integration block is installed.
    Status {
        #[arg(long, value_name = "SHELL")]
        shell: Option<Shell>,
    },
}

/// `recall git <subcommand>`; no subcommand defaults to `status`.
#[derive(Subcommand, Debug)]
pub enum GitCommand {
    /// Install the post-commit hook (never overwrites an existing user
    /// hook; use --append to add the recall block to it).
    Install {
        #[arg(long)]
        append: bool,
    },
    /// Remove the recall hook (existing user hook content is preserved).
    Uninstall,
    /// Report whether the recall hook is installed in this repository.
    Status,
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

    /// Capture from the shell failure snapshot recorded by the prompt hook
    /// (`recall shell install`): pre-fills the problem with the failed
    /// command and its exit code. Piped stdin supplies the error output.
    #[arg(long, conflicts_with = "from_git")]
    pub from_shell: bool,

    /// Capture after a commit (used by the post-commit hook): pre-fills
    /// the problem from the commit subject and records the commit's files.
    /// Skips silently when there is no interactive terminal.
    #[arg(long, conflicts_with = "from_shell")]
    pub from_git: bool,
}

#[derive(Subcommand, Debug)]
pub enum EmbeddingsCommand {
    /// Show model presence and embedding coverage.
    Status,
    /// Embed all memories that are missing or have stale embeddings.
    Build,
    /// One-time model download (the only command that uses the network).
    Download,
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
///
/// The database is opened only for subcommands that need it — shell/git
/// management must work even when no database exists yet (ADR-0017/0020).
pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    crate::observability::init(cli.verbose);

    let cwd = std::env::current_dir()?;

    match cli.command {
        Command::Capture(args) => {
            let config = Config::resolve(cli.db.clone())?;
            let mut db = Db::open(&config.db_path)?;
            let outcome = application::capture::run(&mut db, args.as_ref(), &cwd)?;
            match outcome {
                application::capture::CaptureOutcome::Captured { id, project } => {
                    println!(
                        "Captured #{} (project: {})",
                        id,
                        project.unwrap_or_else(|| "no project".to_string())
                    );
                }
                application::capture::CaptureOutcome::SkippedDuplicate { id, project } => {
                    println!(
                        "Skipped: near-identical memory #{} already exists (project: {}). Use --force to capture anyway.",
                        id,
                        project.unwrap_or_else(|| "no project".to_string())
                    );
                }
                application::capture::CaptureOutcome::Declined { reason } => {
                    println!("{reason}");
                }
            }
        }
        Command::Edit(args) => {
            let config = Config::resolve(cli.db.clone())?;
            let mut db = Db::open(&config.db_path)?;
            application::edit::run(&mut db, &args)?;
            println!("Edited #{}", args.id);
        }
        Command::Search { query, explain } => {
            let config = Config::resolve(cli.db.clone())?;
            let db = Db::open(&config.db_path)?;
            application::search::run(
                &db,
                &query.join(" "),
                application::search::DEFAULT_LIMIT,
                explain,
            )?;
        }
        Command::List { limit } => {
            let config = Config::resolve(cli.db.clone())?;
            let db = Db::open(&config.db_path)?;
            application::list::run(&db, limit)?;
        }
        Command::Embeddings(command) => {
            let config = Config::resolve(cli.db.clone())?;
            let mut db = Db::open(&config.db_path)?;
            match command {
                EmbeddingsCommand::Status => application::embeddings::status(&db)?,
                EmbeddingsCommand::Build => application::embeddings::build(&mut db)?,
                EmbeddingsCommand::Download => application::embeddings::download()?,
            }
        }
        Command::Shell(command) => application::shell::run(&command)?,
        Command::Git(command) => application::git_hooks::run(&command)?,
    }
    Ok(())
}
