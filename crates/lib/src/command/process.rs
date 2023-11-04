use std::process::ExitStatus;

use command_group::tokio::ErasedChild;
use tracing::{debug, trace};

use crate::error::RuntimeError;

/// Low-level wrapper around a process child, be it grouped or ungrouped.
#[derive(Debug)]
pub enum Process {
	/// The initial state of the process, before it's spawned.
	None,

	/// A process that's been spawned.
	Spawned(ErasedChild),

	/// The cached exit status of the process.
	Done(ExitStatus),
}

impl Default for Process {
	/// Returns [`Process::None`].
	fn default() -> Self {
		Self::None
	}
}

impl Process {
	/// Sends a Unix signal to the process.
	///
	/// Does nothing if the process is not running.
	#[cfg(unix)]
	pub fn signal(&mut self, sig: command_group::Signal) -> Result<(), RuntimeError> {
		match self {
			Self::None | Self::Done(_) => Ok(()),
			Self::Spawned(c) => {
				debug!(signal=%sig, pid=?c.id(), "sending signal to process");
				c.signal(sig)
			}
		}
		.map_err(RuntimeError::Process)
	}

	/// Kills the process.
	///
	/// Does nothing if the process is not running.
	pub async fn kill(&mut self) -> Result<(), RuntimeError> {
		match self {
			Self::None | Self::Done(_) => Ok(()),
			Self::Spawned(c) => {
				debug!(pid=?c.id(), "killing process");
				c.kill().await
			}
		}
		.map_err(RuntimeError::Process)
	}

	/// Checks the status of the process.
	///
	/// Returns `true` if the process is still running.
	///
	/// This takes `&mut self` as it transitions the [`Process`] state to [`Process::Done`] if it
	/// finds the process has ended, such that it will cache the exit status. Otherwise that status
	/// would be lost.
	///
	/// Does nothing and returns `false` immediately if the `Process` is `Done` or `None`.
	pub fn is_running(&mut self) -> Result<bool, RuntimeError> {
		match self {
			Self::None | Self::Done(_) => Ok(false),
			Self::Spawned(c) => c.try_wait().map(|status| {
				trace!("try-waiting on process");
				if let Some(status) = status {
					trace!(?status, "converting to ::Done");
					*self = Self::Done(status);
					true
				} else {
					false
				}
			}),
		}
		.map_err(RuntimeError::Process)
	}

	/// Waits for the process to exit, and returns its exit status.
	///
	/// This takes `&mut self` as it transitions the [`Process`] state to [`Process::Done`] if it
	/// finds the process has ended, such that it will cache the exit status.
	///
	/// This makes it possible to call `wait` on a process multiple times, without losing the exit
	/// status.
	///
	/// Returns immediately with the cached exit status if the `Process` is `Done`, and with `None`
	/// if the `Process` is `None`.
	pub async fn wait(&mut self) -> Result<Option<ExitStatus>, RuntimeError> {
		match self {
			Self::None => Ok(None),
			Self::Done(status) => Ok(Some(*status)),
			Self::Spawned(c) => {
				trace!("waiting on process");
				let status = c.wait().await.map_err(|err| RuntimeError::IoError {
					about: "waiting on process",
					err,
				})?;
				trace!(?status, "converting to ::Done");
				*self = Self::Done(status);
				Ok(Some(status))
			}
		}
		.map_err(RuntimeError::Process)
	}
}
