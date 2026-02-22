use std::{ffi::OsStr, path::PathBuf};

use clap::{
	builder::TypedValueParser, error::ErrorKind, Arg, Command, CommandFactory, Parser, ValueEnum,
};
use miette::Result;

use tracing::warn;
use watchexec_signals::Signal;

use super::{command::CommandArgs, filtering::FilteringArgs, TimeSpan, OPTSET_EVENTS};

#[derive(Debug, Clone, Parser)]
pub struct EventsArgs {
	/// What to do when receiving events while the command is running
	///
	/// Default is to 'do-nothing', which ignores events while the command is running, so that
	/// changes that occur due to the command are ignored, like compilation outputs. You can also
	/// use 'queue' which will run the command once again when the current run has finished if any
	/// events occur while it's running, or 'restart', which terminates the running command and starts
	/// a new one. Finally, there's 'signal', which only sends a signal; this can be useful with
	/// programs that can reload their configuration without a full restart.
	///
	/// The signal can be specified with the '--signal' option.
	#[arg(
		short,
		long,
		help_heading = OPTSET_EVENTS,
		default_value = "do-nothing",
		hide_default_value = true,
		value_name = "MODE",
		display_order = 150,
	)]
	pub on_busy_update: OnBusyUpdate,

	/// Restart the process if it's still running
	///
	/// This is a shorthand for '--on-busy-update=restart'.
	#[arg(
		short,
		long,
		help_heading = OPTSET_EVENTS,
		conflicts_with_all = ["on_busy_update"],
		display_order = 180,
	)]
	pub restart: bool,

	/// Send a signal to the process when it's still running
	///
	/// Specify a signal to send to the process when it's still running. This implies
	/// '--on-busy-update=signal'; otherwise the signal used when that mode is 'restart' is
	/// controlled by '--stop-signal'.
	///
	/// See the long documentation for '--stop-signal' for syntax.
	///
	/// Signals are not supported on Windows at the moment, and will always be overridden to 'kill'.
	/// See '--stop-signal' for more on Windows "signals".
	#[arg(
		short,
		long,
		help_heading = OPTSET_EVENTS,
		conflicts_with_all = ["restart"],
		value_name = "SIGNAL",
		display_order = 190,
	)]
	pub signal: Option<Signal>,

	/// Translate signals from the OS to signals to send to the command
	///
	/// Takes a pair of signal names, separated by a colon, such as "TERM:INT" to map SIGTERM to
	/// SIGINT. The first signal is the one received by watchexec, and the second is the one sent to
	/// the command. The second can be omitted to discard the first signal, such as "TERM:" to
	/// not do anything on SIGTERM.
	///
	/// If SIGINT or SIGTERM are mapped, then they no longer quit Watchexec. Besides making it hard
	/// to quit Watchexec itself, this is useful to send pass a Ctrl-C to the command without also
	/// terminating Watchexec and the underlying program with it, e.g. with "INT:INT".
	///
	/// This option can be specified multiple times to map multiple signals.
	///
	/// Signal syntax is case-insensitive for short names (like "TERM", "USR2") and long names (like
	/// "SIGKILL", "SIGHUP"). Signal numbers are also supported (like "15", "31"). On Windows, the
	/// forms "STOP", "CTRL+C", and "CTRL+BREAK" are also supported to receive, but Watchexec cannot
	/// yet deliver other "signals" than a STOP.
	#[arg(
		long = "map-signal",
		help_heading = OPTSET_EVENTS,
		value_name = "SIGNAL:SIGNAL",
		value_parser = SignalMappingValueParser,
		display_order = 130,
	)]
	pub signal_map: Vec<SignalMapping>,

	/// Time to wait for new events before taking action
	///
	/// When an event is received, Watchexec will wait for up to this amount of time before handling
	/// it (such as running the command). This is essential as what you might perceive as a single
	/// change may actually emit many events, and without this behaviour, Watchexec would run much
	/// too often. Additionally, it's not infrequent that file writes are not atomic, and each write
	/// may emit an event, so this is a good way to avoid running a command while a file is
	/// partially written.
	///
	/// An alternative use is to set a high value (like "30min" or longer), to save power or
	/// bandwidth on intensive tasks, like an ad-hoc backup script. In those use cases, note that
	/// every accumulated event will build up in memory.
	///
	/// Takes a unit-less value in milliseconds, or a time span value such as "5sec 20ms".
	/// Providing a unit-less value is deprecated and will warn; it will be an error in the future.
	///
	/// The default is 50 milliseconds. Setting to 0 is highly discouraged.
	#[arg(
		long,
		short,
		help_heading = OPTSET_EVENTS,
		default_value = "50ms",
		hide_default_value = true,
		value_name = "TIMEOUT",
		display_order = 40,
	)]
	pub debounce: TimeSpan<1_000_000>,

	/// Exit when stdin closes
	///
	/// This watches the stdin file descriptor for EOF, and exits Watchexec gracefully when it is
	/// closed. This is used by some process managers to avoid leaving zombie processes around.
	#[arg(
		long,
		help_heading = OPTSET_EVENTS,
		display_order = 191,
	)]
	pub stdin_quit: bool,

	/// Enable interactive mode
	///
	/// In interactive mode, Watchexec listens for keypresses and responds to them. Currently
	/// supported keys are: 'r' to restart the command, 'p' to toggle pausing the watch, and 'q'
	/// to quit. This requires a terminal (TTY) and puts stdin into raw mode, so the child process
	/// will not receive stdin input.
	#[arg(
		long,
		short = 'H',
		help_heading = OPTSET_EVENTS,
		display_order = 192,
	)]
	pub interactive: bool,

	/// Wait until first change before running command
	///
	/// By default, Watchexec will run the command once immediately. With this option, it will
	/// instead wait until an event is detected before running the command as normal.
	#[arg(
		long,
		short,
		help_heading = OPTSET_EVENTS,
		display_order = 161,
	)]
	pub postpone: bool,

	/// Poll for filesystem changes
	///
	/// By default, and where available, Watchexec uses the operating system's native file system
	/// watching capabilities. This option disables that and instead uses a polling mechanism, which
	/// is less efficient but can work around issues with some file systems (like network shares) or
	/// edge cases.
	///
	/// Optionally takes a unit-less value in milliseconds, or a time span value such as "2s 500ms",
	/// to use as the polling interval. If not specified, the default is 30 seconds.
	/// Providing a unit-less value is deprecated and will warn; it will be an error in the future.
	///
	/// Aliased as '--force-poll'.
	#[arg(
		long,
		help_heading = OPTSET_EVENTS,
		alias = "force-poll",
		num_args = 0..=1,
		default_missing_value = "30s",
		value_name = "INTERVAL",
		display_order = 160,
	)]
	pub poll: Option<TimeSpan<1_000_000>>,

	/// Configure event emission
	///
	/// Watchexec can emit event information when running a command, which can be used by the child
	/// process to target specific changed files.
	///
	/// One thing to take care with is assuming inherent behaviour where there is only chance.
	/// Notably, it could appear as if the `RENAMED` variable contains both the original and the new
	/// path being renamed. In previous versions, it would even appear on some platforms as if the
	/// original always came before the new. However, none of this was true. It's impossible to
	/// reliably and portably know which changed path is the old or new, "half" renames may appear
	/// (only the original, only the new), "unknown" renames may appear (change was a rename, but
	/// whether it was the old or new isn't known), rename events might split across two debouncing
	/// boundaries, and so on.
	///
	/// This option controls where that information is emitted. It defaults to 'none', which doesn't
	/// emit event information at all. The other options are 'environment' (deprecated), 'stdio',
	/// 'file', 'json-stdio', and 'json-file'.
	///
	/// The 'stdio' and 'file' modes are text-based: 'stdio' writes absolute paths to the stdin of
	/// the command, one per line, each prefixed with `create:`, `remove:`, `rename:`, `modify:`,
	/// or `other:`, then closes the handle; 'file' writes the same thing to a temporary file, and
	/// its path is given with the $WATCHEXEC_EVENTS_FILE environment variable.
	///
	/// There are also two JSON modes, which are based on JSON objects and can represent the full
	/// set of events Watchexec handles. Here's an example of a folder being created on Linux:
	///
	/// ```json
	///   {
	///     "tags": [
	///       {
	///         "kind": "path",
	///         "absolute": "/home/user/your/new-folder",
	///         "filetype": "dir"
	///       },
	///       {
	///         "kind": "fs",
	///         "simple": "create",
	///         "full": "Create(Folder)"
	///       },
	///       {
	///         "kind": "source",
	///         "source": "filesystem",
	///       }
	///     ],
	///     "metadata": {
	///       "notify-backend": "inotify"
	///     }
	///   }
	/// ```
	///
	/// The fields are as follows:
	///
	///   - `tags`, structured event data.
	///   - `tags[].kind`, which can be:
	///     * 'path', along with:
	///       + `absolute`, an absolute path.
	///       + `filetype`, a file type if known ('dir', 'file', 'symlink', 'other').
	///     * 'fs':
	///       + `simple`, the "simple" event type ('access', 'create', 'modify', 'remove', or 'other').
	///       + `full`, the "full" event type, which is too complex to fully describe here, but looks like 'General(Precise(Specific))'.
	///     * 'source', along with:
	///       + `source`, the source of the event ('filesystem', 'keyboard', 'mouse', 'os', 'time', 'internal').
	///     * 'keyboard', along with:
	///       + `keycode`. Currently only the value 'eof' is supported.
	///     * 'process', for events caused by processes:
	///       + `pid`, the process ID.
	///     * 'signal', for signals sent to Watchexec:
	///       + `signal`, the normalised signal name ('hangup', 'interrupt', 'quit', 'terminate', 'user1', 'user2').
	///     * 'completion', for when a command ends:
	///       + `disposition`, the exit disposition ('success', 'error', 'signal', 'stop', 'exception', 'continued').
	///       + `code`, the exit, signal, stop, or exception code.
	///   - `metadata`, additional information about the event.
	///
	/// The 'json-stdio' mode will emit JSON events to the standard input of the command, one per
	/// line, then close stdin. The 'json-file' mode will create a temporary file, write the
	/// events to it, and provide the path to the file with the $WATCHEXEC_EVENTS_FILE
	/// environment variable.
	///
	/// Finally, the 'environment' mode was the default until 2.0. It sets environment variables
	/// with the paths of the affected files, for filesystem events:
	///
	/// $WATCHEXEC_COMMON_PATH is set to the longest common path of all of the below variables,
	/// and so should be prepended to each path to obtain the full/real path. Then:
	///
	///   - $WATCHEXEC_CREATED_PATH is set when files/folders were created
	///   - $WATCHEXEC_REMOVED_PATH is set when files/folders were removed
	///   - $WATCHEXEC_RENAMED_PATH is set when files/folders were renamed
	///   - $WATCHEXEC_WRITTEN_PATH is set when files/folders were modified
	///   - $WATCHEXEC_META_CHANGED_PATH is set when files/folders' metadata were modified
	///   - $WATCHEXEC_OTHERWISE_CHANGED_PATH is set for every other kind of pathed event
	///
	/// Multiple paths are separated by the system path separator, ';' on Windows and ':' on unix.
	/// Within each variable, paths are deduplicated and sorted in binary order (i.e. neither
	/// Unicode nor locale aware).
	///
	/// This is the legacy mode, is deprecated, and will be removed in the future. The environment
	/// is a very restricted space, while also limited in what it can usefully represent. Large
	/// numbers of files will either cause the environment to be truncated, or may error or crash
	/// the process entirely. The $WATCHEXEC_COMMON_PATH is also unintuitive, as demonstrated by the
	/// multiple confused queries that have landed in my inbox over the years.
	#[arg(
		long,
		help_heading = OPTSET_EVENTS,
		verbatim_doc_comment,
		default_value = "none",
		hide_default_value = true,
		value_name = "MODE",
		display_order = 50,
	)]
	pub emit_events_to: EmitEvents,
}

impl EventsArgs {
	pub(crate) fn normalise(
		&mut self,
		command: &CommandArgs,
		filtering: &FilteringArgs,
		only_emit_events: bool,
	) -> Result<()> {
		if self.signal.is_some() {
			self.on_busy_update = OnBusyUpdate::Signal;
		} else if self.restart {
			self.on_busy_update = OnBusyUpdate::Restart;
		}

		if command.no_environment {
			warn!("--no-environment is deprecated");
			self.emit_events_to = EmitEvents::None;
		}

		if only_emit_events
			&& !matches!(
				self.emit_events_to,
				EmitEvents::JsonStdio | EmitEvents::Stdio
			) {
			self.emit_events_to = EmitEvents::JsonStdio;
		}

		if self.stdin_quit && filtering.watch_file == Some(PathBuf::from("-")) {
			super::Args::command()
				.error(
					ErrorKind::InvalidValue,
					"stdin-quit cannot be used when --watch-file=-",
				)
				.exit();
		}

		if self.interactive && filtering.watch_file == Some(PathBuf::from("-")) {
			super::Args::command()
				.error(
					ErrorKind::InvalidValue,
					"interactive mode cannot be used when --watch-file=-",
				)
				.exit();
		}

		Ok(())
	}
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum EmitEvents {
	#[default]
	Environment,
	Stdio,
	File,
	JsonStdio,
	JsonFile,
	None,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum OnBusyUpdate {
	#[default]
	Queue,
	DoNothing,
	Restart,
	Signal,
}

#[derive(Clone, Copy, Debug)]
pub struct SignalMapping {
	pub from: Signal,
	pub to: Option<Signal>,
}

#[derive(Clone)]
struct SignalMappingValueParser;

impl TypedValueParser for SignalMappingValueParser {
	type Value = SignalMapping;

	fn parse_ref(
		&self,
		_cmd: &Command,
		_arg: Option<&Arg>,
		value: &OsStr,
	) -> Result<Self::Value, clap::error::Error> {
		let value = value
			.to_str()
			.ok_or_else(|| clap::error::Error::raw(ErrorKind::ValueValidation, "invalid UTF-8"))?;
		let (from, to) = value
			.split_once(':')
			.ok_or_else(|| clap::error::Error::raw(ErrorKind::ValueValidation, "missing ':'"))?;

		let from = from
			.parse::<Signal>()
			.map_err(|sigparse| clap::error::Error::raw(ErrorKind::ValueValidation, sigparse))?;
		let to = if to.is_empty() {
			None
		} else {
			Some(to.parse::<Signal>().map_err(|sigparse| {
				clap::error::Error::raw(ErrorKind::ValueValidation, sigparse)
			})?)
		};

		Ok(Self::Value { from, to })
	}
}
