mod error;
mod planner;
mod scanner;
mod ui;
mod worker;

use clap::Parser;
use console::style;
use error::Result;
use std::path::PathBuf;
use std::time::Instant;
use tokio_util::sync::CancellationToken;
use ui::CopyUi;

#[derive(Parser, Debug)]
#[command(name = "fastcp", about = "Sleek multithreaded & async directory copy utility")]
struct Cli {
    /// Source directory to copy from
    pub src: PathBuf,

    /// Destination directory to copy into
    pub dst: PathBuf,

    /// Optional override for number of async copy workers
    #[arg(short, long)]
    pub workers: Option<usize>,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let start = Instant::now();

    if let Err(err) = execute(cli, start).await {
        eprintln!(
            "\n{} {}",
            style("✖ Error:").red().bold(),
            style(err).red()
        );
        std::process::exit(1);
    }
}

async fn execute(cli: Cli, start: Instant) -> Result<()> {
    // 1. Scan directory tree on Rayon thread pool with strict access verification
    let spinner = CopyUi::new_scanner_spinner();
    spinner.set_message(format!("Scanning {} ...", cli.src.display()));

    let src_clone = cli.src.clone();
    let dst_clone = cli.dst.clone();
    let spinner_clone = spinner.clone();

    let manifest = tokio::task::spawn_blocking(move || {
        scanner::scan_directory_parallel(&src_clone, &dst_clone, &spinner_clone)
    })
    .await
    .expect("Scanner task panicked")?;

    spinner.finish_and_clear();

    // 2. Compute optimal worker allocation
    let plan = planner::compute_worker_plan(&manifest, cli.workers);

    eprintln!(
        "{} Scanned {} files, {} symlinks, {} dirs ({:.2} MB) — allocated {} workers",
        style("✔").green().bold(),
        style(manifest.files.len()).cyan().bold(),
        style(manifest.symlinks.len()).cyan().bold(),
        style(manifest.directories.len()).cyan().bold(),
        manifest.total_bytes as f64 / (1024.0 * 1024.0),
        style(plan.num_workers).magenta().bold(),
    );

    // 3. Create target directory skeleton & recreate symlinks
    worker::create_directories(&manifest.directories).await?;
    worker::create_symlinks(&manifest.symlinks).await?;

    if manifest.files.is_empty() {
        eprintln!(
            "{} Completed in {:.2?}",
            style("✨").green(),
            start.elapsed()
        );
        return Ok(());
    }

    // 4. Initialize MultiProgress UI (Global bar + 1 bar per worker)
    let ui = CopyUi::new(manifest.total_bytes, plan.num_workers);

    // 5. Enqueue file jobs (largest files first to minimize tail latency)
    let mut sorted_files = manifest.files;
    sorted_files.sort_by(|a, b| b.size.cmp(&a.size));

    let (tx, rx) = async_channel::bounded(sorted_files.len().max(1));
    for job in sorted_files {
        tx.send(job).await.unwrap();
    }
    tx.close();

    // 6. Spawn Tokio workers
    let cancel_token = CancellationToken::new();
    let mut handles = Vec::with_capacity(plan.num_workers);

    for i in 0..plan.num_workers {
        let rx_clone = rx.clone();
        let worker_pb = ui.worker_pbs[i].clone();
        let global_pb = ui.global_pb.clone();
        let token_clone = cancel_token.clone();
        let chunk_size = plan.chunk_size_bytes;

        handles.push(tokio::spawn(async move {
            worker::run_file_worker(
                i,
                rx_clone,
                worker_pb,
                global_pb,
                chunk_size,
                token_clone,
            )
            .await
        }));
    }

    // 7. Await workers and propagate any I/O failure immediately
    let mut first_err = None;
    for handle in handles {
        if let Err(e) = handle.await.expect("Worker task panicked") {
            if first_err.is_none() && !matches!(e, error::CopyError::Cancelled) {
                first_err = Some(e);
            }
        }
    }

    if let Some(err) = first_err {
        ui.abandon_all();
        return Err(err);
    }

    ui.finish_all();
    eprintln!(
        "{} Copied {:.2} MB in {:.2?}",
        style("✔").green().bold(),
        manifest.total_bytes as f64 / (1024.0 * 1024.0),
        start.elapsed()
    );

    Ok(())
}
