//! Copy ordering policies.
//!
//! On a hard disk the order in which files are read decides how far the
//! head travels. [`OrderKind`] is the runtime choice; each variant maps onto
//! a statically dispatched [`OrderingPolicy`].

use std::cmp::Reverse;

use rayon::prelude::*;

use crate::domain::job::FileJob;
use crate::domain::plan::OrderKind;
use crate::platform::extent::ExtentLocator;

/// Reorders jobs in place.
pub trait OrderingPolicy {
    /// Sorts `jobs` into copy order.
    fn order(&self, jobs: &mut [FileJob]);
}

/// Largest files first.
#[derive(Debug, Clone, Copy, Default)]
pub struct LargestFirst;

impl OrderingPolicy for LargestFirst {
    fn order(&self, jobs: &mut [FileJob]) {
        jobs.sort_by_key(|job| Reverse(job.size));
    }
}

/// Lexicographic source path (groups directories together).
#[derive(Debug, Clone, Copy, Default)]
pub struct ByPath;

impl OrderingPolicy for ByPath {
    fn order(&self, jobs: &mut [FileJob]) {
        jobs.sort_by(|a, b| a.src.cmp(&b.src));
    }
}

/// Ascending source inode number.
#[derive(Debug, Clone, Copy, Default)]
pub struct ByInode;

impl OrderingPolicy for ByInode {
    fn order(&self, jobs: &mut [FileJob]) {
        jobs.sort_by_key(|job| job.ino);
    }
}

/// Ascending physical offset of each file's first extent.
///
/// Files the locator cannot place go last, in path order; if none can be
/// placed the result equals [`ByPath`].
#[derive(Debug, Clone, Copy, Default)]
pub struct ByPhysical<L> {
    locator: L,
}

impl<L> ByPhysical<L> {
    /// Orders using `locator`.
    pub const fn new(locator: L) -> Self {
        Self { locator }
    }
}

impl<L: ExtentLocator + Sync> OrderingPolicy for ByPhysical<L> {
    fn order(&self, jobs: &mut [FileJob]) {
        // Lookups are independent syscalls, so run them in parallel, then sort indices.
        let offsets: Vec<Option<u64>> = jobs
            .par_iter()
            .map(|job| self.locator.first_physical_offset(&job.src).ok().flatten())
            .collect();

        let mut perm: Vec<usize> = (0..jobs.len()).collect();
        perm.sort_by(|&a, &b| {
            let key = |i: usize| (offsets[i].is_none(), offsets[i]);
            key(a)
                .cmp(&key(b))
                .then_with(|| jobs[a].src.cmp(&jobs[b].src))
        });
        apply_permutation(jobs, perm);
    }
}

/// Sorts `jobs` with the policy that `kind` names.
///
/// ```
/// use multithreaded_data_transfer_rs::OrderKind;
/// use multithreaded_data_transfer_rs::engine::order::sort_jobs;
/// use multithreaded_data_transfer_rs::platform::extent::NoLocator;
///
/// let mut jobs = Vec::new(); // normally from a scan
/// sort_jobs(OrderKind::Physical, &mut jobs, &NoLocator);
/// ```
pub fn sort_jobs<L: ExtentLocator + Sync>(kind: OrderKind, jobs: &mut [FileJob], locator: &L) {
    match kind {
        OrderKind::Largest => LargestFirst.order(jobs),
        OrderKind::Path => ByPath.order(jobs),
        OrderKind::Inode => ByInode.order(jobs),
        OrderKind::Physical => ByPhysical::new(locator).order(jobs),
    }
}

/// Rearranges `items` so that `items[i]` becomes the element previously at `perm[i]`.
///
/// Runs in place by following permutation cycles: O(n) swaps, no clones.
fn apply_permutation<T>(items: &mut [T], mut perm: Vec<usize>) {
    for start in 0..items.len() {
        let mut current = start;
        loop {
            let source = perm[current];
            perm[current] = current;
            if source == start {
                break;
            }
            items.swap(current, source);
            current = source;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::job::fixtures::job_at;
    use std::collections::HashMap;
    use std::io;
    use std::path::{Path, PathBuf};

    /// Locator answering from a map; demonstrates the generic seam.
    struct FakeLocator(HashMap<PathBuf, u64>);

    impl ExtentLocator for FakeLocator {
        fn first_physical_offset(&self, path: &Path) -> io::Result<Option<u64>> {
            Ok(self.0.get(path).copied())
        }
    }

    fn jobs() -> Vec<FileJob> {
        vec![
            job_at("/b/1", 10, 30),
            job_at("/a/2", 30, 10),
            job_at("/c/3", 20, 20),
        ]
    }

    fn srcs(jobs: &[FileJob]) -> Vec<&str> {
        jobs.iter().map(|j| j.src.to_str().unwrap()).collect()
    }

    #[test]
    fn largest_first_sorts_descending() {
        let mut j = jobs();
        LargestFirst.order(&mut j);
        assert_eq!(srcs(&j), ["/a/2", "/c/3", "/b/1"]);
    }

    #[test]
    fn by_path_groups_directories() {
        let mut j = jobs();
        ByPath.order(&mut j);
        assert_eq!(srcs(&j), ["/a/2", "/b/1", "/c/3"]);
    }

    #[test]
    fn by_inode_sorts_ascending() {
        let mut j = jobs();
        ByInode.order(&mut j);
        assert_eq!(srcs(&j), ["/a/2", "/c/3", "/b/1"]);
    }

    #[test]
    fn physical_uses_locator_offsets() {
        let locator = FakeLocator(HashMap::from([
            ("/a/2".into(), 300),
            ("/b/1".into(), 100),
            ("/c/3".into(), 200),
        ]));
        let mut j = jobs();
        ByPhysical::new(locator).order(&mut j);
        assert_eq!(srcs(&j), ["/b/1", "/c/3", "/a/2"]);
    }

    #[test]
    fn physical_puts_unlocated_last_in_path_order() {
        let locator = FakeLocator(HashMap::from([("/c/3".into(), 5)]));
        let mut j = jobs();
        ByPhysical::new(locator).order(&mut j);
        assert_eq!(srcs(&j), ["/c/3", "/a/2", "/b/1"]);
    }

    #[test]
    fn physical_falls_back_to_path_when_nothing_located() {
        let mut j = jobs();
        sort_jobs(
            OrderKind::Physical,
            &mut j,
            &crate::platform::extent::NoLocator,
        );
        assert_eq!(srcs(&j), ["/a/2", "/b/1", "/c/3"]);
    }

    #[test]
    fn apply_permutation_handles_cycles() {
        let mut items = ['a', 'b', 'c', 'd'];
        apply_permutation(&mut items, vec![2, 0, 3, 1]);
        assert_eq!(items, ['c', 'a', 'd', 'b']);
    }
}
