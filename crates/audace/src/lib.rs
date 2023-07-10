//! Handle PID 1 duties on Unix.
//!
//! This library should be used to make a binary able to handle being called as PID 1, aka the
//! init process. It is primarily written for applications running in containers.
//!
//! This library handles:
//! - setting the correct sigmask
//! - reaping zombies
//! - propagating signals
//!
//! It is not: a process supervisor.
//!
//! # Example
//!
//! In most cases, you should use:
//!
//! ```
//! fn main() {
//!     audace::install();
//! }
//! ```
