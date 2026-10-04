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
	/// [`process-wrap`](process_wrap) crate.
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

	/// Watch the process for terminal stops.
	///
	/// When enabled (Unix only), a watcher observes the running process and fires the job's stop
	/// hook (see [`Job::set_stop_hook`](crate::job::Job::set_stop_hook)) when the process is
	/// stopped by a terminal-generated signal: SIGTTIN or SIGTTOU, which the kernel delivers when
	/// the process attempts to read from or change the terminal while its process group is not the
	/// terminal's foreground process group, or SIGTSTP when it is suspended from the terminal.
	///
	/// This is a no-op when the process is not in its own process group (`grouped: false`) or in a
	/// session (`session: true`): in both cases it shares the parent's terminal foreground, or has
	/// no controlling terminal, and so can never be stopped this way.
	pub observe_stops: bool,

	/// Grant the process the terminal foreground when it needs it.
	///
	/// When enabled (Unix only, implies [`observe_stops`](Self::observe_stops)), if the process is
	/// stopped by SIGTTIN or SIGTTOU, the job gives the process group the foreground of the
	/// controlling terminal, exactly as a job control shell does, and continues it. The foreground
	/// is reclaimed, and the terminal state restored, when the process exits or is stopped again.
	///
	/// This makes programs which interact with the terminal — pagers, full-screen programs,
	/// password prompts — work under the process group wrap, without a pty. It requires the
	/// process group wrap (`grouped: true`, and not `session: true`): see [`observe_stops`]
	/// (Self::observe_stops) for why. If there is no controlling terminal, this is a no-op.
	pub grant_foreground: bool,
}
