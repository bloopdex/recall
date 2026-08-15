//! Terminal-facing presentation helpers.
//!
//! Decorative output (icons, symbols, the first-run banner) is shown
//! only when stdout is an interactive terminal AND the user has not
//! asked for plain output. Scripts, pipes, and CI always get the plain
//! form, so `recall | grep ...` output stays stable and machine-friendly.
//! Set `RECALL_PLAIN=1` to force plain output even in a terminal
//! (legacy consoles, broken fonts).
//!
//! Icon vocabulary (kept small, one meaning each):
//!
//! - 🧠 Recall · ✓ success · ✗ error · ⚠ warning · → next step
//! - 🔒 local-first/privacy · 📦 installation · 🔍 search · 💾 storage
//! - 🌿 git · 🖥 shell · ⚙ maintenance · 💡 tip · 📁 project · 🕒 time

use std::io::IsTerminal;
use std::path::Path;

use crate::error::Error;

/// True when the current invocation gets decorative output.
pub fn pretty() -> bool {
    std::io::stdout().is_terminal() && std::env::var_os("RECALL_PLAIN").is_none()
}

/// The symbol set for one mode (fancy terminal vs plain output).
#[derive(Debug, Clone, Copy)]
struct Style {
    ok: &'static str,
    err: &'static str,
    warn: &'static str,
    arrow: &'static str,
    tip: &'static str,
    brain: &'static str,
    search: &'static str,
    lock: &'static str,
    folder: &'static str,
    clock: &'static str,
    storage: &'static str,
}

const FANCY: Style = Style {
    ok: "✓ ",
    err: "✗",
    warn: "⚠ ",
    arrow: "→",
    tip: "💡 Tip:",
    brain: "🧠 ",
    search: "🔍 ",
    lock: "🔒 ",
    folder: "📁 ",
    clock: "🕒 ",
    storage: "💾 ",
};

const PLAIN: Style = Style {
    ok: "",
    err: "error:",
    warn: "",
    arrow: "->",
    tip: "Tip:",
    brain: "",
    search: "",
    lock: "",
    folder: "",
    clock: "",
    storage: "",
};

fn style(enabled: bool) -> Style {
    if enabled {
        FANCY
    } else {
        PLAIN
    }
}

/// Success marker (`✓ ` on a terminal, nothing when piped).
pub fn ok() -> &'static str {
    style(pretty()).ok
}

/// Warning marker (`⚠ ` on a terminal, nothing when piped).
pub fn warn() -> &'static str {
    style(pretty()).warn
}

/// Next-step arrow (`→` on a terminal, `->` when piped).
pub fn arrow() -> &'static str {
    style(pretty()).arrow
}

/// Tip prefix (`💡 Tip:` on a terminal, `Tip:` when piped).
pub fn tip() -> &'static str {
    style(pretty()).tip
}

/// The first-run welcome: shown once, when the database is first
/// created, and only on an interactive terminal (never in scripts or
/// CI). Purely informational — it does not prompt and does not block.
pub fn print_first_run(db_path: &Path) {
    let s = style(true);
    println!();
    println!("{}Recall — your personal engineering memory", s.brain);
    println!();
    println!("Remember how you solved engineering problems, and find the");
    println!("solution months later with one search.");
    println!();
    println!("{}Local-first", s.lock);
    println!("Your memories are stored on this machine only:");
    println!("  {}", db_path.display());
    println!("No network, no telemetry, no accounts.");
    println!();
    println!("{}Start here:", s.arrow);
    println!("  recall capture                 remember how you solved a problem");
    println!("  recall search \"<a few words>\"   find it again months later");
    println!("  recall --help                  explore every command");
    println!();
    println!("{}Quiet mode: set RECALL_PLAIN=1 for plain output.", s.tip);
    println!();
}

/// Print a user-facing error. On a terminal: a `✗` line plus a short
/// recovery hint when one exists. Piped: the same text, ASCII-only.
pub fn print_error(err: &Error) {
    let s = style(pretty());
    eprintln!("{} {err}", s.err);
    if let Error::Db(rusqlite::Error::SqliteFailure(e, _)) = err {
        if matches!(
            e.code,
            rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
        ) {
            eprintln!(
                "{} Another Recall process is writing right now. Retry the command; it will succeed once the write finishes.",
                s.arrow
            );
        }
    }
}

/// Decoration for search result lines, only used in the pretty format.
pub fn folder() -> &'static str {
    style(pretty()).folder
}

pub fn brain() -> &'static str {
    style(pretty()).brain
}

pub fn clock() -> &'static str {
    style(pretty()).clock
}

pub fn search() -> &'static str {
    style(pretty()).search
}

pub fn storage() -> &'static str {
    style(pretty()).storage
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_style_is_ascii_and_minimal() {
        let s = style(false);
        assert_eq!(s.ok, "");
        assert_eq!(s.err, "error:");
        assert_eq!(s.arrow, "->");
        assert_eq!(s.tip, "Tip:");
        assert_eq!(s.brain, "");
        assert!(s.ok.is_ascii() && s.err.is_ascii() && s.warn.is_ascii());
        assert!(s.arrow.is_ascii() && s.tip.is_ascii() && s.search.is_ascii());
    }

    #[test]
    fn fancy_style_has_the_documented_icons() {
        let s = style(true);
        assert_eq!(s.ok, "✓ ");
        assert_eq!(s.err, "✗");
        assert_eq!(s.warn, "⚠ ");
        assert_eq!(s.arrow, "→");
        assert_eq!(s.brain, "🧠 ");
        assert_eq!(s.lock, "🔒 ");
        assert_eq!(s.folder, "📁 ");
    }

    #[test]
    fn arrow_is_plain_when_not_pretty() {
        std::env::remove_var("RECALL_PLAIN");
        // No TTY in tests: the piped form is what scripts see and it
        // must stay ASCII.
        assert_eq!(arrow(), "->");
        assert!(arrow().is_ascii());
    }
}
