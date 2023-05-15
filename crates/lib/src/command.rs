//! Command construction, configuration, and tracking.

use std::fmt;

use tokio::process::Command as TokioCommand;
use tracing::trace;

use crate::error::RuntimeError;

#[doc(inline)]
pub use process::Process;

#[doc(inline)]
pub use supervisor::{Supervisor, SupervisorBuilder};

mod process;
mod supervisor;

#[cfg(test)]
mod tests;

/// A command to execute.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Command {
	/// A raw command which will be executed as-is.
	Exec {
		/// The program to run.
		prog: String,

		/// The arguments to pass.
		args: Vec<String>,
	},

	/// A shelled command line.
	Shell {
		/// The shell to run.
		shell: Shell,

		/// Additional options or arguments to pass to the shell.
		///
		/// These will be inserted before the `-c` (or equivalent) option immediately preceding the
		/// command line string.
		args: Vec<String>,

		/// The command line to pass to the shell.
		command: String,
	},
}

/// Shell to use to run shelled commands.
///
/// `Cmd` and `Powershell` are special-cased because they have different calling conventions. Also
/// `Cmd` is only available in Windows, while `Powershell` is also available on unices (provided the
/// end-user has it installed, of course).
///
/// There is no default implemented: as consumer of this library you are encouraged to set your own
/// default as makes sense in your application / for your platform.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Shell {
	/// Use the given string as a unix shell invocation.
	///
	/// This is invoked with `-c` followed by the command.
	Unix(String),

	/// Use the Windows CMD.EXE shell.
	///
	/// This is `cmd.exe` invoked with `/C` followed by the command.
	#[cfg(windows)]
	Cmd,

	/// Use Powershell, on Windows or elsewhere.
	///
	/// This is `powershell.exe` invoked with `-Command` followed by the command on Windows.
	/// On unices, it is equivalent to `Unix("pwsh")`.
	Powershell,
}

impl Command {
	/// Obtain a [`tokio::process::Command`] from a [`Command`].
	///
	/// Behaves as described in the [`Command`] and [`Shell`] documentation.
	///
	/// # Errors
	///
	/// - Errors if the `command` of a `Command::Shell` is empty.
	/// - Errors if the `shell` of a `Shell::Unix(shell)` is empty.
	pub fn to_spawnable(&self) -> Result<TokioCommand, RuntimeError> {
		trace!(cmd=?self, "constructing command");

		match self {
			Self::Exec { prog, args } => {
				let mut c = TokioCommand::new(prog);
				c.args(args);
				Ok(c)
			}

			Self::Shell {
				shell,
				args,
				command,
			} => {
				let (shcmd, shcliopt) = match shell {
					#[cfg(windows)]
					Shell::Cmd => ("cmd.exe", "/C"),

					#[cfg(windows)]
					Shell::Powershell => ("powershell.exe", "-Command"),
					#[cfg(not(windows))]
					Shell::Powershell => ("pwsh", "-c"),

					Shell::Unix(cmd) => {
						if cmd.is_empty() {
							return Err(RuntimeError::CommandShellEmptyShell);
						}

						(cmd.as_str(), "-c")
					}
				};

				if command.is_empty() {
					return Err(RuntimeError::CommandShellEmptyCommand);
				}

				let mut c = TokioCommand::new(shcmd);
				c.args(args);
				c.arg(shcliopt).arg(command);
				Ok(c)
			}
		}
	}
}

impl fmt::Display for Command {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Exec { prog, args } => {
				write!(f, "{prog}")?;
				for arg in args {
					write!(f, " {arg}")?;
				}

				Ok(())
			}
			Self::Shell { command, .. } => {
				write!(f, "{command}")
			}
		}
	}
}
