//! Watchexec's process supervisor.
//!
//! This crate implements the process supervisor for Watchexec. It is responsible for spawning and
//! managing processes, and for sending events to them.
//!
//! You may use this crate to implement your own process supervisor, but keep in mind its direction
//! will always primarily be driven by the needs of Watchexec itself.

pub mod command;
mod flag;
pub mod job;

// #[doc(inline)]
// pub use supervisor::{Args, Supervisor, SupervisorId};

// Supervisor -> Job(Command, runtime info, control (priority) queue) -> ErasedChild
