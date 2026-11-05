//! Errors raised while preparing or executing a copy.

use std::io;
use std::path::PathBuf;

use thiserror::Error;

use super::progress::Role;
use crate::domain::job::FileJob;
use crate::util::sync::Cancellation;

/// Something went wrong while scanning or copying.
#[derive(Debug, Error)]
pub enum EngineError {
    /// The source path does not exist.
    #[error("source '{0}' does not exist")]
    SourceNotFound(PathBuf),
    /// The source path is not a directory.
    #[error("source '{0}' is not a directory")]
    SourceNotDir(PathBuf),
    /// The destination path could not be resolved.
    #[error("cannot resolve destination '{path}': {source}")]
    DestUnresolvable {
        /// Destination as given.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: io::Error,
    },
    /// The destination is the source or lies inside it.
    #[error("destination '{dst}' is inside source '{src}'; this would copy the tree into itself")]
    DestInsideSource {
        /// Source as given.
        src: PathBuf,
        /// Destination as given.
        dst: PathBuf,
    },
    /// `--if-exists fail` found an existing file.
    #[error("'{0}' already exists (policy: fail)")]
    DestExists(PathBuf),
    /// A source directory could not be listed.
    #[error("cannot read directory '{path}': {source}")]
    DirAccess {
        /// Directory path.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: io::Error,
    },
    /// An entry's metadata could not be read.
    #[error("cannot inspect '{path}': {source}")]
    Metadata {
        /// Entry path.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: io::Error,
    },
    /// A symlink could not be read or created.
    #[error("cannot copy symlink '{path}': {source}")]
    Symlink {
        /// Link path.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: io::Error,
    },
    /// A destination directory could not be created.
    #[error("cannot create directory '{path}': {source}")]
    DirCreate {
        /// Directory path.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: io::Error,
    },
    /// Copying a file's contents failed.
    #[error("cannot copy '{src}' → '{dst}': {source}")]
    FileCopy {
        /// Source file.
        src: PathBuf,
        /// Destination file.
        dst: PathBuf,
        /// Underlying error.
        #[source]
        source: io::Error,
    },
    /// A worker thread could not be started.
    #[error("cannot start worker thread: {0}")]
    ThreadSpawn(#[source] io::Error),
    /// A worker thread panicked.
    #[error("a {role} thread panicked")]
    WorkerPanicked {
        /// Which kind of worker.
        role: Role,
    },
    /// The copy was interrupted (Ctrl-C or a failure elsewhere).
    #[error("operation cancelled")]
    Cancelled,
}

impl EngineError {
    /// An error constructor for I/O failures while copying `job`.
    ///
    /// The returned closure borrows `job`, so it can be passed straight to `map_err`.
    pub fn copy_failed(job: &FileJob) -> impl Fn(io::Error) -> Self + '_ {
        move |source| Self::FileCopy {
            src: job.src.clone(),
            dst: job.dst.clone(),
            source,
        }
    }
}

impl Cancellation for EngineError {
    fn is_cancellation(&self) -> bool {
        matches!(self, Self::Cancelled)
    }
}
