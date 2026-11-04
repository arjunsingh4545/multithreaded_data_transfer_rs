use crate::error::{CopyError, Result};
use crate::scanner::{FileJob, SymlinkJob};
use async_channel::Receiver;
use indicatif::ProgressBar;
use std::path::PathBuf;
use tokio::fs::{self, File};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

const STREAM_THRESHOLD: u64 = 1024 * 1024; // 1 MB

/// Pre-creates the directory skeleton before file workers start
pub async fn create_directories(dirs: &[PathBuf]) -> Result<()> {
    for dir in dirs {
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(|source| CopyError::DirCreateError {
                path: dir.clone(),
                source,
            })?;
    }
    Ok(())
}

/// Recreates symlinks accurately across Unix and Windows
pub async fn create_symlinks(symlinks: &[SymlinkJob]) -> Result<()> {
    for link in symlinks {
        // Remove existing destination link/file if present so symlink creation succeeds
        if tokio::fs::symlink_metadata(&link.dst).await.is_ok() {
            let _ = tokio::fs::remove_file(&link.dst).await;
        }

        #[cfg(unix)]
        {
            tokio::fs::symlink(&link.target, &link.dst)
                .await
                .map_err(|source| CopyError::SymlinkCreateError {
                    dst: link.dst.clone(),
                    target: link.target.clone(),
                    source,
                })?;
        }

        #[cfg(windows)]
        {
            let is_dir = link
                .src
                .parent()
                .map(|p| p.join(&link.target))
                .and_then(|resolved| std::fs::metadata(resolved).ok())
                .map(|m| m.is_dir())
                .unwrap_or(false);

            let res = if is_dir {
                tokio::fs::symlink_dir(&link.target, &link.dst).await
            } else {
                tokio::fs::symlink_file(&link.target, &link.dst).await
            };

            res.map_err(|source| CopyError::SymlinkCreateError {
                dst: link.dst.clone(),
                target: link.target.clone(),
                source,
            })?;
        }
    }
    Ok(())
}

/// Async worker loop pulling jobs and updating its dedicated progress bar
pub async fn run_file_worker(
    _worker_id: usize,
    rx: Receiver<FileJob>,
    worker_pb: ProgressBar,
    global_pb: ProgressBar,
    chunk_size: usize,
    cancel_token: CancellationToken,
) -> Result<()> {
    let mut buffer = vec![0u8; chunk_size];

    while let Ok(job) = rx.recv().await {
        if cancel_token.is_cancelled() {
            return Err(CopyError::Cancelled);
        }

        let short_name = job
            .src
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| job.src.display().to_string());

        worker_pb.reset();
        worker_pb.set_length(job.size.max(1));
        worker_pb.set_message(short_name);

        let copy_res = if job.size < STREAM_THRESHOLD {
            // Fast kernel-offloaded copy for small files
            match fs::copy(&job.src, &job.dst).await {
                Ok(bytes) => {
                    worker_pb.set_position(bytes.max(1));
                    global_pb.inc(bytes);
                    Ok(())
                }
                Err(e) => Err(e),
            }
        } else {
            // Chunked async stream for large files with live per-chunk progress updates
            stream_large_file(&job, &mut buffer, &worker_pb, &global_pb, &cancel_token).await
        };

        if let Err(source) = copy_res {
            cancel_token.cancel();
            return Err(CopyError::FileCopyError {
                src: job.src,
                dst: job.dst,
                source,
            });
        }

        // Preserve original file permissions
        let _ = fs::set_permissions(&job.dst, job.permissions).await;
    }

    worker_pb.finish_and_clear();
    Ok(())
}

async fn stream_large_file(
    job: &FileJob,
    buffer: &mut [u8],
    worker_pb: &ProgressBar,
    global_pb: &ProgressBar,
    cancel_token: &CancellationToken,
) -> std::io::Result<()> {
    let mut reader = File::open(&job.src).await?;
    let mut writer = File::create(&job.dst).await?;

    loop {
        if cancel_token.is_cancelled() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "cancelled",
            ));
        }

        let n = reader.read(buffer).await?;
        if n == 0 {
            break;
        }

        writer.write_all(&buffer[..n]).await?;
        worker_pb.inc(n as u64);
        global_pb.inc(n as u64);
    }

    writer.flush().await?;
    Ok(())
}
