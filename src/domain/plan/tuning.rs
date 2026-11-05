//! User-facing knobs that steer planning.

use std::fmt;
use std::num::NonZeroUsize;

use crate::util::units::ByteSize;

/// Which strategy family to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Decide from the detected disks.
    #[default]
    Auto,
    /// Always use many concurrent whole-file copies.
    Parallel,
    /// Always use the reader → writer pipeline.
    Pipeline,
}

/// The order in which files are copied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrderKind {
    /// Largest files first (minimises tail latency on SSDs).
    Largest,
    /// Lexicographic by path (keeps each directory together).
    Path,
    /// By source inode number (approximates on-disk order on ext4/xfs).
    Inode,
    /// By physical block address via FIEMAP (true on-disk order; falls back to path).
    Physical,
}

impl fmt::Display for OrderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Largest => "largest first",
            Self::Path => "path order",
            Self::Inode => "inode order",
            Self::Physical => "physical order",
        })
    }
}

/// Optional overrides; `None` means "let the planner decide".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tuning {
    /// Strategy family.
    pub mode: Mode,
    /// Worker count for parallel mode.
    pub workers: Option<NonZeroUsize>,
    /// Reader threads for pipeline mode.
    pub readers: Option<NonZeroUsize>,
    /// Writer threads for pipeline mode.
    pub writers: Option<NonZeroUsize>,
    /// I/O size per request.
    pub chunk: Option<ByteSize>,
    /// Buffer memory for pipeline mode.
    pub mem: Option<ByteSize>,
    /// Copy order.
    pub order: Option<OrderKind>,
}
