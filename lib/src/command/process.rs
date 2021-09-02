use std::process::ExitStatus;

use command_group::{AsyncGroupChild, Signal};
use tokio::process::Child;
use tracing::{debug, trace};

use crate::error::RuntimeError;

#[derive(Debug)]
pub enum Process {
	None,
	Grouped(AsyncGroupChild),
	Ungrouped(Child),
	Done(ExitStatus),
}

impl Default for Process {
	fn default() -> Self {
		Process::None
	}
}

impl Process {
	#[cfg(unix)]
	pub fn signal(&mut self, sig: Signal) -> Result<(), RuntimeError> {
		use command_group::UnixChildExt;

		match self {
			Self::None | Self::Done(_) => Ok(()),
			Self::Grouped(c) => {
				debug!(signal=%sig, pgid=?c.id(), "sending signal to process group");
				c.signal(sig)
			}
			Self::Ungrouped(c) => {
				debug!(signal=%sig, pid=?c.id(), "sending signal to process");
				c.signal(sig)
			}
		}
		.map_err(RuntimeError::Process)
	}

	pub async fn kill(&mut self) -> Result<(), RuntimeError> {
		match self {
			Self::None | Self::Done(_) => Ok(()),
			Self::Grouped(c) => {
				debug!(pgid=?c.id(), "killing process group");
				c.kill()
			}
			Self::Ungrouped(c) => {
				debug!(pid=?c.id(), "killing process");
				c.kill().await
			}
		}
		.map_err(RuntimeError::Process)
	}

	pub fn is_running(&mut self) -> Result<bool, RuntimeError> {
		match self {
			Self::None | Self::Done(_) => Ok(false),
			Self::Grouped(c) => c.try_wait().map(|status| {
				trace!("try-waiting on process group");
				if let Some(status) = status {
					trace!(?status, "converting to ::Done");
					*self = Self::Done(status);
					true
				} else {
					false
				}
			}),
			Self::Ungrouped(c) => c.try_wait().map(|status| {
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

	pub async fn wait(&mut self) -> Result<Option<ExitStatus>, RuntimeError> {
		match self {
			Self::None => Ok(None),
			Self::Done(status) => Ok(Some(*status)),
			Self::Grouped(c) => {
				trace!("waiting on process group");
				let status = c.wait().await?;
				trace!(?status, "converting to ::Done");
				*self = Self::Done(status);
				Ok(Some(status))
			}
			Self::Ungrouped(c) => {
				trace!("waiting on process");
				let status = c.wait().await?;
				trace!(?status, "converting to ::Done");
				*self = Self::Done(status);
				Ok(Some(status))
			}
		}
		.map_err(RuntimeError::Process)
	}
}
