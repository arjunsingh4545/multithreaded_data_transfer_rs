//! Ctrl-C handling: first press cancels cleanly, second press exits at once.

use console::style;
use tokio::runtime::Runtime;
use tokio_util::sync::CancellationToken;

/// Exit code for an interrupted run (128 + SIGINT).
pub const INTERRUPTED: u8 = 130;

/// Spawns the signal watcher on `runtime`.
pub fn install(runtime: &Runtime, cancel: CancellationToken) {
    runtime.spawn(async move {
        if tokio::signal::ctrl_c().await.is_err() {
            return;
        }
        eprintln!(
            "\n{} cancelling, removing partial files… (press Ctrl-C again to quit immediately)",
            style("!").yellow().bold()
        );
        cancel.cancel();
        if tokio::signal::ctrl_c().await.is_ok() {
            std::process::exit(i32::from(INTERRUPTED));
        }
    });
}
