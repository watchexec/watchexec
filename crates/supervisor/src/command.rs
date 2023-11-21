//! Command construction and configuration.

#[doc(inline)]
pub use self::{program::Program, shell::Shell};

mod conversions;
mod program;
mod shell;

/// A command to execute.
///
/// # Examples
///
/// ```
/// # use watchexec_supervisor::command::{Command, Program};
/// Command {
///     program: Program::Exec {
///         prog: "make".into(),
///         args: vec!["check".into()],
///     },
///     grouped: true,
/// };
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Command {
	/// Program to execute for this command.
	pub program: Program,

	/// Run the program in a new process group.
	///
	/// This will use either of Unix [process groups] or Windows [Job Objects] via the
	/// [`command-group`](command_group) crate.
	///
	/// [process groups]: https://en.wikipedia.org/wiki/Process_group
	/// [Job Objects]: https://en.wikipedia.org/wiki/Object_Manager_(Windows)
	pub grouped: bool,
}
