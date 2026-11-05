//! Which bars to draw for a strategy.

use crate::domain::plan::Strategy;

/// Bar layout derived from the chosen strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiLayout {
    /// One bar per copier.
    Parallel {
        /// Copier count.
        workers: usize,
    },
    /// Reader bars, writer bars and a buffer gauge.
    Pipeline {
        /// Reader count.
        readers: usize,
        /// Writer count.
        writers: usize,
    },
}

impl From<&Strategy> for UiLayout {
    fn from(strategy: &Strategy) -> Self {
        match strategy {
            Strategy::Parallel(p) => Self::Parallel {
                workers: p.workers.get(),
            },
            Strategy::Pipeline(p) => Self::Pipeline {
                readers: p.readers.get(),
                writers: p.writers.get(),
            },
        }
    }
}
