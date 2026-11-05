//! Every colour, symbol and bar template in one place.

use indicatif::ProgressStyle;

// indicatif placeholders must not contain whitespace (`{eta}`, never `{ eta }`).
/// Scanner spinner.
pub const SCANNER_TEMPLATE: &str = "{spinner:.cyan.bold} {msg}";
/// Overall progress.
pub const TOTAL_TEMPLATE: &str = "{spinner:.green} {bar:40.cyan/dim} {bytes:>10}/{total_bytes:<10} [{binary_bytes_per_sec}, ETA {eta}]";
/// One worker.
pub const WORKER_TEMPLATE: &str =
    "{prefix:.bold.dim} {bar:24.magenta/dim} {bytes:>9}/{total_bytes:<9} {wide_msg:.dim}";
/// Pipeline buffer occupancy.
pub const GAUGE_TEMPLATE: &str =
    "{prefix:.bold.dim} {bar:24.yellow/dim} {pos:>4}/{len} buffers in flight";

/// Spinner frames (last frame shown on finish).
pub const TICKS: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", "✔"];
/// Bar fill characters.
pub const BAR_CHARS: &str = "━━╸";

/// Success mark.
pub const OK: &str = "✔";
/// Failure mark.
pub const ERR: &str = "✖";
/// Section bullet.
pub const BULLET: &str = "▸";
/// Hint marker.
pub const HINT: &str = "›";
/// Warning marker.
pub const WARN: &str = "!";

/// Builds a bar style from one of the constant templates above.
///
/// # Panics
///
/// If `template` is not a valid indicatif template (never for the constants here).
#[expect(
    clippy::expect_used,
    reason = "templates are compile-time constants, each covered by a unit test"
)]
pub fn bar_style(template: &str) -> ProgressStyle {
    ProgressStyle::with_template(template)
        .expect("progress template is valid")
        .progress_chars(BAR_CHARS)
        .tick_strings(TICKS)
}

/// `├─ label` for all but the last row, `└─ label` for the last.
pub fn tree_prefix(index: usize, count: usize, label: &str) -> String {
    let branch = if index + 1 == count {
        "└─"
    } else {
        "├─"
    };
    format!("  {branch} {label:<9}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_templates_parse() {
        for template in [
            SCANNER_TEMPLATE,
            TOTAL_TEMPLATE,
            WORKER_TEMPLATE,
            GAUGE_TEMPLATE,
        ] {
            assert!(ProgressStyle::with_template(template).is_ok(), "{template}");
        }
    }

    /// Regression: whitespace inside a placeholder once caused a template panic.
    #[test]
    fn whitespace_in_placeholder_is_rejected() {
        assert!(ProgressStyle::with_template("[{ binary_bytes_per_sec }]").is_err());
    }

    #[test]
    fn last_tree_row_uses_corner() {
        assert!(tree_prefix(1, 2, "read #2").contains("└─"));
    }
}
