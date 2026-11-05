//! Terminal front end: parse → dispatch → map the result to an exit code.

pub mod args;
pub mod commands;
pub mod output;
pub mod report;
pub mod signal;
pub mod ui;

use std::process::ExitCode;

use tokio_util::sync::CancellationToken;

use self::args::Cli;
use self::commands::{Context, Run};
use self::output::Output;

/// Runs the CLI and returns the process exit code.
pub fn main() -> ExitCode {
    let cli = Cli::parse_args();
    let out = Output::new(cli.output.verbosity(), cli.output.color);
    let Some(command) = cli.into_command() else {
        // `arg_required_else_help` makes this unreachable in practice.
        return ExitCode::from(2);
    };

    let cancel = CancellationToken::new();
    // The runtime only hosts the Ctrl-C watcher; all copying is plain threads.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .thread_name("signals")
        .enable_all()
        .build();
    match &runtime {
        Ok(rt) => signal::install(rt, cancel.clone()),
        Err(err) => out.warn(format!("Ctrl-C handling unavailable: {err}")),
    }

    let ctx = Context { out, cancel };
    match command.run(&ctx) {
        Ok(code) => code,
        Err(err) => {
            out.error(&err);
            ExitCode::from(err.exit_code())
        }
    }
}
