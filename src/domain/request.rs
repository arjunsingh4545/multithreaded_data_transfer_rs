//! What the user asked for, independent of how it was asked (CLI, library call…).

use std::path::PathBuf;

use super::plan::Tuning;
use super::topology::KindOverrides;

/// What to do with files that already exist at the destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExistingPolicy {
    /// Replace them.
    #[default]
    Overwrite,
    /// Leave them untouched.
    Skip,
    /// Skip only files whose size and modification time already match (resume).
    Update,
    /// Abort before copying anything.
    Fail,
}

/// A complete description of one copy.
#[derive(Debug, Clone, Default)]
pub struct CopyRequest {
    /// Source directory.
    pub src: PathBuf,
    /// Destination directory (created if missing).
    pub dst: PathBuf,
    /// Strategy and performance knobs.
    pub tuning: Tuning,
    /// Disk-kind overrides.
    pub overrides: KindOverrides,
    /// Handling of existing destination files.
    pub existing: ExistingPolicy,
}

impl CopyRequest {
    /// A request with default tuning.
    pub fn new(src: impl Into<PathBuf>, dst: impl Into<PathBuf>) -> Self {
        Self {
            src: src.into(),
            dst: dst.into(),
            ..Self::default()
        }
    }

    /// Sets the tuning.
    #[must_use]
    pub fn with_tuning(mut self, tuning: Tuning) -> Self {
        self.tuning = tuning;
        self
    }

    /// Sets the disk-kind overrides.
    #[must_use]
    pub fn with_overrides(mut self, overrides: KindOverrides) -> Self {
        self.overrides = overrides;
        self
    }

    /// Sets the existing-file policy.
    #[must_use]
    pub fn with_existing(mut self, existing: ExistingPolicy) -> Self {
        self.existing = existing;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_sets_existing_policy() {
        let req = CopyRequest::new("a", "b").with_existing(ExistingPolicy::Update);
        assert_eq!(req.existing, ExistingPolicy::Update);
    }

    #[test]
    fn default_policy_is_overwrite() {
        assert_eq!(
            CopyRequest::new("a", "b").existing,
            ExistingPolicy::Overwrite
        );
    }
}
