//! `copy`: prepare, show the plan, confirm, execute, summarise.

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::process::ExitCode;

use super::{Context, Run};
use crate::cli::args::{CopyArgs, FlagsUsed};
use crate::cli::output::Output;
use crate::cli::report;
use crate::cli::ui::{CopyUi, ScanSpinner, UiLayout};
use crate::domain::request::{CopyRequest, ExistingPolicy};
use crate::engine::progress::{NoProgress, ProgressSink};
use crate::engine::{Engine, Prepared, Summary};
use crate::error::Result;

impl Run for CopyArgs {
    fn run(self, ctx: &Context) -> Result<ExitCode> {
        let out = &ctx.out;
        let flags = FlagsUsed::from(&self);
        let (dry_run, assume_yes) = (self.safety.dry_run, self.safety.yes);
        let request = CopyRequest::from(self);

        if !dry_run && !assume_yes && !confirm_destination(*out, &request) {
            out.warn("aborted; nothing was copied");
            return Ok(ExitCode::FAILURE);
        }

        let engine = Engine::new();
        let prepared = {
            let spinner = ScanSpinner::new(&request.src, out.shows_progress());
            let prepared = engine.prepare(&request, &spinner);
            spinner.finish();
            prepared?
        };

        report::plan(out, &prepared);
        if let Some(hint) = flags.hint_for(&prepared.plan().strategy) {
            out.hint(hint);
        }
        if dry_run {
            out.success("dry run: nothing was written");
            return Ok(ExitCode::SUCCESS);
        }

        let summary = if out.shows_progress() {
            let ui = CopyUi::new(
                prepared.manifest().total_bytes,
                UiLayout::from(&prepared.plan().strategy),
            );
            let result = execute(&engine, &prepared, &ui, ctx);
            ui.finish(result.is_ok());
            result?
        } else {
            execute(&engine, &prepared, &NoProgress, ctx)?
        };

        report::summary(out, &summary);
        Ok(ExitCode::SUCCESS)
    }
}

/// Statically dispatched over the progress sink (bars or nothing).
fn execute<P: ProgressSink>(
    engine: &Engine,
    prepared: &Prepared,
    progress: &P,
    ctx: &Context,
) -> Result<Summary> {
    engine.execute(prepared, progress, &ctx.cancel)
}

/// Asks before copying into a non-empty destination with `--if-exists overwrite`.
///
/// Never blocks scripts: without an interactive terminal the answer is "yes".
fn confirm_destination(out: Output, request: &CopyRequest) -> bool {
    if request.existing != ExistingPolicy::Overwrite
        || !out.is_interactive()
        || !is_non_empty_dir(&request.dst)
    {
        return true;
    }
    eprint!(
        "{} '{}' is not empty; files with the same names will be overwritten. Continue? [y/N] ",
        console::style("?").cyan().bold(),
        request.dst.display()
    );
    io::stderr().flush().ok();
    let mut answer = String::new();
    io::stdin().lock().read_line(&mut answer).is_ok()
        && matches!(answer.trim(), "y" | "Y" | "yes" | "YES")
}

fn is_non_empty_dir(path: &Path) -> bool {
    fs::read_dir(path).is_ok_and(|mut entries| entries.next().is_some())
}
