//! Enumerated flag values. Kept separate from domain enums so `clap` never
//! leaks below the CLI layer; `From` impls bridge the two.

use clap::ValueEnum;

use crate::domain::device::DiskKind;
use crate::domain::plan::{Mode, OrderKind};
use crate::domain::request::ExistingPolicy;

/// `--mode`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum ModeArg {
    /// Pick from the detected disks.
    #[default]
    Auto,
    /// Many concurrent file copies (best for SSDs).
    Parallel,
    /// Sequential reader → buffer → writer (best when an HDD is involved).
    Pipeline,
}

impl From<ModeArg> for Mode {
    fn from(arg: ModeArg) -> Self {
        match arg {
            ModeArg::Auto => Self::Auto,
            ModeArg::Parallel => Self::Parallel,
            ModeArg::Pipeline => Self::Pipeline,
        }
    }
}

/// `--src-disk` / `--dst-disk`
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum KindArg {
    /// Solid-state drive.
    Ssd,
    /// Rotational hard disk.
    Hdd,
}

impl From<KindArg> for DiskKind {
    fn from(arg: KindArg) -> Self {
        match arg {
            KindArg::Ssd => Self::Ssd,
            KindArg::Hdd => Self::Hdd,
        }
    }
}

/// `--order`
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OrderArg {
    /// Largest files first.
    Largest,
    /// Alphabetical by path.
    Path,
    /// By inode number.
    Inode,
    /// By on-disk position (FIEMAP).
    Physical,
}

impl From<OrderArg> for OrderKind {
    fn from(arg: OrderArg) -> Self {
        match arg {
            OrderArg::Largest => Self::Largest,
            OrderArg::Path => Self::Path,
            OrderArg::Inode => Self::Inode,
            OrderArg::Physical => Self::Physical,
        }
    }
}

/// `--if-exists`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum ExistingArg {
    /// Replace existing files.
    #[default]
    Overwrite,
    /// Leave existing files untouched.
    Skip,
    /// Skip files whose size and mtime already match (resume an interrupted copy).
    Update,
    /// Abort if any destination file exists.
    Fail,
}

impl From<ExistingArg> for ExistingPolicy {
    fn from(arg: ExistingArg) -> Self {
        match arg {
            ExistingArg::Overwrite => Self::Overwrite,
            ExistingArg::Skip => Self::Skip,
            ExistingArg::Update => Self::Update,
            ExistingArg::Fail => Self::Fail,
        }
    }
}

/// `--color`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum ColorArg {
    /// Colour when writing to a terminal.
    #[default]
    Auto,
    /// Always colour.
    Always,
    /// Never colour.
    Never,
}
