use std::{mem::take, path::PathBuf};

use clap::{Parser, ValueEnum, ValueHint};
use miette::{IntoDiagnostic, Result};
use tracing::{info, warn};
use watchexec_signals::Signal;

use crate::fd_socket::{FdSpec, FdSpecValueParser};

use super::{TimeSpan, OPTSET_COMMAND};

#[derive(Debug, Clone, Parser)]
pub struct CommandArgs {
	/// Use a different shell
	///
	/// By default, Watchexec will use '$SHELL' if it's defined or a default of 'sh' on Unix-likes,
	/// and either 'pwsh', 'powershell', or 'cmd' (CMD.EXE) on Windows, depending on what Watchexec
	/// detects is the running shell.
	///
	/// With this option, you can override that and use a different shell, for example one with more
	/// features or one which has your custom aliases and functions.
	///
	/// If the value has spaces, it is parsed as a command line, and the first word used as the
	/// shell program, with the rest as arguments to the shell.
	///
	/// The command is run with the '-c' flag (except for 'cmd' on Windows, where it's '/C').
	///
	/// The special value 'none' can be used to disable shell use entirely. In that case, the
	/// command provided to Watchexec will be parsed, with the first word being the executable and
	/// the rest being the arguments, and executed directly. Note that this parsing is rudimentary,
	/// and may not work as expected in all cases.
	///
	/// Using 'none' is a little more efficient and can enable a stricter interpretation of the
	/// input, but it also means that you can't use shell features like globbing, redirection,
	/// control flow, logic, or pipes.
	///
	/// Examples:
	///
	/// Use without shell:
	///
	///   $ watchexec -n -- zsh -x -o shwordsplit scr
	///
	/// Use with powershell core:
	///
	///   $ watchexec --shell=pwsh -- Test-Connection localhost
	///
	/// Use with CMD.exe:
	///
	///   $ watchexec --shell=cmd -- dir
	///
	/// Use with a different unix shell:
	///
	///   $ watchexec --shell=bash -- 'echo $BASH_VERSION'
	///
	/// Use with a unix shell and options:
	///
	///   $ watchexec --shell='zsh -x -o shwordsplit' -- scr
	#[arg(
		long,
		help_heading = OPTSET_COMMAND,
		value_name = "SHELL",
		display_order = 190,
	)]
	pub shell: Option<String>,

	/// Shorthand for '--shell=none'
	#[arg(
		short = 'n',
		help_heading = OPTSET_COMMAND,
		display_order = 140,
	)]
	pub no_shell: bool,

	/// Deprecated shorthand for '--emit-events=none'
	///
	/// This is the old way to disable event emission into the environment. See '--emit-events' for
	/// more. Will be removed at next major release.
	#[arg(
		long,
		help_heading = OPTSET_COMMAND,
		hide = true, // deprecated
	)]
	pub no_environment: bool,

	/// Add env vars to the command
	///
	/// This is a convenience option for setting environment variables for the command, without
	/// setting them for the Watchexec process itself.
	///
	/// Use key=value syntax. Multiple variables can be set by repeating the option.
	#[arg(
		long,
		short = 'E',
		help_heading = OPTSET_COMMAND,
		value_name = "KEY=VALUE",
		display_order = 50,
	)]
	pub env: Vec<String>,

	/// Don't use a process group
	///
	/// By default, Watchexec will run the command in a process group, so that signals and
	/// terminations are sent to all processes in the group. Sometimes that's not what you want, and
	/// you can disable the behaviour with this option.
	///
	/// Deprecated, use '--wrap-process=none' instead.
	#[arg(
		long,
		help_heading = OPTSET_COMMAND,
		display_order = 141,
	)]
	pub no_process_group: bool,

	/// Configure how the process is wrapped
	///
	/// By default, Watchexec will run the command in a process group in Unix, and in a Job Object
	/// in Windows.
	///
	/// Some Unix programs prefer running in a session, while others do not work in a process group.
	///
	/// Use 'group' to use a process group, 'session' to use a process session, and 'none' to run
	/// the command directly. On Windows, either of 'group' or 'session' will use a Job Object.
	#[arg(
		long,
		help_heading = OPTSET_COMMAND,
		value_name = "MODE",
		default_value = "group",
		display_order = 231,
	)]
	pub wrap_process: WrapMode,

	/// Signal to send to stop the command
	///
	/// This is used by 'restart' and 'signal' modes of '--on-busy-update' (unless '--signal' is
	/// provided). The restart behaviour is to send the signal, wait for the command to exit, and if
	/// it hasn't exited after some time (see '--timeout-stop'), forcefully terminate it.
	///
	/// The default on unix is "SIGTERM".
	///
	/// Input is parsed as a full signal name (like "SIGTERM"), a short signal name (like "TERM"),
	/// or a signal number (like "15"). All input is case-insensitive.
	///
	/// On Windows this option is technically supported but only supports the "KILL" event, as
	/// Watchexec cannot yet deliver other events. Windows doesn't have signals as such; instead it
	/// has termination (here called "KILL" or "STOP") and "CTRL+C", "CTRL+BREAK", and "CTRL+CLOSE"
	/// events. For portability the unix signals "SIGKILL", "SIGINT", "SIGTERM", and "SIGHUP" are
	/// respectively mapped to these.
	#[arg(
		long,
		help_heading = OPTSET_COMMAND,
		value_name = "SIGNAL",
		display_order = 191,
	)]
	pub stop_signal: Option<Signal>,

	/// Time to wait for the command to exit gracefully
	///
	/// This is used by the 'restart' mode of '--on-busy-update'. After the graceful stop signal
	/// is sent, Watchexec will wait for the command to exit. If it hasn't exited after this time,
	/// it is forcefully terminated.
	///
	/// Takes a unit-less value in seconds, or a time span value such as "5min 20s".
	/// Providing a unit-less value is deprecated and will warn; it will be an error in the future.
	///
	/// The default is 10 seconds. Set to 0 to immediately force-kill the command.
	///
	/// This has no practical effect on Windows as the command is always forcefully terminated; see
	/// '--stop-signal' for why.
	#[arg(
		long,
		help_heading = OPTSET_COMMAND,
		default_value = "10s",
		hide_default_value = true,
		value_name = "TIMEOUT",
		display_order = 192,
	)]
	pub stop_timeout: TimeSpan,

	/// Sleep before running the command
	///
	/// This option will cause Watchexec to sleep for the specified amount of time before running
	/// the command, after an event is detected. This is like using "sleep 5 && command" in a shell,
	/// but portable and slightly more efficient.
	///
	/// Takes a unit-less value in seconds, or a time span value such as "2min 5s".
	/// Providing a unit-less value is deprecated and will warn; it will be an error in the future.
	#[arg(
		long,
		help_heading = OPTSET_COMMAND,
		value_name = "DURATION",
		display_order = 40,
	)]
	pub delay_run: Option<TimeSpan>,

	/// Set the working directory
	///
	/// By default, the working directory of the command is the working directory of Watchexec. You
	/// can change that with this option. Note that paths may be less intuitive to use with this.
	#[arg(
		long,
		help_heading = OPTSET_COMMAND,
		value_hint = ValueHint::DirPath,
		value_name = "DIRECTORY",
		display_order = 230,
	)]
	pub workdir: Option<PathBuf>,

	/// Create listen-fd sockets
	///
	/// This implements the systemd socket-passing protocol, like with `systemfd`: sockets are
	/// opened from the watchexec process, and then passed to the commands it runs. This lets you
	/// keep sockets open and avoid address reuse issues or dropping packets.
	///
	/// This option can be supplied multiple times, to open multiple sockets.
	///
	/// The value can be either of `PORT` (opens a TCP listening socket at that port), `HOST:PORT`
	/// (specify a host IP address; IPv6 addresses can be specified `[bracketed]`), `TYPE::PORT` or
	/// `TYPE::HOST:PORT` (specify a socket type, `tcp` / `udp`).
	///
	/// This integration only provides basic support, if you want more control you should use the
	/// `systemfd` tool from <https://github.com/mitsuhiko/systemfd>, upon which this is based. The
	/// syntax here and the spawning behaviour is identical to `systemfd`, and both watchexec and
	/// systemfd are compatible implementations of the systemd socket-activation protocol.
	#[arg(
		long,
		help_heading = OPTSET_COMMAND,
		value_name = "PORT",
		value_parser = FdSpecValueParser,
		display_order = 60,
	)]
	pub fd_socket: Vec<FdSpec>,
}

impl CommandArgs {
	pub(crate) fn normalise(&mut self) -> Result<()> {
		if self.no_process_group {
			warn!("--no-process-group is deprecated");
			self.wrap_process = WrapMode::None;
		}

		let workdir = if let Some(w) = take(&mut self.workdir) {
			w
		} else {
			let curdir = std::env::current_dir().into_diagnostic()?;
			dunce::canonicalize(curdir).into_diagnostic()?
		};
		info!(path=?workdir, "effective working directory");
		self.workdir = Some(workdir);

		debug_assert!(self.workdir.is_some());
		Ok(())
	}
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum WrapMode {
	#[default]
	Group,
	Session,
	None,
}
