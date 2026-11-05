//! `completions`: print a shell completion script.

use std::io::{self, Write};
use std::process::ExitCode;

use clap::CommandFactory;

use super::{Context, Run};
use crate::cli::args::{Cli, CompletionsArgs};
use crate::error::Result;

impl Run for CompletionsArgs {
    fn run(self, ctx: &Context) -> Result<ExitCode> {
        let mut command = Cli::command();
        let name = command.get_name().to_owned();
        // Render into memory first: `generate` panics if writing to stdout fails.
        let mut script = Vec::new();
        clap_complete::generate(self.shell, &mut command, name, &mut script);

        match io::stdout().lock().write_all(&script) {
            // `fastcp completions bash | head` closes the pipe early; that is not an error.
            Ok(()) => Ok(ExitCode::SUCCESS),
            Err(err) if err.kind() == io::ErrorKind::BrokenPipe => Ok(ExitCode::SUCCESS),
            Err(err) => {
                ctx.out.error(format!("cannot write completions: {err}"));
                Ok(ExitCode::FAILURE)
            }
        }
    }
}
