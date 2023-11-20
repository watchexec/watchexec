//! Watchexec's process supervisor.
//!
//! This crate implements the process supervisor for Watchexec. It is responsible for spawning and
//! managing processes, and for sending events to them.
//!
//! You may use this crate to implement your own process supervisor, but keep in mind its direction
//! will always primarily be driven by the needs of Watchexec itself.

use command::Command;
use job::{start_job, Job};
use tokio::task::JoinSet;

pub mod command;
mod errors;
mod flag;
pub mod job;

/// The supervisor.
///
/// A supervisor in this crate is a simple structure: it wraps the [`JoinSet`] that holds the tasks
/// running the [`Job`]s that it manages, and keeps a bag of the handles to those jobs.
///
/// To start a job, call [`Supervisor::add`]. To end it, call [`Job::delete`]. To list all jobs, or
/// obtain one or more of them, get an iterator with [`Supervisor::list`].
///
/// To abort all jobs, drop the supervisor. To get a future that completes when all jobs are done,
/// call [`Supervisor::wait`].
///
/// If you start lots of jobs and then delete them without starting any new ones, you may want to
/// call [`Supervisor::gc`] to clean up the internal lists. This is called internally on `add()` and
/// within `wait()`.
#[derive(Debug, Default)]
pub struct Supervisor {
	tasks: JoinSet<()>,
	jobs: Vec<Job>,
}

impl Supervisor {
	/// Create and spawn a new [`Job`].
	pub fn add(&mut self, command: Command) -> Job {
		let job = start_job(&mut self.tasks, command);
		self.jobs.push(job.clone());
		self.gc();
		job
	}

	/// An iterator of alive jobs.
	pub fn list(&self) -> impl Iterator<Item=Job> + '_ {
		self.jobs.iter().filter(|job| !job.is_dead()).cloned()
	}

	/// Clear out dead jobs.
	pub fn gc(&mut self) {
		self.jobs.retain(|job| !job.is_dead());
	}

	/// Wait for all jobs to finish.
	pub async fn wait(&mut self) {
		while self.tasks.join_next().await.is_some() {
			self.gc();
		}
	}
}
