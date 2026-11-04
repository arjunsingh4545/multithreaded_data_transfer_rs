use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::time::Duration;

// NOTE: indicatif placeholders must not contain whitespace, e.g. `{eta}` not `{ eta }`.
// A stray space makes the parser treat `{` as literal text and then fail on the `}`.
const SCANNER_TEMPLATE: &str = "{spinner:.cyan.bold} {msg}";
const GLOBAL_TEMPLATE: &str = "{spinner:.green} {bar:40.cyan/dim} {bytes:>10}/{total_bytes:<10} [{binary_bytes_per_sec}, ETA {eta}]";
const WORKER_TEMPLATE: &str =
    "{prefix:.bold.dim} {bar:24.magenta/dim} {bytes:>9}/{total_bytes:<9} {wide_msg:.dim}";

pub struct CopyUi {
    pub _mp: MultiProgress,
    pub global_pb: ProgressBar,
    pub worker_pbs: Vec<ProgressBar>,
}

impl CopyUi {
    /// Creates a sleek spinner while Rayon scans the directory tree
    pub fn new_scanner_spinner() -> ProgressBar {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::with_template(SCANNER_TEMPLATE)
                .expect("invalid scanner spinner template")
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", "✔"]),
        );
        pb.enable_steady_tick(Duration::from_millis(80));
        pb
    }

    /// Initializes the global progress bar + 1 progress bar per allocated worker
    pub fn new(total_bytes: u64, num_workers: usize) -> Self {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::stderr_with_hz(20));

        // Global progress bar (uv/pixi style solid UTF-8 blocks)
        let global_pb = mp.add(ProgressBar::new(total_bytes));
        global_pb.set_style(
            ProgressStyle::with_template(GLOBAL_TEMPLATE)
                .expect("invalid global progress bar template")
                .progress_chars("━━╸"),
        );
        global_pb.enable_steady_tick(Duration::from_millis(100));

        // Individual progress bar per worker
        let mut worker_pbs = Vec::with_capacity(num_workers);
        for i in 0..num_workers {
            let prefix = if i + 1 == num_workers {
                format!("  └─ Worker #{:<2}", i + 1)
            } else {
                format!("  ├─ Worker #{:<2}", i + 1)
            };

            let pb = mp.add(ProgressBar::new(100));
            pb.set_prefix(prefix);
            pb.set_style(
                ProgressStyle::with_template(WORKER_TEMPLATE)
                    .expect("invalid worker progress bar template")
                    .progress_chars("━━╸"),
            );
            pb.set_message("idle");
            worker_pbs.push(pb);
        }

        Self {
            _mp: mp,
            global_pb,
            worker_pbs,
        }
    }

    pub fn finish_all(&self) {
        for pb in &self.worker_pbs {
            pb.finish_and_clear();
        }
        self.global_pb.finish_with_message("Done");
    }

    pub fn abandon_all(&self) {
        for pb in &self.worker_pbs {
            pb.finish_and_clear();
        }
        self.global_pb.abandon();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_templates_parse() {
        for (name, t) in [
            ("scanner", SCANNER_TEMPLATE),
            ("global", GLOBAL_TEMPLATE),
            ("worker", WORKER_TEMPLATE),
        ] {
            assert!(
                ProgressStyle::with_template(t).is_ok(),
                "{name} template failed to parse: {t}"
            );
        }
    }

    /// Regression: whitespace inside a placeholder caused the original
    /// `TemplateError { state: DoubleClose, next: ',' }` panic.
    #[test]
    fn whitespace_in_placeholder_is_rejected() {
        let bad = "[{ binary_bytes_per_sec }, ETA {eta}]";
        assert!(ProgressStyle::with_template(bad).is_err());
    }

    #[test]
    fn copy_ui_constructs() {
        let ui = CopyUi::new(1024, 4);
        assert_eq!(ui.worker_pbs.len(), 4);
        ui.finish_all();
    }
}
