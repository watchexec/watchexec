use std::path::PathBuf;

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
	},

	/// A shell program: a string which is to be executed by a shell.
	///
	/// (Tip: in general, a shell will handle its own job control, so there's no inherent need to
	/// set `grouped: true` at the [`Command`](super::Command) level.)
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
