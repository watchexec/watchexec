//! Command construction and configuration.

#[doc(inline)]
pub use self::{program::Program, shell::Shell};

mod conversions;
mod program;
mod shell;

/// A command to execute.
///
/// # Example
///
/// ```
/// # use watchexec_supervisor::command::{Command, Program};
/// Command {
///     program: Program::Exec {
///         prog: "make".into(),
///         args: vec!["check".into()],
///     },
///     options: Default::default(),
/// };
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Command {
	/// Program to execute for this command.
	pub program: Program,

	/// Options for spawning the program.
	pub options: SpawnOptions,
}

/// Options set when constructing or spawning a command.
///
/// It's recommended to use the [`Default`] implementation for this struct, and only set the options
/// you need to change, to proof against new options being added in future.
///
/// # Examples
///
/// ```
/// # use watchexec_supervisor::command::{Command, Program, SpawnOptions};
/// Command {
///     program: Program::Exec {
///         prog: "make".into(),
///         args: vec!["check".into()],
///     },
///     options: SpawnOptions {
///         grouped: true,
///         ..Default::default()
///     },
/// };
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SpawnOptions {
	/// Run the program in a new process group.
	///
	/// This will use either of Unix [process groups] or Windows [Job Objects] via the
	/// [`command-group`](command_group) crate.
	///
	/// [process groups]: https://en.wikipedia.org/wiki/Process_group
	/// [Job Objects]: https://en.wikipedia.org/wiki/Object_Manager_(Windows)
	pub grouped: bool,

	/// Run the program in a new session.
	///
	/// This will use Unix [sessions]. On Windows, this is not supported. This
	/// implies `grouped: true`.
	///
	/// [sessions]: https://pubs.opengroup.org/onlinepubs/9699919799/functions/setsid.html
	pub session: bool,

	/// Reset the signal mask of the process before we spawn it.
	///
	/// By default, the signal mask of the process is inherited from the parent process. This means
	/// that if the parent process has blocked any signals, the child process will also block those
	/// signals. This can cause problems if the child process is expecting to receive those signals.
	///
	/// This is only supported on Unix systems.
	pub reset_sigmask: bool,
}
