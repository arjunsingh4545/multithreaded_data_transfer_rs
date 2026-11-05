//! Concurrency helpers shared by the scanner, parallel and pipeline paths.

use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread::ScopedJoinHandle;

/// Locks a mutex, recovering the data if another thread panicked while holding it.
pub trait LockExt<T: ?Sized> {
    /// Like [`Mutex::lock`], but never fails.
    fn lock_or_recover(&self) -> MutexGuard<'_, T>;
}

impl<T: ?Sized> LockExt<T> for Mutex<T> {
    fn lock_or_recover(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Errors that may simply mean "stopped because something else failed or the user asked".
pub trait Cancellation {
    /// `true` when this error is a cancellation rather than a root cause.
    fn is_cancellation(&self) -> bool;
}

/// Picks the most useful error out of many workers' results.
///
/// A real failure wins over a cancellation (cancellations are usually a
/// consequence of that failure). Returns `Ok` only if every result is `Ok`.
pub fn first_error<E: Cancellation>(
    results: impl IntoIterator<Item = Result<(), E>>,
) -> Result<(), E> {
    let mut cancelled = None;
    for err in results.into_iter().filter_map(Result::err) {
        if !err.is_cancellation() {
            return Err(err);
        }
        cancelled.get_or_insert(err);
    }
    cancelled.map_or(Ok(()), Err)
}

/// Joins a scoped thread, turning a panic into the error produced by `on_panic`.
pub fn join_scoped<T, E>(
    handle: ScopedJoinHandle<'_, Result<T, E>>,
    on_panic: impl FnOnce() -> E,
) -> Result<T, E> {
    handle.join().unwrap_or_else(|_| Err(on_panic()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    enum TestError {
        Cancelled,
        Real(u8),
    }

    impl Cancellation for TestError {
        fn is_cancellation(&self) -> bool {
            matches!(self, Self::Cancelled)
        }
    }

    #[test]
    fn first_error_is_ok_when_all_ok() {
        assert_eq!(first_error::<TestError>([Ok(()), Ok(())]), Ok(()));
    }

    #[test]
    fn first_error_skips_cancelled() {
        let results = [Err(TestError::Cancelled), Err(TestError::Real(7))];
        assert_eq!(first_error(results), Err(TestError::Real(7)));
    }

    #[test]
    fn first_error_reports_cancelled_when_nothing_else_failed() {
        let results = [Ok(()), Err(TestError::Cancelled)];
        assert_eq!(first_error(results), Err(TestError::Cancelled));
    }

    #[test]
    fn join_scoped_maps_panic_to_error() {
        let result = std::thread::scope(|s| {
            let handle = s.spawn(|| -> Result<(), TestError> { panic!("boom") });
            join_scoped(handle, || TestError::Real(1))
        });
        assert_eq!(result, Err(TestError::Real(1)));
    }

    #[test]
    fn lock_or_recover_survives_poison() {
        let mutex = Mutex::new(5);
        let _ = std::thread::scope(|s| {
            s.spawn(|| {
                let _guard = mutex.lock().unwrap();
                panic!("poison");
            })
            .join()
        });
        assert_eq!(*mutex.lock_or_recover(), 5);
    }
}
