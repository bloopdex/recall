//! CLI definition and dispatch (ADR-007).
//!
//! clap-derive keeps the surface declarative so future subcommands
//! (`edit`, `export`, ...) plug in without touching the dispatch core.

use std::io::IsTerminal;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::application;
use crate::config::Config;
use crate::infrastructure::database::Db;
use crate::infrastructure::shell::Shell;
use crate::ui;

#[derive(Parser, Debug)]
#[command(
    name = "recall",
    version,
    about = "Personal engineering solution memory — a local-first CLI that remembers how you solved engineering problems.",
    after_help = "Examples:\n  recall capture\n  recall search \"postgres connection pool\"\n  echo \"sqlite database is locked\" | recall capture --solution \"set busy_timeout\"\n  recall shell install\n  recall git install\n\nCommand groups:\n  capture, search, list               capture & search\n  edit, archive, unarchive, delete    manage memories\n  projects, export, import            projects & transfer\n  shell, git                          integrations\n  check, embeddings, version          maintenance\n\nEverything stays local: no network, no telemetry.\nSet RECALL_PLAIN=1 for plain output without symbols."
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
    /// Capture how you solved a problem (interactive, flags, or piped stdin).
    #[command(display_order = 1)]
    Capture(Box<CaptureArgs>),

    /// Edit user-provided fields of an existing memory.
    #[command(display_order = 4)]
    Edit(EditArgs),

    /// Search past solutions (hybrid: keyword + semantic).
    #[command(display_order = 2)]
    Search {
        /// Search terms; e.g. recall search "postgres connection pool".
        /// Trailing-var-arg: place flags such as --explain BEFORE the query.
        #[arg(trailing_var_arg = true, required = true, value_name = "QUERY")]
        query: Vec<String>,

        /// Restrict results to one project label (case-insensitive exact
        /// match; memories without a project never match). Default:
        /// search across all projects.
        #[arg(long, value_name = "NAME")]
        project: Option<String>,

        /// Include archived memories in the results (default: active only).
        #[arg(long)]
        include_archived: bool,

        /// Show the per-engine ranking signals behind each result
        /// (put this flag before the query).
        #[arg(long)]
        explain: bool,
    },

    /// Manage the semantic-search layer (model + embeddings).
    #[command(subcommand, display_order = 13)]
    Embeddings(EmbeddingsCommand),

    /// List recent memories, newest first.
    #[command(display_order = 3)]
    List {
        /// Maximum entries to show.
        #[arg(long, default_value_t = 20, value_name = "N")]
        limit: usize,

        /// Restrict to one project label (case-insensitive exact match).
        #[arg(long, value_name = "NAME")]
        project: Option<String>,

        /// List archived memories instead of active ones.
        #[arg(long)]
        archived: bool,
    },

    /// Overview of the projects in the store: labels, memory counts,
    /// last capture time (the current project is marked with *).
    #[command(display_order = 8)]
    Projects,

    /// Run read-only consistency checks over the database (structural
    /// integrity, index sync, lifecycle validity). Exits non-zero when
    /// problems are found. Never repairs anything — see the recovery
    /// hints in the report.
    #[command(display_order = 14)]
    Check,

    /// Print the four versioned surfaces: application, database schema,
    /// export format, embedding model (ADR-0031). Does not create a
    /// database.
    #[command(display_order = 15)]
    Version,

    /// Move a memory out of active search (recoverable: `recall unarchive`).
    #[command(display_order = 5)]
    Archive {
        /// Id of the memory to archive.
        id: i64,
    },

    /// Move an archived memory back into active search.
    #[command(display_order = 6)]
    Unarchive {
        /// Id of the memory to unarchive.
        id: i64,
    },

    /// Permanently delete a memory (or every memory of one project).
    #[command(display_order = 7)]
    Delete {
        /// Id of the memory to delete.
        #[arg(required_unless_present = "project", value_name = "ID")]
        id: Option<i64>,

        /// Delete every memory with this project label.
        #[arg(long, value_name = "NAME")]
        project: Option<String>,

        /// Confirm the deletion in non-interactive contexts
        /// (a terminal prompts instead).
        #[arg(long)]
        yes: bool,
    },

    /// Export memories as portable JSON (secrets redacted by default).
    #[command(display_order = 9)]
    Export {
        /// Write to a file instead of stdout.
        #[arg(long, value_name = "FILE")]
        path: Option<PathBuf>,

        /// Export raw field text without redaction (opt-in).
        #[arg(long)]
        include_secrets: bool,
    },

    /// Import a Recall export (duplicates skipped unless --force).
    #[command(display_order = 10)]
    Import {
        /// Path of the export file to import.
        #[arg(value_name = "FILE")]
        path: PathBuf,

        /// Import entries even when a memory with the same project and
        /// problem already exists.
        #[arg(long)]
        force: bool,
    },

    /// Manage the shell integration (failure-context capture + the
    /// `recall` dispatch function).
    #[command(subcommand, display_order = 11)]
    Shell(ShellCommand),

    /// Manage the git post-commit hook integration.
    #[command(subcommand, display_order = 12)]
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
    #[arg(long, conflicts_with = "from_git", conflicts_with = "from_ci")]
    pub from_shell: bool,

    /// Capture after a commit (used by the post-commit hook): pre-fills
    /// the problem from the commit subject and records the commit's files.
    /// Skips silently when there is no interactive terminal.
    #[arg(long, conflicts_with = "from_shell", conflicts_with = "from_ci")]
    pub from_git: bool,

    /// Capture a CI failure (ADR-0030): runs inside an opt-in GitHub
    /// Actions failure step (`if: failure()`). Pre-fills the problem from
    /// the whitelisted GITHUB_* environment (workflow/job/event — no run
    /// id, so repeated failures deduplicate); piped stdin supplies the
    /// log tail as the error. A `--solution` is REQUIRED — Recall stores
    /// fixes, not raw CI events. Secret-like log content fails closed in
    /// CI (nothing is stored unless it passes sanitization).
    #[arg(long, conflicts_with = "from_shell", conflicts_with = "from_git")]
    pub from_ci: bool,

    /// Label of the failing CI step, appended to the auto-built problem
    /// (only meaningful with --from-ci).
    #[arg(long, value_name = "NAME")]
    pub step: Option<String>,
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

/// Open the database for a command, remembering when this run first
/// creates it (first-run detection: the welcome banner prints once,
/// after the very first command that creates the database).
fn open_db(config: &Config, created: &mut Option<PathBuf>) -> anyhow::Result<Db> {
    if created.is_none() && !config.db_path.exists() {
        *created = Some(config.db_path.clone());
    }
    Ok(Db::open(&config.db_path)?)
}

/// Parse, resolve configuration, and dispatch.
///
/// The database is opened only for subcommands that need it — shell/git
/// management must work even when no database exists yet (ADR-0017/0020).
pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    crate::observability::init(cli.verbose);

    let cwd = std::env::current_dir()?;
    let mut created: Option<PathBuf> = None;

    match cli.command {
        Command::Capture(args) => {
            let config = Config::resolve(cli.db.clone())?;
            let mut db = open_db(&config, &mut created)?;
            let outcome = application::capture::run(&mut db, args.as_ref(), &cwd)?;
            match outcome {
                application::capture::CaptureOutcome::Captured { id, project } => {
                    println!(
                        "{}Captured #{} (project: {})",
                        ui::ok(),
                        id,
                        project.unwrap_or_else(|| "no project".to_string())
                    );
                    if ui::pretty() && args.error.is_none() && args.context.is_none() {
                        println!(
                            "{} add the exact error text later: recall edit {id} --error \"...\"",
                            ui::tip()
                        );
                    }
                }
                application::capture::CaptureOutcome::SkippedDuplicate { id, project } => {
                    println!(
                        "{}Skipped: near-identical memory #{} already exists (project: {}). Use --force to capture anyway.",
                        ui::warn(),
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
            let mut db = open_db(&config, &mut created)?;
            application::edit::run(&mut db, &args)?;
            println!("{}Edited #{}", ui::ok(), args.id);
        }
        Command::Search {
            query,
            project,
            include_archived,
            explain,
        } => {
            let config = Config::resolve(cli.db.clone())?;
            let db = open_db(&config, &mut created)?;
            let filter = crate::infrastructure::database::SearchFilter {
                project,
                include_archived,
            };
            application::search::run(
                &db,
                &query.join(" "),
                application::search::DEFAULT_LIMIT,
                explain,
                &filter,
            )?;
        }
        Command::List {
            limit,
            project,
            archived,
        } => {
            let config = Config::resolve(cli.db.clone())?;
            let db = open_db(&config, &mut created)?;
            let filter = crate::infrastructure::database::SearchFilter {
                project,
                include_archived: archived,
            };
            application::list::run(&db, limit, &filter)?;
        }
        Command::Projects => {
            let config = Config::resolve(cli.db.clone())?;
            let db = open_db(&config, &mut created)?;
            application::projects::run(&db, &cwd)?;
        }
        Command::Check => {
            let config = Config::resolve(cli.db.clone())?;
            let db = open_db(&config, &mut created)?;
            application::check::run(&db)?;
        }
        Command::Version => {
            let config = Config::resolve(cli.db.clone())?;
            // Read-only: only open the database when it already exists.
            // Never a first-run trigger — informational commands must not
            // have write side effects.
            let schema = if config.db_path.exists() {
                let db = Db::open(&config.db_path)?;
                Some(db.schema_version()?)
            } else {
                None
            };
            application::version::run(schema)?;
        }
        Command::Archive { id } => {
            let config = Config::resolve(cli.db.clone())?;
            let mut db = open_db(&config, &mut created)?;
            application::lifecycle::set_status(
                &mut db,
                id,
                crate::domain::memory::MemoryStatus::Archived,
            )?;
        }
        Command::Unarchive { id } => {
            let config = Config::resolve(cli.db.clone())?;
            let mut db = open_db(&config, &mut created)?;
            application::lifecycle::set_status(
                &mut db,
                id,
                crate::domain::memory::MemoryStatus::Active,
            )?;
        }
        Command::Delete { id, project, yes } => {
            let config = Config::resolve(cli.db.clone())?;
            let mut db = open_db(&config, &mut created)?;
            let mut input = std::io::stdin().lock();
            let mut out = std::io::stderr();
            let stdin_is_tty = std::io::stdin().is_terminal();
            match (id, project) {
                (Some(id), _) => application::lifecycle::delete_one(
                    &mut db,
                    id,
                    yes,
                    stdin_is_tty,
                    &mut input,
                    &mut out,
                )?,
                (None, Some(project)) => application::lifecycle::delete_project(
                    &mut db,
                    &project,
                    yes,
                    stdin_is_tty,
                    &mut input,
                    &mut out,
                )?,
                (None, None) => unreachable!("clap requires id or --project"),
            }
        }
        Command::Export {
            path,
            include_secrets,
        } => {
            let config = Config::resolve(cli.db.clone())?;
            let db = open_db(&config, &mut created)?;
            application::transfer::export(&db, path.as_deref(), include_secrets)?;
        }
        Command::Import { path, force } => {
            let config = Config::resolve(cli.db.clone())?;
            let mut db = open_db(&config, &mut created)?;
            application::transfer::import(&mut db, &path, force)?;
        }
        Command::Embeddings(command) => {
            let config = Config::resolve(cli.db.clone())?;
            let mut db = open_db(&config, &mut created)?;
            match command {
                EmbeddingsCommand::Status => application::embeddings::status(&db)?,
                EmbeddingsCommand::Build => application::embeddings::build(&mut db)?,
                EmbeddingsCommand::Download => application::embeddings::download()?,
            }
        }
        Command::Shell(command) => application::shell::run(&command)?,
        Command::Git(command) => application::git_hooks::run(&command)?,
    }

    // First-run welcome: printed once, after the first command that
    // creates the database, and only on an interactive terminal.
    if let Some(path) = created {
        if ui::pretty() {
            ui::print_first_run(&path);
        }
    }
    Ok(())
}
