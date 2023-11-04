//! Command construction and configuration.

use std::{borrow::Cow, collections::VecDeque, ffi::OsStr, fmt, path::PathBuf};

use tokio::process::Command as TokioCommand;
use tracing::trace;

/// A command to execute.
///
/// For simple uses, the `From` and `FromIterator` implementations may be useful:
///
/// ```
/// # use watchexec_supervisor::command::{Command, Program};
/// Command::from(Program::Exec {
///     prog: "ping".into(),
///     args: vec!["-c".into(), "4".into()],
/// });
/// ```
///
/// ```
/// # use watchexec_supervisor::command::{Command, Program, Shell};
/// Command::from_iter(vec![
///     Program::Exec {
///         prog: "nslookup".into(),
///         args: vec!["google.com".into()],
///     },
///     Program::Shell {
///         shell: Shell::new("bash"),
///         command: "curl -L google.com >/dev/null".into(),
///         args: Vec::new(),
///     },
/// ]);
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Command {
	/// Programs to execute as part of this command.
	///
	/// The [`Sequence`] type defines a sequential control flow for the programs, and can represent
	/// such flows as `a && b`, `a || b`, `a; b`, and more. However, pipelines are not supported;
	/// use a shell program for that.
	pub sequence: Sequence,

	/// Execution isolation method to use.
	pub isolation: Isolation,
}

impl From<Program> for Command {
	fn from(program: Program) -> Self {
		Self {
			sequence: Sequence::from(program),
			isolation: Isolation::default(),
		}
	}
}

impl FromIterator<Program> for Command {
	fn from_iter<I>(programs: I) -> Self
	where
		I: IntoIterator<Item = Program>,
	{
		Self {
			sequence: Sequence::from_iter(programs),
			isolation: Isolation::default(),
		}
	}
}

/// A sequence of programs, with some control flow.
///
/// This is effectively a hybrid tree of programs and subsequences, and can be thought of as the AST
/// that would result from parsing a command line.
///
/// # Examples
///
/// For simple uses, the `From` and `FromIterator` implementations may be useful:
///
/// ```
/// # use watchexec_supervisor::command::{Sequence, Program};
/// Sequence::from(Program::Exec {
///     prog: "ping".into(),
///     args: vec!["-c".into(), "4".into()],
/// });
/// ```
///
/// ```
/// # use watchexec_supervisor::command::{Sequence, Program, Shell};
/// Sequence::from_iter(vec![
///     Program::Exec {
///         prog: "nslookup".into(),
///         args: vec!["google.com".into()],
///     },
///     Program::Shell {
///         shell: Shell::new("bash"),
///         command: "curl -L google.com >/dev/null".into(),
///         args: Vec::new(),
///     },
/// ]);
/// ```
///
/// For more complex uses, the `and`, `or`, and `andor` methods may be useful:
///
/// ```
/// # use watchexec_supervisor::command::{Sequence, Program, Shell};
/// Sequence::Run(Program::Exec {
///     prog: "nslookup".into(),
///     args: vec!["google.com".into()],
/// }).and(Program::Shell {
///     shell: Shell::new("bash"),
///     command: "curl -L google.com >/dev/null".into(),
///     args: Vec::new(),
/// }.into());
/// // = `nslookup google.com && curl -L google.com >/dev/null`
/// ```
///
/// ```
/// # use watchexec_supervisor::command::{Sequence, Program, Shell};
/// Sequence::Run(Program::Shell {
///     shell: Shell::new("bash"),
///     command: "make test".into(),
///     args: Vec::new(),
/// }).or(Program::Shell {
///     shell: Shell::new("bash"),
///     command: "make fix".into(),
///     args: Vec::new(),
/// }.into());
/// // = `make test || make fix`
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Sequence {
	/// A single program to execute.
	Run(Program),

	/// A list of subsequences to execute sequentially.
	///
	/// If a sequence fails, the next sequence is started. If all sequences fail, this sequence fails.
	///
	/// It is equivalent to `a; b; c; ...`.
	List(VecDeque<Sequence>),

	/// A conditional control.
	///
	/// If the `given` sequence succeeds, the `then` sequence is executed, and its status bubbles up.
	/// If it fails, the `otherwise` sequence is executed, and its status bubbles up. If there is no
	/// sequence for the outcome of `given`, the status of `given` bubbles up.
	///
	/// This can represent `a && b` (with `otherwise` empty), `a || b` (with `then` empty), and
	/// `a && b || c` (both `then` and `otherwise` provided).
	///
	/// If neither `then` nor `otherwise` are provided, this is equivalent to the `given` sequence.
	Condition {
		given: Box<Sequence>,
		then: Option<Box<Sequence>>,
		otherwise: Option<Box<Sequence>>,
	}
}

impl Default for Sequence {
	fn default() -> Self {
		Self::List(VecDeque::new())
	}
}

impl Sequence {
	fn condition(given: Self, then: Option<Self>, otherwise: Option<Self>) -> Self {
		Self::Condition {
			given: Box::new(given),
			then: then.map(Box::new),
			otherwise: otherwise.map(Box::new),
		}
	}

	pub fn and(self, then: Self) -> Self {
		Self::condition(self, Some(then), None)
	}

	pub fn or(self, otherwise: Self) -> Self {
		Self::condition(self, None, Some(otherwise))
	}

	pub fn andor(self, then: Self, otherwise: Self) -> Self {
		Self::condition(self, Some(then), Some(otherwise))
	}
}

impl From<Program> for Sequence {
	fn from(program: Program) -> Self {
		Self::Run(program)
	}
}

impl FromIterator<Program> for Sequence {
	fn from_iter<I>(programs: I) -> Self
	where
		I: IntoIterator<Item = Program>,
	{
		Self::List(programs.into_iter().map(Self::Run).collect())
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
