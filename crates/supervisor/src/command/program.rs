use std::{fmt, path::PathBuf};

use tokio::process::Command as TokioCommand;
use tracing::trace;

use super::Shell;

/// A single program call.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Program {
	/// A raw program call: the path or name of a program and its argument list.
	Exec {
		/// Path or name of the program.
		prog: PathBuf,

		/// The arguments to pass.
		args: Vec<String>,

		/// Run the program in a new process group.
		///
		/// This will use either of Unix [process groups] or Windows [Job Objects] via the
		/// [`command-group`](command_group) crate.
		///
		/// [process group]: https://en.wikipedia.org/wiki/Process_group
		/// [Job Objects]: https://en.wikipedia.org/wiki/Object_Manager_(Windows)
		grouped: bool,
	},

	/// A shell program: a string which is to be executed by a shell.
	///
	/// We assume that the shell is handling job control, and so there is no option to create a new
	/// process group as with [`Program::Exec`].
	Shell {
		/// The shell to run.
		shell: Shell,

		/// The command line to pass to the shell.
		command: String,

		/// The arguments to pass to the shell invocation.
		///
		/// This may not be supported by all shells. Note that some shells require the use of `--`
		/// for disambiguation: this is not handled by Watchexec, and will need to be the first
		/// item in this vec if desired.
		///
		/// This appends the values within to the shell process invocation.
		args: Vec<String>,
	},
}

impl Program {
	/// Obtain a [`tokio::process::Command`] from a [`Command`].
	///
	/// Behaves as described in the [`Command`] and [`Shell`] documentation.
	pub fn to_spawnable(&self) -> TokioCommand {
		trace!(prog=?self, "constructing command");

		match self {
			Self::Exec { prog, args, .. } => {
				let mut c = TokioCommand::new(prog);
				c.args(args);
				c
			}

			Self::Shell {
				shell,
				args,
				command,
			} => {
				let mut c = TokioCommand::new(shell.prog.clone());

				// Avoid quoting issues on Windows by using raw_arg everywhere
				#[cfg(windows)]
				{
					for opt in &shell.options {
						c.raw_arg(opt);
					}
					if let Some(progopt) = &shell.program_option {
						c.raw_arg(progopt);
					}
					c.raw_arg(command);
					for arg in args {
						c.raw_arg(arg);
					}
				}

				#[cfg(not(windows))]
				{
					c.args(shell.options.clone());
					if let Some(progopt) = &shell.program_option {
						c.arg(progopt);
					}
					c.arg(command);
					for arg in args {
						c.arg(arg);
					}
				}

				c
			}
		}
	}

	/// Internal method to create a (non-sensical) empty program, for mem::replace.
	pub(crate) fn empty() -> Self {
		Self::Exec {
			prog: PathBuf::new(),
			args: Vec::new(),
			grouped: false,
		}
	}
}

impl fmt::Display for Program {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Exec { prog, args, .. } => {
				write!(f, "{}", prog.display())?;
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
