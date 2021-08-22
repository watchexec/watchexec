//! Command construction and configuration thereof.

use std::process::ExitStatus;

use command_group::{AsyncGroupChild, Signal};
use tokio::process::{Child, Command};
use tracing::{debug, trace};

use crate::error::RuntimeError;

/// Shell to use to run commands.
///
/// `Cmd` and `Powershell` are special-cased because they have different calling
/// conventions. Also `Cmd` is only available in Windows, while `Powershell` is
/// also available on unices (provided the end-user has it installed, of course).
///
/// See [`Config.cmd`] for the semantics of `None` vs the
/// other options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shell {
	/// Use no shell, and execute the command directly.
	None,

	/// Use the given string as a unix shell invocation.
	///
	/// This means two things:
	/// - the program is invoked with `-c` followed by the command, and
	/// - the string will be split on space, and the resulting vec used as
	///   execvp(3) arguments: first is the shell program, rest are additional
	///   arguments (which come before the `-c` mentioned above). This is a very
	///   simplistic approach deliberately: it will not support quoted
	///   arguments, for example. Use [`Shell::None`] with a custom command vec
	///   if you want that.
	Unix(String),

	/// Use the Windows CMD.EXE shell.
	///
	/// This is invoked with `/C` followed by the command.
	#[cfg(windows)]
	Cmd,

	/// Use Powershell, on Windows or elsewhere.
	///
	/// This is invoked with `-Command` followed by the command.
	///
	/// This is preferred over `Unix("pwsh")`, though that will also work
	/// on unices due to Powershell supporting the `-c` short option.
	Powershell,
}

impl Default for Shell {
	#[cfg(windows)]
	fn default() -> Self {
		Self::Powershell
	}

	#[cfg(not(windows))]
	fn default() -> Self {
		Self::Unix("sh".into())
	}
}

impl Shell {
	/// Obtain a [`Command`] given a list of command parts.
	///
	/// Behaves as described in the enum documentation.
	///
	/// # Panics
	///
	/// - Panics if `cmd` is empty.
	/// - Panics if the string in the `Unix` variant is empty or only whitespace.
	pub fn to_command(&self, cmd: &[String]) -> Command {
		assert!(!cmd.is_empty(), "cmd was empty");

		match self {
			Shell::None => {
				// UNWRAP: checked by assert
				#[allow(clippy::unwrap_used)]
				let (first, rest) = cmd.split_first().unwrap();
				let mut c = Command::new(first);
				c.args(rest);
				c
			}

			#[cfg(windows)]
			Shell::Cmd => {
				let mut c = Command::new("cmd.exe");
				c.arg("/C").arg(cmd.join(" "));
				c
			}

			Shell::Powershell if cfg!(windows) => {
				let mut c = Command::new("powershell.exe");
				c.arg("-Command").arg(cmd.join(" "));
				c
			}

			Shell::Powershell => {
				let mut c = Command::new("pwsh");
				c.arg("-Command").arg(cmd.join(" "));
				c
			}

			Shell::Unix(name) => {
				assert!(!name.is_empty(), "shell program was empty");
				let sh = name.split_ascii_whitespace().collect::<Vec<_>>();

				// UNWRAP: checked by assert
				#[allow(clippy::unwrap_used)]
				let (shprog, shopts) = sh.split_first().unwrap();

				let mut c = Command::new(shprog);
				c.args(shopts);
				c.arg("-c").arg(cmd.join(" "));
				c
			}
		}
	}
}

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

#[cfg(test)]
mod test {
	use super::Shell;
	use command_group::AsyncCommandGroup;

	#[tokio::test]
	#[cfg(unix)]
	async fn unix_shell_default() -> Result<(), std::io::Error> {
		assert!(Shell::default()
			.to_command(&["echo".into(), "hi".into()])
			.group_status()
			.await?
			.success());
		Ok(())
	}

	#[tokio::test]
	#[cfg(unix)]
	async fn unix_shell_none() -> Result<(), std::io::Error> {
		assert!(Shell::None
			.to_command(&["echo".into(), "hi".into()])
			.group_status()
			.await?
			.success());
		Ok(())
	}

	#[tokio::test]
	#[cfg(unix)]
	async fn unix_shell_alternate() -> Result<(), std::io::Error> {
		assert!(Shell::Unix("bash".into())
			.to_command(&["echo".into(), "hi".into()])
			.group_status()
			.await?
			.success());
		Ok(())
	}

	#[tokio::test]
	#[cfg(unix)]
	async fn unix_shell_alternate_shopts() -> Result<(), std::io::Error> {
		assert!(Shell::Unix("bash -o errexit".into())
			.to_command(&["echo".into(), "hi".into()])
			.group_status()
			.await?
			.success());
		Ok(())
	}

	#[tokio::test]
	#[cfg(windows)]
	async fn windows_shell_default() -> Result<(), std::io::Error> {
		assert!(Shell::default()
			.to_command(&["echo".into(), "hi".into()])
			.group_status()
			.await?
			.success());
		Ok(())
	}

	#[tokio::test]
	#[cfg(windows)]
	async fn windows_shell_cmd() -> Result<(), std::io::Error> {
		assert!(Shell::Cmd
			.to_command(&["echo".into(), "hi".into()])
			.group_status()
			.await?
			.success());
		Ok(())
	}

	#[tokio::test]
	#[cfg(windows)]
	async fn windows_shell_powershell() -> Result<(), std::io::Error> {
		assert!(Shell::Powershell
			.to_command(&["echo".into(), "hi".into()])
			.group_status()
			.await?
			.success());
		Ok(())
	}

	#[tokio::test]
	#[cfg(windows)]
	async fn windows_shell_unix_style_powershell() -> Result<(), std::io::Error> {
		assert!(Shell::Unix("powershell.exe".into())
			.to_command(&["echo".into(), "hi".into()])
			.group_status()
			.await?
			.success());
		Ok(())
	}
}
