//! Pure types and decisions. Nothing in this layer performs I/O, so every rule
//! here can be unit-tested with plain values.

pub mod device;
pub mod job;
pub mod plan;
pub mod request;
pub mod topology;
