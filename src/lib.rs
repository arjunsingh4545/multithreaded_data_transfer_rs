//! Disk-aware directory copy.
//!
//! The crate picks a copy strategy based on the kind of disk on each side:
//! many concurrent copies for SSDs, and a sequential reader → writer pipeline
//! with large buffers when a rotational disk is involved.
//!
//! # Layers
//!
//! | module       | role                                                    |
//! |--------------|---------------------------------------------------------|
//! | [`cli`]      | terminal input/output only (clap, progress bars)        |
//! | [`engine`]   | runs a copy: prepare → scan → order → execute           |
//! | [`domain`]   | pure types and decisions, no I/O                        |
//! | [`platform`] | OS adapters; the only home of `libc` and `unsafe`       |
//! | [`util`]     | small helpers that know nothing about copying           |
//!
//! Each layer depends only on the layers below it.
//!
//! # Example
//!
//! ```no_run
//! use multithreaded_data_transfer_rs::{CopyRequest, Engine, NoProgress};
//! use tokio_util::sync::CancellationToken;
//!
//! let engine = Engine::new();
//! let prepared = engine.prepare(&CopyRequest::new("/mnt/a", "/mnt/b"), &NoProgress)?;
//! println!("{}", prepared.plan());
//! let summary = engine.execute(&prepared, &NoProgress, &CancellationToken::new())?;
//! println!("copied {} files", summary.stats.files);
//! # Ok::<(), multithreaded_data_transfer_rs::Error>(())
//! ```

#[cfg(not(unix))]
compile_error!("this crate relies on Unix positional I/O and currently supports Unix only");

pub mod cli;
pub mod domain;
pub mod engine;
pub mod error;
pub mod platform;
pub mod util;

pub use crate::domain::device::DiskKind;
pub use crate::domain::plan::{CopyPlan, Mode, OrderKind, Strategy, Tuning};
pub use crate::domain::request::{CopyRequest, ExistingPolicy};
pub use crate::domain::topology::{KindOverrides, Topology, TransferKind};
pub use crate::engine::progress::{NoProgress, ProgressSink, Role, WorkerIdx};
pub use crate::engine::{CopyStats, Engine, Prepared, Summary};
pub use crate::error::{Error, Result};
pub use crate::util::units::ByteSize;
