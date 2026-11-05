//! Crate-level error type, assembled from each layer's own error enum.

use thiserror::Error;

use crate::engine::error::EngineError;
use crate::platform::device::ProbeError;
use crate::util::sync::Cancellation;

/// Every error the library can return.
#[derive(Debug, Error)]
pub enum Error {
    /// Disk detection failed.
    #[error(transparent)]
    Probe(#[from] ProbeError),
    /// Scanning or copying failed.
    #[error(transparent)]
    Engine(#[from] EngineError),
}

/// `Result` alias defaulting to the crate [`Error`](enum@Error).
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// Process exit code: `130` for a user interrupt, `1` for everything else.
    pub fn exit_code(&self) -> u8 {
        if self.is_cancellation() { 130 } else { 1 }
    }
}

impl Cancellation for Error {
    fn is_cancellation(&self) -> bool {
        matches!(self, Self::Engine(e) if e.is_cancellation())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelled_maps_to_exit_code_130() {
        assert_eq!(Error::from(EngineError::Cancelled).exit_code(), 130);
    }

    #[test]
    fn other_errors_map_to_exit_code_1() {
        let err = Error::from(EngineError::SourceNotFound("/x".into()));
        assert_eq!(err.exit_code(), 1);
    }
}
