//! Command-line grammar.
//!
//! `fastcp SRC DST` is shorthand for `fastcp copy SRC DST`; the explicit
//! subcommands are `copy`, `probe` and `completions`.

mod convert;
mod copy;
mod value;

use std::path::PathBuf;

use std::ffi::OsString;

use clap::error::ErrorKind;
use clap::parser::ValueSource;
use clap::{ArgMatches, Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueHint};
use clap_complete::Shell;

pub use self::convert::FlagsUsed;
pub use self::copy::{CopyArgs, CopyPaths, DetectArgs, PerfArgs, SafetyArgs};
pub use self::value::{ColorArg, ExistingArg, KindArg, ModeArg, OrderArg};
use super::output::Verbosity;

const EXAMPLES: &str = "\
Examples:
  fastcp /mnt/usb/photos /data/photos         auto-detect everything
  fastcp -n /mnt/usb/photos /data/photos      preview the plan only
  fastcp --if-exists update src/ dst/         resume an interrupted copy
  fastcp --mem 4G --chunk-size 16M src/ dst/  more buffering for HDD → HDD
  fastcp probe /mnt/usb /data                 show detected disks";

/// Disk-aware directory copy: parallel for SSDs, streaming pipeline for hard disks.
#[derive(Debug, Parser)]
#[command(
    name = "fastcp",
    version,
    about,
    subcommand_negates_reqs = true,
    arg_required_else_help = true,
    after_help = EXAMPLES,
)]
pub struct Cli {
    /// Explicit subcommand.
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Bare `SRC DST` form. Kept as a flat struct: clap only detects an
    /// optional flattened group when it has no nested `flatten` fields.
    #[command(flatten)]
    pub paths: Option<CopyPaths>,

    /// Safety options for the bare form.
    #[command(flatten)]
    pub safety: SafetyArgs,

    /// Performance options for the bare form.
    #[command(flatten)]
    pub perf: PerfArgs,

    /// Disk detection overrides for the bare form.
    #[command(flatten)]
    pub detect: DetectArgs,

    /// Output flags (accepted before or after the subcommand).
    #[command(flatten)]
    pub output: OutputArgs,
}

impl Cli {
    /// Parses `std::env::args_os()`, printing help or errors and exiting like
    /// [`Parser::parse`].
    pub fn parse_args() -> Self {
        Self::try_parse_args(std::env::args_os()).unwrap_or_else(|err| err.exit())
    }

    /// Parses `args` (including the program name).
    ///
    /// Global output flags may come before a subcommand
    /// (`fastcp -q probe /`), so subcommands cannot simply conflict with every
    /// top-level argument. Instead this rejects the copy-only arguments
    /// (paths, safety, performance, detection) when a subcommand is used.
    pub fn try_parse_args<I, T>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        let mut command = Self::command();
        let matches = command.try_get_matches_from_mut(args)?;
        if let Some((name, _)) = matches.subcommand()
            && let Some(flag) = copy_only_arg(&command, &matches)
        {
            return Err(command.error(
                ErrorKind::ArgumentConflict,
                format!("'{flag}' cannot be used with the '{name}' subcommand"),
            ));
        }
        Self::from_arg_matches(&matches).map_err(|err| err.format(&mut command))
    }

    /// The command to run; bare paths become `copy`.
    pub fn into_command(self) -> Option<Command> {
        let Self {
            command,
            paths,
            safety,
            perf,
            detect,
            ..
        } = self;
        command.or_else(|| {
            paths.map(|paths| {
                Command::Copy(CopyArgs {
                    paths,
                    safety,
                    perf,
                    detect,
                })
            })
        })
    }
}

/// The first top-level, non-global argument given on the command line, as the
/// user would write it (`--mem`, `<SRC>`).
fn copy_only_arg(command: &clap::Command, matches: &ArgMatches) -> Option<String> {
    command
        .get_arguments()
        .filter(|arg| !arg.is_global_set())
        .find(|arg| matches.value_source(arg.get_id().as_str()) == Some(ValueSource::CommandLine))
        .map(|arg| match arg.get_long() {
            Some(long) => format!("--{long}"),
            None => format!("<{}>", arg.get_id().as_str().to_uppercase()),
        })
}

/// Subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Copy a directory tree (the default when paths are given directly)
    Copy(CopyArgs),
    /// Show the detected disk behind each path and the strategy a copy would use
    Probe(ProbeArgs),
    /// Print a shell completion script to stdout
    Completions(CompletionsArgs),
}

/// `probe <PATH>...`
#[derive(Debug, Clone, Args)]
pub struct ProbeArgs {
    /// Paths to inspect (give two to see the planned strategy between them)
    #[arg(required = true, value_name = "PATH", value_hint = ValueHint::AnyPath)]
    pub paths: Vec<PathBuf>,
}

/// `completions <SHELL>`
#[derive(Debug, Clone, Args)]
pub struct CompletionsArgs {
    /// Target shell
    #[arg(value_enum)]
    pub shell: Shell,
}

/// Global output flags.
#[derive(Debug, Clone, Args)]
#[command(next_help_heading = "Output")]
pub struct OutputArgs {
    /// Only print errors
    #[arg(short, long, global = true, conflicts_with = "verbose")]
    pub quiet: bool,

    /// More detail (-v: plan reasoning, -vv: everything)
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// When to use colours
    #[arg(long, global = true, value_name = "WHEN", value_enum, default_value_t)]
    pub color: ColorArg,
}

impl OutputArgs {
    /// Combined verbosity level.
    pub const fn verbosity(&self) -> Verbosity {
        Verbosity::from_flags(self.quiet, self.verbose)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ByteSize;
    use crate::domain::request::{CopyRequest, ExistingPolicy};
    use clap::CommandFactory;
    use clap::error::ErrorKind;
    use std::num::NonZeroUsize;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_args(std::iter::once("fastcp").chain(args.iter().copied()))
    }

    fn copy_args(args: &[&str]) -> CopyArgs {
        match parse(args).unwrap().into_command() {
            Some(Command::Copy(copy)) => copy,
            other => panic!("expected copy, got {other:?}"),
        }
    }

    #[test]
    fn clap_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn bare_paths_parse_as_copy() {
        assert_eq!(copy_args(&["a", "b"]).paths.dst, PathBuf::from("b"));
    }

    #[test]
    fn explicit_copy_subcommand_parses() {
        assert!(copy_args(&["copy", "a", "b", "-n"]).safety.dry_run);
    }

    #[test]
    fn bare_form_keeps_options() {
        assert_eq!(
            copy_args(&["a", "b", "--readers", "3"]).perf.readers,
            NonZeroUsize::new(3)
        );
    }

    #[test]
    fn subcommand_and_bare_paths_conflict() {
        let err = parse(&["a", "b", "probe", "/"]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
    }

    #[test]
    fn copy_flags_conflict_with_other_subcommands() {
        let err = parse(&["--mem", "2G", "probe", "/"]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
    }

    #[test]
    fn output_flags_may_precede_a_subcommand() {
        let cli = parse(&["-q", "--color", "never", "probe", "/"]).unwrap();
        assert!(matches!(cli.command, Some(Command::Probe(_))));
        assert!(cli.output.quiet);
    }

    #[test]
    fn output_flags_may_follow_a_subcommand() {
        assert!(parse(&["probe", "-q", "/"]).unwrap().output.quiet);
    }

    #[test]
    fn plain_subcommand_has_no_copy_only_args() {
        let mut command = Cli::command();
        let matches = command
            .try_get_matches_from_mut(["fastcp", "probe", "/"])
            .unwrap();
        assert!(copy_only_arg(&command, &matches).is_none());
    }

    #[test]
    fn missing_destination_is_a_usage_error() {
        assert!(parse(&["only-src"]).is_err());
    }

    #[test]
    fn probe_requires_a_path() {
        assert!(parse(&["probe"]).is_err());
    }

    #[test]
    fn global_verbose_works_after_subcommand() {
        assert_eq!(parse(&["probe", "/", "-vv"]).unwrap().output.verbose, 2);
    }

    #[test]
    fn chunk_size_rejects_bad_suffix() {
        let err = parse(&["a", "b", "--chunk-size", "12X"]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::ValueValidation);
    }

    #[test]
    fn chunk_size_accepts_human_units() {
        assert_eq!(
            copy_args(&["a", "b", "--chunk-size", "16M"])
                .perf
                .chunk_size,
            Some(ByteSize::mib(16))
        );
    }

    #[test]
    fn zero_workers_is_rejected() {
        assert!(parse(&["a", "b", "-j", "0"]).is_err());
    }

    #[test]
    fn no_arguments_prints_help() {
        assert_eq!(
            parse(&[]).unwrap_err().kind(),
            ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        );
    }

    #[test]
    fn if_exists_maps_to_policy() {
        let request = CopyRequest::from(copy_args(&["a", "b", "--if-exists", "update"]));
        assert_eq!(request.existing, ExistingPolicy::Update);
    }

    #[test]
    fn disk_overrides_map_to_kinds() {
        let request = CopyRequest::from(copy_args(&["a", "b", "--src-disk", "hdd"]));
        assert_eq!(request.overrides.src, Some(crate::DiskKind::Hdd));
    }

    mod hints {
        use super::*;
        use crate::domain::plan::{Tuning, plan};
        use crate::domain::topology::TransferKind;

        fn strategy(transfer: TransferKind) -> crate::Strategy {
            plan(
                crate::domain::plan::WorkloadStats::default(),
                transfer,
                &Tuning::default(),
                NonZeroUsize::MIN,
            )
            .strategy
        }

        #[test]
        fn workers_with_pipeline_strategy_hints() {
            let flags = FlagsUsed::from(&copy_args(&["a", "b", "-j", "4"]));
            assert!(flags.hint_for(&strategy(TransferKind::HddToHdd)).is_some());
        }

        #[test]
        fn readers_with_parallel_strategy_hints() {
            let flags = FlagsUsed::from(&copy_args(&["a", "b", "--readers", "2"]));
            assert!(flags.hint_for(&strategy(TransferKind::SsdToSsd)).is_some());
        }

        #[test]
        fn matching_flags_give_no_hint() {
            let flags = FlagsUsed::from(&copy_args(&["a", "b", "-j", "4"]));
            assert!(flags.hint_for(&strategy(TransferKind::SsdToSsd)).is_none());
        }
    }
}
