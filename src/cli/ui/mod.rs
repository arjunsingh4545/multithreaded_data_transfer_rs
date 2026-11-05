//! Terminal progress: implements [`ProgressSink`] with indicatif bars.

pub mod layout;
pub mod theme;

use std::path::Path;
use std::time::Duration;

use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget};

pub use self::layout::UiLayout;
use crate::engine::progress::{ProgressSink, Role, WorkerIdx};
use crate::util::units::ByteSize;

/// Live copy progress: an overall bar plus one bar per worker.
#[derive(Debug)]
pub struct CopyUi {
    _multi: MultiProgress,
    total: ProgressBar,
    copiers: Vec<ProgressBar>,
    readers: Vec<ProgressBar>,
    writers: Vec<ProgressBar>,
    buffer: Option<ProgressBar>,
}

impl CopyUi {
    /// Draws bars for `layout` on stderr.
    pub fn new(total_bytes: u64, layout: UiLayout) -> Self {
        let multi = MultiProgress::with_draw_target(ProgressDrawTarget::stderr_with_hz(15));
        let total = multi
            .add(ProgressBar::new(total_bytes).with_style(theme::bar_style(theme::TOTAL_TEMPLATE)));
        total.enable_steady_tick(Duration::from_millis(100));

        let rows = |multi: &MultiProgress, count: usize, label: &str, offset: usize, all: usize| {
            (0..count)
                .map(|i| {
                    let bar = multi.add(
                        ProgressBar::new(1).with_style(theme::bar_style(theme::WORKER_TEMPLATE)),
                    );
                    bar.set_prefix(theme::tree_prefix(
                        offset + i,
                        all,
                        &format!("{label} #{}", i + 1),
                    ));
                    bar.set_message("idle");
                    bar
                })
                .collect::<Vec<_>>()
        };

        let (copiers, readers, writers, buffer) = match layout {
            UiLayout::Parallel { workers } => (
                rows(&multi, workers, "copy", 0, workers),
                vec![],
                vec![],
                None,
            ),
            UiLayout::Pipeline { readers, writers } => {
                let all = readers + writers + 1;
                let r = rows(&multi, readers, "read", 0, all);
                let w = rows(&multi, writers, "write", readers, all);
                let gauge = multi
                    .add(ProgressBar::new(1).with_style(theme::bar_style(theme::GAUGE_TEMPLATE)));
                gauge.set_prefix(theme::tree_prefix(all - 1, all, "buffer"));
                (vec![], r, w, Some(gauge))
            }
        };

        Self {
            _multi: multi,
            total,
            copiers,
            readers,
            writers,
            buffer,
        }
    }

    /// Removes worker bars; keeps (or abandons) the overall bar.
    pub fn finish(&self, success: bool) {
        self.copiers
            .iter()
            .chain(&self.readers)
            .chain(&self.writers)
            .chain(&self.buffer)
            .for_each(ProgressBar::finish_and_clear);
        if success {
            self.total.finish();
        } else {
            self.total.abandon();
        }
    }

    fn bar(&self, role: Role, worker: WorkerIdx) -> Option<&ProgressBar> {
        let bars = match role {
            Role::Copier => &self.copiers,
            Role::Reader => &self.readers,
            Role::Writer => &self.writers,
        };
        bars.get(worker.0)
    }
}

impl ProgressSink for CopyUi {
    fn file_started(&self, role: Role, worker: WorkerIdx, name: &str, size: u64) {
        if let Some(bar) = self.bar(role, worker) {
            bar.reset();
            bar.set_length(size.max(1));
            bar.set_message(name.to_owned());
        }
    }

    fn advanced(&self, role: Role, worker: WorkerIdx, bytes: u64) {
        if let Some(bar) = self.bar(role, worker) {
            bar.inc(bytes);
        }
        if role.counts_toward_total() {
            self.total.inc(bytes);
        }
    }

    fn buffer_level(&self, in_use: usize, capacity: usize) {
        if let Some(gauge) = &self.buffer {
            gauge.set_length(capacity as u64);
            gauge.set_position(in_use as u64);
        }
    }

    fn worker_done(&self, role: Role, worker: WorkerIdx) {
        if let Some(bar) = self.bar(role, worker) {
            bar.set_message("done");
        }
    }
}

/// Spinner shown while the source tree is scanned.
#[derive(Debug)]
pub struct ScanSpinner {
    bar: ProgressBar,
    root: String,
}

impl ScanSpinner {
    /// A spinner for scanning `src`; hidden when `visible` is false.
    pub fn new(src: &Path, visible: bool) -> Self {
        let bar = if visible {
            ProgressBar::new_spinner().with_style(theme::bar_style(theme::SCANNER_TEMPLATE))
        } else {
            ProgressBar::hidden()
        };
        bar.enable_steady_tick(Duration::from_millis(80));
        let root = src.display().to_string();
        bar.set_message(format!("Scanning {root} …"));
        Self { bar, root }
    }

    /// Removes the spinner.
    pub fn finish(&self) {
        self.bar.finish_and_clear();
    }
}

impl ProgressSink for ScanSpinner {
    fn scanned(&self, files: usize, bytes: u64) {
        self.bar.set_message(format!(
            "Scanning {} … {files} files ({})",
            self.root,
            ByteSize::new(bytes)
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_layout_creates_reader_writer_and_gauge_bars() {
        let ui = CopyUi::new(
            1024,
            UiLayout::Pipeline {
                readers: 2,
                writers: 1,
            },
        );
        assert_eq!(
            (ui.readers.len(), ui.writers.len(), ui.buffer.is_some()),
            (2, 1, true)
        );
        ui.finish(true);
    }

    #[test]
    fn reader_bytes_do_not_advance_total() {
        let ui = CopyUi::new(
            100,
            UiLayout::Pipeline {
                readers: 1,
                writers: 1,
            },
        );
        ui.advanced(Role::Reader, WorkerIdx(0), 40);
        ui.advanced(Role::Writer, WorkerIdx(0), 10);
        assert_eq!(ui.total.position(), 10);
    }

    #[test]
    fn unknown_worker_index_is_ignored() {
        let ui = CopyUi::new(100, UiLayout::Parallel { workers: 1 });
        ui.file_started(Role::Copier, WorkerIdx(9), "x", 1);
        assert_eq!(ui.copiers.len(), 1);
    }
}
