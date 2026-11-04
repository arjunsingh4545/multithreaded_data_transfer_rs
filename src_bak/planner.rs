use crate::scanner::ScanManifest;

#[derive(Debug, Clone, Copy)]
pub struct WorkerPlan {
    pub num_workers: usize,
    pub chunk_size_bytes: usize,
}

/// Dynamically allocates workers based on file count, average file size, and CPU cores.
pub fn compute_worker_plan(manifest: &ScanManifest, max_override: Option<usize>) -> WorkerPlan {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    let file_count = manifest.files.len();
    if file_count == 0 {
        return WorkerPlan {
            num_workers: 1,
            chunk_size_bytes: 64 * 1024,
        };
    }

    let avg_size = manifest.total_bytes / (file_count as u64);

    let optimal_workers = if let Some(custom) = max_override {
        custom.max(1)
    } else if avg_size < 64 * 1024 {
        // Metadata/syscall heavy (lots of tiny files): oversubscribe cores 2x (up to 32)
        (cores * 2).min(32).min(file_count)
    } else if avg_size > 16 * 1024 * 1024 {
        // Bandwidth heavy (large files): cap workers to avoid NVMe/disk thrashing
        cores.clamp(2, 8).min(file_count)
    } else {
        // Balanced workload
        cores.clamp(4, 16).min(file_count)
    };

    let chunk_size_bytes = if avg_size > 8 * 1024 * 1024 {
        256 * 1024 // 256 KB buffer for large streams
    } else {
        64 * 1024 // 64 KB buffer for general files
    };

    WorkerPlan {
        num_workers: optimal_workers,
        chunk_size_bytes,
    }
}
