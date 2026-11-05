//! Message output with verbosity and colour handling.

use std::fmt::Display;
use std::io::IsTerminal;

use console::style;

use super::args::ColorArg;
use super::ui::theme;

/// How much to print.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verbosity {
    /// Errors only.
    Quiet,
    /// Plan, progress and summary.
    Normal,
    /// Plus the reasoning behind the plan.
    Verbose,
    /// Plus per-file detail.
    Trace,
}

impl Verbosity {
    /// From `-q` and the number of `-v` flags.
    pub const fn from_flags(quiet: bool, verbose: u8) -> Self {
        match (quiet, verbose) {
            (true, _) => Self::Quiet,
            (false, 0) => Self::Normal,
            (false, 1) => Self::Verbose,
            (false, _) => Self::Trace,
        }
    }
}

/// Writes user-facing messages to stderr (stdout stays clean for completions).
#[derive(Debug, Clone, Copy)]
pub struct Output {
    verbosity: Verbosity,
}

impl Output {
    /// Applies the colour choice globally and returns a writer.
    pub fn new(verbosity: Verbosity, color: ColorArg) -> Self {
        match color {
            ColorArg::Always => console::set_colors_enabled_stderr(true),
            ColorArg::Never => console::set_colors_enabled_stderr(false),
            ColorArg::Auto => {}
        }
        Self { verbosity }
    }

    /// Current verbosity.
    pub const fn verbosity(&self) -> Verbosity {
        self.verbosity
    }

    /// `true` when live progress bars make sense.
    pub fn shows_progress(&self) -> bool {
        self.verbosity >= Verbosity::Normal && std::io::stderr().is_terminal()
    }

    /// `true` when the user can answer a prompt.
    pub fn is_interactive(&self) -> bool {
        std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
    }

    /// A labelled line: `▸ label  value`.
    pub fn field(&self, label: &str, value: impl Display) {
        if self.verbosity >= Verbosity::Normal {
            eprintln!(
                "{} {:<12} {value}",
                style(theme::BULLET).cyan(),
                style(label).bold()
            );
        }
    }

    /// A success line.
    pub fn success(&self, msg: impl Display) {
        if self.verbosity >= Verbosity::Normal {
            eprintln!("{} {msg}", style(theme::OK).green().bold());
        }
    }

    /// Extra detail shown with `-v`.
    pub fn detail(&self, msg: impl Display) {
        if self.verbosity >= Verbosity::Verbose {
            eprintln!("  {}", style(msg).dim());
        }
    }

    /// A suggestion about flags.
    pub fn hint(&self, msg: impl Display) {
        if self.verbosity >= Verbosity::Normal {
            eprintln!(
                "{} {}",
                style(theme::HINT).yellow().bold(),
                style(msg).yellow()
            );
        }
    }

    /// A warning (shown unless quiet).
    pub fn warn(&self, msg: impl Display) {
        if self.verbosity >= Verbosity::Normal {
            eprintln!("{} {}", style(theme::WARN).yellow().bold(), msg);
        }
    }

    /// An error (always shown).
    pub fn error(&self, msg: impl Display) {
        eprintln!(
            "{} {}",
            style(format!("{} error:", theme::ERR)).red().bold(),
            style(msg).red()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_wins_over_verbose() {
        assert_eq!(Verbosity::from_flags(true, 2), Verbosity::Quiet);
    }

    #[test]
    fn two_v_flags_mean_trace() {
        assert_eq!(Verbosity::from_flags(false, 2), Verbosity::Trace);
    }
}
