//! Arguments of the `copy` command, grouped by help heading.

use std::num::NonZeroUsize;
use std::path::PathBuf;

use clap::{Args, ValueHint};

use super::value::{ExistingArg, KindArg, ModeArg, OrderArg};
use crate::util::units::ByteSize;

/// `copy <SRC> <DST> [OPTIONS]`
#[derive(Debug, Clone, Args)]
pub struct CopyArgs {
    /// Source and destination.
    #[command(flatten)]
    pub paths: CopyPaths,

    /// Safety options.
    #[command(flatten)]
    pub safety: SafetyArgs,

    /// Performance options.
    #[command(flatten)]
    pub perf: PerfArgs,

    /// Disk detection overrides.
    #[command(flatten)]
    pub detect: DetectArgs,
}

/// The two positional paths.
#[derive(Debug, Clone, Args)]
pub struct CopyPaths {
    /// Directory to copy from
    #[arg(value_name = "SRC", value_hint = ValueHint::DirPath)]
    pub src: PathBuf,

    /// Directory to copy into (created if missing)
    #[arg(value_name = "DST", value_hint = ValueHint::DirPath)]
    pub dst: PathBuf,
}

/// What may be touched.
#[derive(Debug, Clone, Args)]
#[command(next_help_heading = "Safety")]
pub struct SafetyArgs {
    /// Scan and show the plan without writing anything
    #[arg(short = 'n', long)]
    pub dry_run: bool,

    /// What to do with files that already exist at DST
    #[arg(long, value_name = "POLICY", value_enum, default_value_t)]
    pub if_exists: ExistingArg,

    /// Don't ask before copying into a non-empty DST
    #[arg(short = 'y', long)]
    pub yes: bool,
}

/// Performance knobs; leave unset to let the planner decide.
#[derive(Debug, Clone, Args)]
#[command(next_help_heading = "Performance")]
pub struct PerfArgs {
    /// Copy strategy
    #[arg(short, long, value_enum, default_value_t)]
    pub mode: ModeArg,

    /// Concurrent copies in parallel mode
    #[arg(short = 'j', long, value_name = "N")]
    pub workers: Option<NonZeroUsize>,

    /// Reader threads in pipeline mode
    #[arg(long, value_name = "N")]
    pub readers: Option<NonZeroUsize>,

    /// Writer threads in pipeline mode
    #[arg(long, value_name = "N")]
    pub writers: Option<NonZeroUsize>,

    /// I/O size per request, e.g. 512K, 8M
    #[arg(long, value_name = "SIZE", env = "FASTCP_CHUNK")]
    pub chunk_size: Option<ByteSize>,

    /// Buffer memory in pipeline mode [default: 1G, 4G for same-disk copies]
    #[arg(long, value_name = "SIZE", env = "FASTCP_MEM")]
    pub mem: Option<ByteSize>,

    /// Copy order [default: chosen by plan]
    #[arg(long, value_enum)]
    pub order: Option<OrderArg>,
}

/// Disk detection overrides.
#[derive(Debug, Clone, Args)]
#[command(next_help_heading = "Disk detection")]
pub struct DetectArgs {
    /// Treat the source disk as this kind instead of detecting it
    #[arg(long, value_name = "KIND", value_enum)]
    pub src_disk: Option<KindArg>,

    /// Treat the destination disk as this kind instead of detecting it
    #[arg(long, value_name = "KIND", value_enum)]
    pub dst_disk: Option<KindArg>,
}
