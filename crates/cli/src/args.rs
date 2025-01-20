use std::{
	ffi::{OsStr, OsString},
	mem::take,
	path::PathBuf,
	str::FromStr,
	time::Duration,
};

use clap::{
	builder::TypedValueParser, error::ErrorKind, Arg, Command, CommandFactory, Parser, ValueEnum,
	ValueHint,
};
use miette::{IntoDiagnostic, Result};
use tracing::{debug, info, warn};
use tracing_appender::non_blocking::WorkerGuard;
use watchexec_signals::Signal;

pub(crate) mod command;
pub(crate) mod filtering;
pub(crate) mod logging;

const OPTSET_COMMAND: &str = "Command";
const OPTSET_DEBUGGING: &str = "Debugging";
const OPTSET_EVENTS: &str = "Events";
const OPTSET_FILTERING: &str = "Filtering";
const OPTSET_OUTPUT: &str = "Output";

include!(env!("BOSION_PATH"));

/// Execute commands when watched files change.
///
/// Recursively monitors the current directory for changes, executing the command when a filesystem
/// change is detected (among other event sources). By default, watchexec uses efficient
/// kernel-level mechanisms to watch for changes.
///
/// At startup, the specified command is run once, and watchexec begins monitoring for changes.
///
/// Examples:
///
/// Rebuild a project when source files change:
///
///   $ watchexec make
///
/// Watch all HTML, CSS, and JavaScript files for changes:
///
///   $ watchexec -e html,css,js make
///
/// Run tests when source files change, clearing the screen each time:
///
///   $ watchexec -c make test
///
/// Launch and restart a node.js server:
///
///   $ watchexec -r node app.js
///
/// Watch lib and src directories for changes, rebuilding each time:
///
///   $ watchexec -w lib -w src make
#[derive(Debug, Clone, Parser)]
#[command(
	name = "watchexec",
	bin_name = "watchexec",
	author,
	version,
	long_version = Bosion::LONG_VERSION,
	after_help = "Want more detail? Try the long '--help' flag!",
	after_long_help = "Use @argfile as first argument to load arguments from the file 'argfile' (one argument per line) which will be inserted in place of the @argfile (further arguments on the CLI will override or add onto those in the file).\n\nDidn't expect this much output? Use the short '-h' flag to get short help.",
	hide_possible_values = true,
)]
#[cfg_attr(debug_assertions, command(before_help = "⚠ DEBUG BUILD ⚠"))]
#[cfg_attr(
	feature = "dev-console",
	command(before_help = "⚠ DEV CONSOLE ENABLED ⚠")
)]
#[allow(clippy::struct_excessive_bools)]
pub struct Args {
	/// Clear screen before running command
	///
	/// If this doesn't completely clear the screen, try '--clear=reset'.
	#[arg(
		short = 'c',
		long = "clear",
		help_heading = OPTSET_OUTPUT,
		num_args = 0..=1,
		default_missing_value = "clear",
		value_name = "MODE",
	)]
	pub screen_clear: Option<ClearMode>,

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
		value_name = "MODE"
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
		value_name = "SIGNAL"
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
		value_name = "TIMEOUT"
	)]
	pub debounce: TimeSpan<1_000_000>,

	/// Exit when stdin closes
	///
	/// This watches the stdin file descriptor for EOF, and exits Watchexec gracefully when it is
	/// closed. This is used by some process managers to avoid leaving zombie processes around.
	#[arg(
		long,
		help_heading = OPTSET_EVENTS,
	)]
	pub stdin_quit: bool,

	/// Wait until first change before running command
	///
	/// By default, Watchexec will run the command once immediately. With this option, it will
	/// instead wait until an event is detected before running the command as normal.
	#[arg(
		long,
		short,
		help_heading = OPTSET_EVENTS,
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
		required_if_eq("only_emit_events", "true"),
	)]
	pub emit_events_to: EmitEvents,

	/// Only emit events to stdout, run no commands.
	///
	/// This is a convenience option for using Watchexec as a file watcher, without running any
	/// commands. It is almost equivalent to using `cat` as the command, except that it will not
	/// spawn a new process for each event.
	///
	/// This option requires `--emit-events-to` to be set, and restricts the available modes to
	/// `stdio` and `json-stdio`, modifying their behaviour to write to stdout instead of the stdin
	/// of the command.
	#[arg(
		long,
		help_heading = OPTSET_EVENTS,
		conflicts_with_all = ["command", "completions", "manual"],
	)]
	pub only_emit_events: bool,

	/// Testing only: exit Watchexec after the first run
	#[arg(short = '1', hide = true)]
	pub once: bool,

	/// Alert when commands start and end
	///
	/// With this, Watchexec will emit a desktop notification when a command starts and ends, on
	/// supported platforms. On unsupported platforms, it may silently do nothing, or log a warning.
	#[arg(
		short = 'N',
		long,
		help_heading = OPTSET_OUTPUT,
	)]
	pub notify: bool,

	/// When to use terminal colours
	///
	/// Setting the environment variable `NO_COLOR` to any value is equivalent to `--color=never`.
	#[arg(
		long,
		help_heading = OPTSET_OUTPUT,
		default_value = "auto",
		value_name = "MODE",
		alias = "colour",
	)]
	pub color: ColourMode,

	/// Print how long the command took to run
	///
	/// This may not be exactly accurate, as it includes some overhead from Watchexec itself. Use
	/// the `time` utility, high-precision timers, or benchmarking tools for more accurate results.
	#[arg(
		long,
		help_heading = OPTSET_OUTPUT,
	)]
	pub timings: bool,

	/// Don't print starting and stopping messages
	///
	/// By default Watchexec will print a message when the command starts and stops. This option
	/// disables this behaviour, so only the command's output, warnings, and errors will be printed.
	#[arg(
		short,
		long,
		help_heading = OPTSET_OUTPUT,
	)]
	pub quiet: bool,

	/// Ring the terminal bell on command completion
	#[arg(
		long,
		help_heading = OPTSET_OUTPUT,
	)]
	pub bell: bool,

	/// Set the project origin
	///
	/// Watchexec will attempt to discover the project's "origin" (or "root") by searching for a
	/// variety of markers, like files or directory patterns. It does its best but sometimes gets it
	/// it wrong, and you can override that with this option.
	///
	/// The project origin is used to determine the path of certain ignore files, which VCS is being
	/// used, the meaning of a leading '/' in filtering patterns, and maybe more in the future.
	///
	/// When set, Watchexec will also not bother searching, which can be significantly faster.
	#[arg(
		long,
		help_heading = OPTSET_FILTERING,
		value_hint = ValueHint::DirPath,
		value_name = "DIRECTORY",
	)]
	pub project_origin: Option<PathBuf>,

	/// Print events that trigger actions
	///
	/// This prints the events that triggered the action when handling it (after debouncing), in a
	/// human readable form. This is useful for debugging filters.
	///
	/// Use '-vvv' instead when you need more diagnostic information.
	#[arg(
		long,
		help_heading = OPTSET_DEBUGGING,
	)]
	pub print_events: bool,

	/// Show the manual page
	///
	/// This shows the manual page for Watchexec, if the output is a terminal and the 'man' program
	/// is available. If not, the manual page is printed to stdout in ROFF format (suitable for
	/// writing to a watchexec.1 file).
	#[arg(
		long,
		help_heading = OPTSET_DEBUGGING,
		conflicts_with_all = ["command", "completions"],
	)]
	pub manual: bool,

	/// Generate a shell completions script
	///
	/// Provides a completions script or configuration for the given shell. If Watchexec is not
	/// distributed with pre-generated completions, you can use this to generate them yourself.
	///
	/// Supported shells: bash, elvish, fish, nu, powershell, zsh.
	#[arg(
		long,
		help_heading = OPTSET_DEBUGGING,
		conflicts_with_all = ["command", "manual"],
	)]
	pub completions: Option<ShellCompletion>,

	#[command(flatten)]
	pub command: command::CommandArgs,

	#[command(flatten)]
	pub filtering: filtering::FilteringArgs,

	#[command(flatten)]
	pub logging: logging::LoggingArgs,
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

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum ClearMode {
	#[default]
	Clear,
	Reset,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ShellCompletion {
	Bash,
	Elvish,
	Fish,
	Nu,
	Powershell,
	Zsh,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ColourMode {
	Auto,
	Always,
	Never,
}

#[derive(Clone, Copy, Debug)]
pub struct TimeSpan<const UNITLESS_NANOS_MULTIPLIER: u64 = { 1_000_000_000 }>(pub Duration);

impl<const UNITLESS_NANOS_MULTIPLIER: u64> FromStr for TimeSpan<UNITLESS_NANOS_MULTIPLIER> {
	type Err = humantime::DurationError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		s.parse::<u64>()
			.map_or_else(
				|_| humantime::parse_duration(s),
				|unitless| {
					eprintln!("Warning: unitless time span values are deprecated and will be removed in an upcoming version");
					Ok(Duration::from_nanos(unitless * UNITLESS_NANOS_MULTIPLIER))
				},
			)
			.map(TimeSpan)
	}
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

fn expand_args_up_to_doubledash() -> Result<Vec<OsString>, std::io::Error> {
	use argfile::Argument;
	use std::collections::VecDeque;

	let args = std::env::args_os();
	let mut expanded_args = Vec::with_capacity(args.size_hint().0);

	let mut todo: VecDeque<_> = args.map(|a| Argument::parse(a, argfile::PREFIX)).collect();
	while let Some(next) = todo.pop_front() {
		match next {
			Argument::PassThrough(arg) => {
				expanded_args.push(arg.clone());
				if arg == "--" {
					break;
				}
			}
			Argument::Path(path) => {
				let content = std::fs::read_to_string(path)?;
				let new_args = argfile::parse_fromfile(&content, argfile::PREFIX);
				todo.reserve(new_args.len());
				for (i, arg) in new_args.into_iter().enumerate() {
					todo.insert(i, arg);
				}
			}
		}
	}

	while let Some(next) = todo.pop_front() {
		expanded_args.push(match next {
			Argument::PassThrough(arg) => arg,
			Argument::Path(path) => {
				let path = path.as_os_str();
				let mut restored = OsString::with_capacity(path.len() + 1);
				restored.push(OsStr::new("@"));
				restored.push(path);
				restored
			}
		});
	}
	Ok(expanded_args)
}

#[inline]
pub async fn get_args() -> Result<(Args, Option<WorkerGuard>)> {
	let prearg_logs = logging::preargs();
	if prearg_logs {
		warn!("⚠ RUST_LOG environment variable set or hardcoded, logging options have no effect");
	}

	debug!("expanding @argfile arguments if any");
	let args = expand_args_up_to_doubledash().expect("while expanding @argfile");

	debug!("parsing arguments");
	let mut args = Args::parse_from(args);

	let log_guard = if !prearg_logs {
		logging::postargs(&args.logging).await?
	} else {
		None
	};

	// https://no-color.org/
	if args.color == ColourMode::Auto && std::env::var("NO_COLOR").is_ok() {
		args.color = ColourMode::Never;
	}

	if args.signal.is_some() {
		args.on_busy_update = OnBusyUpdate::Signal;
	} else if args.restart {
		args.on_busy_update = OnBusyUpdate::Restart;
	}

	if args.command.no_environment {
		warn!("--no-environment is deprecated");
		args.emit_events_to = EmitEvents::None;
	}

	if args.only_emit_events
		&& !matches!(
			args.emit_events_to,
			EmitEvents::JsonStdio | EmitEvents::Stdio
		) {
		Args::command()
			.error(
				ErrorKind::InvalidValue,
				"only-emit-events requires --emit-events-to=stdio or --emit-events-to=json-stdio",
			)
			.exit();
	}

	if args.stdin_quit && args.filtering.watch_file == Some(PathBuf::from("-")) {
		Args::command()
			.error(
				ErrorKind::InvalidValue,
				"stdin-quit cannot be used when --watch-file=-",
			)
			.exit();
	}

	args.command.normalise()?;

	let project_origin = if let Some(p) = take(&mut args.project_origin) {
		p
	} else {
		crate::dirs::project_origin(&args).await?
	};
	debug!(path=?project_origin, "resolved project origin");
	let project_origin = dunce::canonicalize(project_origin).into_diagnostic()?;
	info!(path=?project_origin, "effective project origin");
	args.project_origin = Some(project_origin.clone());

	args.filtering
		.normalise(&project_origin, args.command.workdir.as_deref().unwrap())
		.await?;

	debug_assert!(args.project_origin.is_some());
	info!(?args, "got arguments");
	Ok((args, log_guard))
}
