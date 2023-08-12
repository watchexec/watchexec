//! Command construction and configuration.

use std::{borrow::Cow, collections::VecDeque, ffi::OsStr, fmt, path::PathBuf};

use tokio::process::Command as TokioCommand;
use tracing::trace;

#[doc(inline)]
pub use process::Process;

#[doc(inline)]
pub use supervisor::{Args, Supervisor, SupervisorId};

mod process;
mod supervisor;

#[cfg(test)]
mod tests;

/// A command to execute.
///
/// For simple uses, the From and FromIterator implementations may be useful:
///
/// ```
/// # use watchexec::command::{Command, Program};
/// Command::from(Program::Exec {
///        prog: "ping".into(),
///        args: vec!["-c".into(), "4".into()],
/// });
/// ```
///
/// ```
/// # use watchexec::command::{Command, Program, Shell};
/// Command::from_iter(vec![
///        Program::Exec {
///            prog: "nslookup".into(),
///            args: vec!["google.com".into()],
///        },
///        Program::Shell {
///            shell: Shell::new("bash"),
///            command: "curl -L google.com >/dev/null".into(),
///         args: Vec::new(),
///        },
///    ]);
///    ```
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Command {
	/// Programs to execute in sequence as part of this command.
	pub sequence: VecDeque<Program>,

	/// Execution isolation method to use.
	pub isolation: Isolation,

	/// If programs in the sequence fail, continue running subsequent ones.
	///
	/// In shell terms, this is the difference between
	///
	/// ```plain
	/// a && b && c
	/// ```
	///
	/// and
	///
	/// ```plain
	/// a; b; c
	/// ```
	///
	/// For more complex flow control, use a shell.
	pub continue_on_fail: bool,
}

impl From<Program> for Command {
	fn from(program: Program) -> Self {
		let mut command = Self::default();
		command.sequence.push_back(program);
		command
	}
}

impl FromIterator<Program> for Command {
	fn from_iter<I>(programs: I) -> Self
	where
		I: IntoIterator<Item = Program>,
	{
		let mut command = Self::default();
		command.sequence.extend(programs);
		command
	}
}

/// A single program call.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Program {
	/// A raw program call: the path or name of a program and its argument list.
	Exec {
		/// Path or name of the program.
		prog: PathBuf,

		/// The arguments to pass.
		args: Vec<String>,
	},

	/// A shell program: a string which is to be executed by a shell.
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

/// How to call the shell used to run shelled programs.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Shell {
	/// Path or name of the shell.
	pub prog: PathBuf,

	/// Additional options or arguments to pass to the shell.
	///
	/// These will be inserted before the `program_option` immediately preceding the program string.
	pub options: Vec<String>,

	/// The syntax of the option which precedes the program string.
	///
	/// For most shells, this is `-c`. On Windows, CMD.EXE prefers `/C`. If this is `None`, then no
	/// option is prepended; this may be useful for non-shell or non-standard shell programs.
	pub program_option: Option<Cow<'static, OsStr>>,
}

impl Shell {
	/// Shorthand for most shells, using the `-c` convention.
	pub fn new(name: impl Into<PathBuf>) -> Self {
		Self {
			prog: name.into(),
			options: Vec::new(),
			program_option: Some(Cow::Borrowed(OsStr::new("-c"))),
		}
	}

	#[cfg(windows)]
	/// Shorthand for the CMD.EXE shell.
	pub fn cmd() -> Self {
		Self {
			prog: "CMD.EXE".into(),
			options: Vec::new(),
			program_option: Some(Cow::Borrowed(OsStr::new("/C"))),
		}
	}
}

/// The execution isolation method to use.
///
/// Note that some values are only available on some platforms.
///
/// This is marked non-exhaustive to allow more methods to be added later.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Isolation {
	/// No isolation (default).
	#[default]
	None,

	/// Command groups.
	///
	/// This will use either of Unix [process groups] or Windows [Job Objects] via the `command-group`
	/// crate.
	///
	/// [process group]: https://en.wikipedia.org/wiki/Process_group
	/// [Job Objects]: https://en.wikipedia.org/wiki/Object_Manager_(Windows)
	Grouped,
}

// to be implemented:
// Pty, (unix only)
// CGroups, (linux only)
// Container, (linux and windows only, when daemon available)

impl Program {
	/// Obtain a [`tokio::process::Command`] from a [`Command`].
	///
	/// Behaves as described in the [`Command`] and [`Shell`] documentation.
	pub fn to_spawnable(&self) -> TokioCommand {
		trace!(prog=?self, "constructing command");

		match self {
			Self::Exec { prog, args } => {
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
}

impl fmt::Display for Program {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Exec { prog, args } => {
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
