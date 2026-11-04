use std::io;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CopyError {
    #[error("Access denied or directory unreadable at '{path}': {source}")]
    DirectoryAccessDenied {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("Failed to inspect metadata for '{path}': {source}")]
    MetadataError {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("Failed to read symlink target at '{path}': {source}")]
    SymlinkReadError {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("Failed to create symlink at '{dst}' pointing to '{target}': {source}")]
    SymlinkCreateError {
        dst: PathBuf,
        target: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("Failed to create directory '{path}': {source}")]
    DirCreateError {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("I/O error while copying '{src}' -> '{dst}': {source}")]
    FileCopyError {
        src: PathBuf,
        dst: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("Source path '{0}' does not exist")]
    SourceNotFound(PathBuf),

    #[error("Operation aborted due to an earlier fatal error")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, CopyError>;
