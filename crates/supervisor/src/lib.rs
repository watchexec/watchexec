//! Watchexec's process supervisor.
//!
//! This crate implements the process supervisor for Watchexec. It is responsible for spawning and
//! managing processes, and for sending events to them.
//!
//! You may use this crate to implement your own process supervisor, but keep in mind its direction
//! will always primarily be driven by the needs of Watchexec itself.
//!
//! # Usage
//!
//! There is no struct or implementation of a single supervisor, as the particular needs of the
//! application will dictate how that is designed. Instead, this crate provides a [`Job`](job::Job)
//! construct, which is a handle to a single [`Command`](command::Command), and manages its
//! lifecycle. The `Job` API has been modeled after the `systemctl` set of commands for service
//! control, with operations for starting, stopping, restarting, sending signals, waiting for the
//! process to complete, etc.
//!
//! There are also methods for running hooks within the job's runtime task, and for handling errors.
//!
//! # Theory of Operation
//!
//! A [`Job`](job::Job) is, properly speaking, a handle which lets one control a Tokio task. That
//! task is spawned on the Tokio runtime, and so runs in the background. A `Job` takes as input a
//! [`Command`](command::Command), which describes how to start a single process, through either a
//! shell command or a direct executable invocation, and if the process should be grouped (using
//! [`process-wrap`](process_wrap)) or not.
//!
//! The job's task runs an event loop on two sources: the process's `wait()` (i.e. when the process
//! ends) and the job's control queue. The control queue is a hybrid MPSC queue, with three priority
//! levels and a timer. When the timer is active, the lowest ("Normal") priority queue is disabled.
//! This is an internal detail which serves to implement graceful stops and restarts. The internals
//! of the job's task are not available to the API user, actions and queries are performed by
//! sending messages on this control queue.
//!
//! The control queue is executed in priority and in order within priorities. Sending a control to
//! the task returns a [`Ticket`](job::Ticket), which is a future that resolves when the control has
//! been processed. Dropping the ticket will not cancel the control. This provides two complementary
//! ways to orchestrate actions: queueing controls in the desired order if there is no need for
//! branching flow or for signaling, and sending controls or performing other actions after awaiting
//! tickets.
//!
//! Do note that both of these can be used together. There is no need for the below pattern:
//!
//! ```no_run
//! # #[tokio::main(flavor = "current_thread")] async fn main() { // single-threaded for doctest only
//! # use std::sync::Arc;
//! # use watchexec_supervisor::Signal;
//! # use watchexec_supervisor::command::{Command, Program};
//! # use watchexec_supervisor::job::{CommandState, start_job};
//! #
//! # let (job, task) = start_job(Arc::new(Command { program: Program::Exec { prog: "/bin/date".into(), args: Vec::new() }.into(), options: Default::default() }));
//! #
//! job.start().await;
//! job.signal(Signal::User1).await;
//! job.stop().await;
//! # task.abort();
//! # }
//! ```
//!
//! Because of ordering, it behaves the same as this:
//!
//! ```no_run
//! # #[tokio::main(flavor = "current_thread")] async fn main() { // single-threaded for doctest only
//! # use std::sync::Arc;
//! # use watchexec_supervisor::Signal;
//! # use watchexec_supervisor::command::{Command, Program};
//! # use watchexec_supervisor::job::{CommandState, start_job};
//! #
//! # let (job, task) = start_job(Arc::new(Command { program: Program::Exec { prog: "/bin/date".into(), args: Vec::new() }.into(), options: Default::default() }));
//! #
//! job.start();
//! job.signal(Signal::User1);
//! job.stop().await; // here, all of start(), signal(), and stop() will have run in order
//! # task.abort();
//! # }
//! ```
//!
//! However, this is a different program:
//!
//! ```no_run
//! # #[tokio::main(flavor = "current_thread")] async fn main() { // single-threaded for doctest only
//! # use std::sync::Arc;
//! # use std::time::Duration;
//! # use tokio::time::sleep;
//! # use watchexec_supervisor::Signal;
//! # use watchexec_supervisor::command::{Command, Program};
//! # use watchexec_supervisor::job::{CommandState, start_job};
//! #
//! # let (job, task) = start_job(Arc::new(Command { program: Program::Exec { prog: "/bin/date".into(), args: Vec::new() }.into(), options: Default::default() }));
//! #
//! job.start().await;
//! println!("program started!");
//! sleep(Duration::from_secs(5)).await; // wait until program is fully started
//!
//! job.signal(Signal::User1).await;
//! sleep(Duration::from_millis(150)).await; // wait until program has dumped stats
//! println!("program stats dumped via USR1 signal!");
//!
//! job.stop().await;
//! println!("program stopped");
//! #
//! # task.abort();
//! # }
//! ```
//!
//! # Example
//!
//! ```no_run
//! # #[tokio::main(flavor = "current_thread")] async fn main() { // single-threaded for doctest only
//! # use std::sync::Arc;
//! use watchexec_supervisor::Signal;
//! use watchexec_supervisor::command::{Command, Program};
//! use watchexec_supervisor::job::{CommandState, start_job};
//!
//! let (job, task) = start_job(Arc::new(Command {
//!     program: Program::Exec {
//!         prog: "/bin/date".into(),
//!         args: Vec::new(),
//!     }.into(),
//!     options: Default::default(),
//! }));
//!
//! job.start().await;
//! job.signal(Signal::User1).await;
//! job.stop().await;
//!
//! job.delete_now().await;
//!
//! task.await; // make sure the task is fully cleaned up
//! # }
//! ```

#![doc(html_favicon_url = "https://watchexec.github.io/logo:watchexec.svg")]
#![doc(html_logo_url = "https://watchexec.github.io/logo:watchexec.svg")]
#![warn(clippy::unwrap_used, missing_docs, rustdoc::unescaped_backticks)]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![deny(rust_2018_idioms)]

#[doc(no_inline)]
pub use watchexec_events::ProcessEnd;
#[doc(no_inline)]
pub use watchexec_signals::Signal;

pub mod command;
pub mod errors;
pub mod job;

mod flag;
