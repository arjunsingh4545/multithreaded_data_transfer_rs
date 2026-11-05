//! One module per subcommand, all behind the [`Run`] trait.

mod completions;
mod copy;
mod probe;

use std::process::ExitCode;

use tokio_util::sync::CancellationToken;

use super::args::Command;
use super::output::Output;
use crate::error::Result;

/// Shared state every command receives.
#[derive(Debug, Clone)]
pub struct Context {
    /// Message writer.
    pub out: Output,
    /// Fired by Ctrl-C.
    pub cancel: CancellationToken,
}

/// A runnable command.
pub trait Run {
    /// Executes the command, returning the process exit code.
    fn run(self, ctx: &Context) -> Result<ExitCode>;
}

impl Run for Command {
    fn run(self, ctx: &Context) -> Result<ExitCode> {
        match self {
            Self::Copy(args) => args.run(ctx),
            Self::Probe(args) => args.run(ctx),
            Self::Completions(args) => args.run(ctx),
        }
    }
}
