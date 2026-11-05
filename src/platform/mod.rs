//! OS adapters. Each trait lives next to its implementations; this is the only
//! layer allowed to call `libc` or use `unsafe`.

pub mod device;
pub mod extent;
pub mod ffi;
pub mod hints;
