//! A lock-free cursor that hands out jobs in planned order.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::domain::job::{FileId, FileJob};

/// Shares a borrowed job slice between worker threads.
///
/// Jobs are handed out exactly once, in slice order, so the planned copy
/// order is preserved even with several workers.
#[derive(Debug)]
pub struct JobQueue<'a> {
    jobs: &'a [FileJob],
    next: AtomicUsize,
}

impl<'a> JobQueue<'a> {
    /// A queue over `jobs`.
    pub const fn new(jobs: &'a [FileJob]) -> Self {
        Self {
            jobs,
            next: AtomicUsize::new(0),
        }
    }

    /// Claims the next job, or `None` when all have been handed out.
    pub fn next(&self) -> Option<(FileId, &'a FileJob)> {
        let index = self.next.fetch_add(1, Ordering::Relaxed);
        self.jobs.get(index).map(|job| (FileId::new(index), job))
    }

    /// Looks up a job by id.
    ///
    /// # Panics
    ///
    /// If `id` did not come from this queue.
    pub fn get(&self, id: FileId) -> &'a FileJob {
        &self.jobs[id.index()]
    }

    /// Total number of jobs.
    pub const fn len(&self) -> usize {
        self.jobs.len()
    }

    /// `true` when there are no jobs at all.
    pub const fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::job::fixtures::job;
    use std::collections::HashSet;
    use std::sync::Mutex;

    #[test]
    fn hands_out_jobs_in_order() {
        let jobs = [job(1), job(2)];
        let queue = JobQueue::new(&jobs);
        let sizes: Vec<u64> = std::iter::from_fn(|| queue.next())
            .map(|(_, j)| j.size)
            .collect();
        assert_eq!(sizes, [1, 2]);
    }

    #[test]
    fn hands_out_each_job_exactly_once_across_threads() {
        let jobs: Vec<_> = (0..1000).map(job).collect();
        let queue = JobQueue::new(&jobs);
        let seen = Mutex::new(Vec::new());
        std::thread::scope(|s| {
            for _ in 0..8 {
                s.spawn(|| {
                    while let Some((id, _)) = queue.next() {
                        seen.lock().unwrap().push(id);
                    }
                });
            }
        });
        let seen = seen.into_inner().unwrap();
        let unique: HashSet<_> = seen.iter().copied().collect();
        assert_eq!((seen.len(), unique.len()), (1000, 1000));
    }

    #[test]
    fn get_returns_job_by_id() {
        let jobs = [job(1), job(2)];
        assert_eq!(JobQueue::new(&jobs).get(FileId::new(1)).size, 2);
    }
}
