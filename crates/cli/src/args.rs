use std::{path::PathBuf, str::FromStr, time::Duration};

use clap::{ArgAction, Parser, ValueEnum, ValueHint};
use tracing::{debug, warn};
use watchexec::signal::process::SubSignal;

const OPTSET_FILTERING: &str = "Filtering";
const OPTSET_COMMAND: &str = "Command";
const OPTSET_DEBUGGING: &str = "Debugging";

// Execute commands when watched files change.
#[derive(Debug, Clone, Parser)]
#[command(
	author,
	version,
	about = "Execute commands when watched files change.",
	long_about = None,
	after_help = "Use @argfile as first argument to load arguments from the file `argfile` (one argument per line) which will be inserted in place of the @argfile (further arguments on the CLI will override or add onto those in the file).",
	hide_possible_values = true,
)]
#[cfg_attr(debug_assertions, command(before_help = "⚠ DEBUG BUILD ⚠"))]
#[cfg_attr(
	feature = "dev-console",
	command(before_help = "⚠ DEV CONSOLE ENABLED ⚠")
)]
pub struct Args {
	/// Command to run on changes
	#[arg(
		trailing_var_arg = true,
		num_args = 1..,
		value_hint = ValueHint::CommandString,
		value_name = "COMMAND",
		required = true,
	)]
	pub command: Vec<String>,

	/// Watch a specific file or directory
	///
	/// When watching a single file, it's often better to watch the containing directory instead,
	/// and filter on the filename. Some editors may replace the file with a new one when saving,
	/// and some platforms may not detect that or further changes.
	///
	/// This option can be specified multiple times to watch multiple files or directories.
	#[arg(
		short = 'w',
		long = "watch",
		help_heading = OPTSET_FILTERING,
		value_hint = ValueHint::AnyPath,
		value_name = "PATH",
	)]
	pub paths: Vec<PathBuf>,

	/// Clear screen before running command
	///
	/// If this doesn't completely clear the screen, try `--clear=reset`.
	#[arg(
		short = 'c',
		long = "clear",
		num_args = 0..=1, // what does that do with 0?
		value_name = "MODE",
	)]
	pub screen_clear: Option<ClearMode>,

	/// What to do when receiving events while the command is running
	///
	/// Default is to **queue** up events and run the command once again when the previous run has
	/// finished. You can also use **do-nothing**, which ignores events while the command is running
	/// and may be useful to avoid spurious changes made by that command, or **restart**, which
	/// terminates the running command and starts a new one. Finally, there's **signal**, which only
	/// sends a signal; this can be useful with programs that can reload their configuration without
	/// a full restart.
	///
	/// The signal can be specified with the `--signal` option.
	///
	/// Note that this option is scheduled to change its default to **do-nothing** in the next major
	/// release. File an issue if you have any concerns.
	#[arg(
		short,
		long,
		default_value = "queue",
		hide_default_value = true,
		value_name = "MODE"
	)]
	pub on_busy_update: OnBusyUpdate,

	/// Deprecated alias for `--on-busy-update=do-nothing`
	///
	/// This option is deprecated and will be removed in the next major release.
	#[arg(
		long,
		short = 'W',
		hide = true,
		conflicts_with = "on_busy_update",
		conflicts_with = "restart"
	)]
	pub watch_when_idle: bool,

	/// Restart the process if it's still running
	///
	/// This is a shorthand for `--on-busy-update=restart`.
	#[arg(
		short,
		long,
		conflicts_with = "on_busy_update",
		conflicts_with = "watch_when_idle"
	)]
	pub restart: bool,

	/// Send a signal to the process when it's still running
	///
	/// Specify a signal to send to the process when it's still running. This implies
	/// `--on-busy-update=signal`; otherwise the signal used when that mode is **restart** is
	/// controlled by `--kill-signal`.
	///
	/// See the long documentation for `--kill-signal` for syntax.
	#[arg(
		short,
		long,
		conflicts_with = "restart",
		conflicts_with = "watch_when_idle",
		value_name = "TIMEOUT"
	)]
	pub signal: Option<SubSignal>,

	/// Hidden legacy shorthand for `--signal=kill`.
	#[arg(short, long)]
	pub kill: bool,

	/// Signal to send to stop the command
	///
	/// This is used by **restart** and **signal** modes of `--on-busy-update` (unless `--signal` is
	/// provided). The restart behaviour is to send the signal, wait for the command to exit, and if
	/// it hasn't exited after some time (see `--timeout-stop`), forcefully terminate it.
	///
	/// The default on unix is **SIGTERM**.
	///
	/// Input is parsed as a full signal name (like "SIGTERM"), a short signal name (like "TERM"),
	/// or a signal number (like "15"). All input is case-insensitive.
	///
	/// On Windows this option is technically supported but only supports the "KILL" event, as
	/// Watchexec cannot yet deliver other events. Windows doesn't have signals as such; instead it
	/// has termination (here called "KILL" or "STOP") and "CTRL+C", "CTRL+BREAK", and "CTRL+CLOSE"
	/// events. For portability the unix signals "SIGKILL", "SIGINT", "SIGTERM", and "SIGHUP" are
	/// respectively mapped to these.
	#[arg(long, value_name = "SIGNAL")]
	pub kill_signal: Option<SubSignal>,

	/// Time to wait for the command to exit gracefully
	///
	/// This is used by the **restart** mode of `--on-busy-update`. After the graceful stop signal
	/// is sent, Watchexec will wait for the command to exit. If it hasn't exited after this time,
	/// it is forcefully terminated.
	///
	/// Takes a unit-less value in seconds, or a time span value such as "5min 20s".
	///
	/// The default is 60 seconds. Set to 0 to immediately force-kill the command.
	#[arg(
		long,
		default_value = "60",
		hide_default_value = true,
		value_name = "TIMEOUT"
	)]
	pub timeout_stop: TimeSpan,

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
	///
	/// The default is 50 milliseconds. Setting to 0 is highly discouraged.
	#[arg(
		long,
		default_value = "50",
		hide_default_value = true,
		value_name = "TIMEOUT"
	)]
	pub debounce: TimeSpan<1_000_000>,

	/// Exit when stdin closes
	///
	/// This watches the stdin file descriptor for EOF, and exits Watchexec gracefully when it is
	/// closed. This is used by some process managers to avoid leaving zombie processes around.
	#[arg(long)]
	pub stdin_quit: bool,

	/// Set diagnostic log level
	///
	/// This enables diagnostic logging, which is useful for investigating bugs or gaining more
	/// insight into faulty filters or "missing" events. Use multiple times to increase verbosity.
	///
	/// Goes up to `-vvvv`. When submitting bug reports, default to a `-vvv` log level.
	///
	/// You may want to use with `--log-file` to avoid polluting your terminal.
	#[arg(
		long,
		short,
		help_heading = OPTSET_DEBUGGING,
		action = ArgAction::Count,
		num_args = 0,
	)]
	pub verbose: Option<u8>,

	/// Write diagnostic logs to a file
	///
	/// This writes diagnostic logs to a file, instead of the terminal, in JSON format. If a log
	/// level was not already specified, this will set it to `-vvv`.
	///
	/// If a path is not provided, the default is the working directory. Note that with
	/// `--ignore-nothing`, the write events to the log will likely get picked up by Watchexec,
	/// causing a loop; prefer setting a path outside of the watched directory.
	///
	/// If the path provided is a directory, a file will be created in that directory. The file name
	/// will be the current date and time, in the format `watchexec.YYYY-MM-DDTHH-MM-SSZ.log`.
	#[arg(
		long,
		help_heading = OPTSET_DEBUGGING,
		num_args = 0..=1,
		value_hint = ValueHint::AnyPath,
		value_name = "PATH",
	)]
	pub log_file: Option<PathBuf>,

	/// Print events that trigger actions
	///
	/// This prints the events that triggered the action when handling it (after debouncing), in a
	/// human readable form. This is useful for debugging filters.
	///
	/// Use `-v` when you need more diagnostic information.
	#[arg(
		long,
		alias = "changes-only", // deprecated
		help_heading = OPTSET_DEBUGGING,
	)]
	pub print_events: bool,

	/// Don't load gitignores
	///
	/// Among other VCS exclude files, like for Mercurial, Subversion, Bazaar, DARCS, Fossil. Note
	/// that Watchexec will detect which of these is in use, if any, and only load the relevant
	/// files. Both global (like `~/.gitignore`) and local (like `.gitignore`) files are considered.
	///
	/// This option is useful if you want to watch files that are ignored by Git.
	#[arg(
		long,
		help_heading = OPTSET_FILTERING,
	)]
	pub no_vcs_ignore: bool,

	/// Don't load project-local ignores
	///
	/// This disables loading of project-local ignore files, like `.gitignore` or `.ignore` in the
	/// watched project. This is contrasted with `--no-vcs-ignore`, which disables loading of Git
	/// and other VCS ignore files, and with `--no-global-ignore`, which disables loading of global
	/// or user ignore files, like `~/.gitignore` or `~/.config/watchexec/ignore`.
	///
	/// Note that this was previously called `--no-ignore`, but that's now deprecated and its use is
	/// discouraged, as it may be repurposed in the future.
	#[arg(
		long,
		help_heading = OPTSET_FILTERING,
		alias = "no-ignore", // deprecated
	)]
	pub no_project_ignore: bool,

	/// Don't load global ignores
	///
	/// This disables loading of global or user ignore files, like `~/.gitignore`,
	/// `~/.config/watchexec/ignore`, or `%APPDATA%\Bazzar\2.0\ignore`. Contrast with
	/// `--no-vcs-ignore` and `--no-project-ignore`.
	#[arg(
		long,
		help_heading = OPTSET_FILTERING,
	)]
	pub no_global_ignore: bool,

	/// Don't use internal default ignores
	///
	/// Watchexec has a set of default ignore patterns, such as editor swap files, git folders,
	/// compiled Python files, and Watchexec log files. This option disables them.
	#[arg(
		long,
		help_heading = OPTSET_FILTERING,
	)]
	pub no_default_ignore: bool,

	/// Wait until first change before running command
	///
	/// By default, Watchexec will run the command once immediately. With this option, it will
	/// instead wait until an event is detected before running the command as normal.
	#[arg(long, short)]
	pub postpone: bool,

	/// Sleep before running the command
	///
	/// This option will cause Watchexec to sleep for the specified amount of time before running
	/// the command, after an event is detected. This is like using "sleep 5 && command" in a shell,
	/// but portable and slightly more efficient.
	///
	/// Takes a unit-less value in seconds, or a time span value such as "2min 5s".
	#[arg(long, value_name = "DURATION")]
	pub delay_run: Option<TimeSpan>,

	/// Poll for filesystem changes
	///
	/// By default, and where available, Watchexec uses the operating system's native file system
	/// watching capabilities. This option disables that and instead uses a polling mechanism, which
	/// is less efficient but can work around issues with some file systems or edge cases.
	///
	/// Optionally takes a unit-less value in milliseconds, or a time span value such as "2s 500ms",
	/// to use as the polling interval. If not specified, the default is 1 second.
	///
	/// Aliased as `--force-poll`.
	#[arg(
		long,
		alias = "force-poll",
		num_args = 0..=1, // TODO how does this work with 0?
		value_name = "INTERVAL",
	)]
	pub poll: Option<TimeSpan>,

	/// Use a different shell
	///
	/// By default, Watchexec will use `sh` on unix and `CMD.EXE` on Windows. With this, you can
	/// override that and use a different shell, for example one with more features or one which has
	/// your custom aliases and functions.
	///
	/// If the value has spaces, it is parsed as a command line, and the first word used as the
	/// shell program, with the rest as arguments to the shell.
	///
	/// The command is run with the `-c` flag (except for `CMD.EXE`, where the `/C` option is used).
	///
	/// Note that the default shell will change at the next major release: the value of `$SHELL`
	/// will be respected, falling back to `sh` on unix and to PowerShell on Windows.
	///
	/// The special value `none` can be used to disable shell use entirely. In that case, the
	/// command provided to Watchexec will be parsed, with the first word being the executable and
	/// the rest being the arguments, and executed directly. Note that this parsing is rudimentary,
	/// and may not work as expected in all cases.
	#[arg(
		long,
		help_heading = OPTSET_COMMAND,
		value_name = "SHELL",
	)]
	pub shell: Option<String>,

	/// Don't use a shell
	///
	/// This is a shorthand for `--shell=none`.
	#[arg(
		short = 'n',
		help_heading = OPTSET_COMMAND,
	)]
	pub no_shell: bool,

	/// Don't use a shell
	///
	/// This is a deprecated alias for `--shell=none`.
	#[arg(
		long,
		hide = true,
		help_heading = OPTSET_COMMAND,
		alias = "no-shell", // deprecated
	)]
	pub no_shell_long: bool,

	/// Shorthand for --emit-events=none
	///
	/// This is the old way to disable event emission into the environment. See `--emit-events` for
	/// more.
	#[arg(
		long,
		help_heading = OPTSET_COMMAND,
		// TODO: deprecate then remove
	)]
	pub no_environment: bool,

	/// Configure event emission
	///
	/// Watchexec emits event information when running a command, which can be used by the command
	/// to target specific changed files. This option controls where that information is emitted. It
	/// defaults to **environment**, which sets environment variables with the paths of the affected
	/// files, for filesystem events:
	///
	/// **$WATCHEXEC_COMMON_PATH** is set to the longest common path of all of the below variables,
	/// and so should be prepended to each path to obtain the full/real path. Then:
	///
	/// - **$WATCHEXEC_CREATED_PATH** is set when files/folders were created
	/// - **$WATCHEXEC_REMOVED_PATH** is set when files/folders were removed
	/// - **$WATCHEXEC_RENAMED_PATH** is set when files/folders were renamed
	/// - **$WATCHEXEC_WRITTEN_PATH** is set when files/folders were modified
	/// - **$WATCHEXEC_META_CHANGED_PATH** is set when files/folders' metadata were modified
	/// - **$WATCHEXEC_OTHERWISE_CHANGED_PATH** is set for every other kind of pathed event
	///
	/// Multiple paths are separated by the system path separator, `;` on Windows and `:` on unix.
	/// Within each variable, paths are deduplicated and sorted in binary order (i.e. neither
	/// Unicode nor locale aware).
	///
	/// This is the legacy mode and will be deprecated and removed in the future. The environment of
	/// a process is a very restricted space, while also limited in what it can usefully represent.
	/// Large numbers of files will either cause the environment to be truncated, or may error or
	/// crash the process entirely.
	///
	/// Two new modes are available: **stdin** writes absolute paths to the stdin of the command,
	/// one per line, each prefixed with `create:`, `remove:`, `rename:`, `modify:`, or `other:`,
	/// then closes the handle; **file** writes the same thing to a temporary file, and its path is
	/// given with the **$WATCHEXEC_EVENTS_FILE** environment variable.
	///
	/// There are also two JSON modes, which are based on JSON objects and can represent the full
	/// set of events Watchexec handles. Here's an example of a folder being created on Linux:
	///
	/// ```json
	/// {"t":[{"k":"path","p":"/home/user/your/new-folder","f":"dir"},{"k":"fs","s":"create","f":"Create(Folder)"}],"m":{"notify-backend":"inotify"}}
	/// ```
	///
	/// The fields are as follows:
	///
	/// - `t`, an array of "tags", which are structured event data.
	/// - `t[].k`, the kind of tag, which can be:
	///   * `path`, along with:
	///     + `p`, an absolute path.
	///     + `f`, a file type if known (`dir`, `file`, `symlink`, `other`).
	///   * `fs`:
	///     + `s`, the "simple" event type (`access`, `create`, `modify`, `remove`, or `other`).
	///     + `f`, the "full" event type, which is too complex to fully describe here, but looks like `General(Precise(Specific))`.
	///   * `source`, along with:
	///     + `s`, the source of the event (`filesystem`, `keyboard`, `mouse`, `os`, `time`, `internal`).
	///   * `keyboard`, along with:
	///     + `c`, the code of the key entered. Currently only the value "eof" is supported.
	///   * `process`, for events caused by processes:
	///     + `i`, the process ID.
	///   * `signal`, for signals sent to Watchexec:
	///    + `n`, the normalised signal name (`hangup`, `interrupt`, `quit`, `terminate`, `user1`, `user2`).
	///   * `completion`, for when a command ends:
	///    + `e`, the exit disposition (`success`, `error`, `signal`, `stop`, `exception`, `continued`).
	///    + `c`, the exit, stop, or exception code.
	///    + `s`, the signal name or number if the exit was caused by a signal.
	///
	/// The **json-stdin** mode will emit JSON events to the standard input of the command, one per
	/// line, then close stdin. The **json-file** mode will create a temporary file, write the
	/// events to it, and provide the path to the file with the **$WATCHEXEC_EVENTS_FILE**
	/// environment variable.
	///
	/// Finally, the special **none** mode will disable event emission entirely.
	#[arg(
		long,
		help_heading = OPTSET_COMMAND,
		verbatim_doc_comment,
		default_value = "environment",
		hide_default_value = true,
		value_name = "MODE",
	)]
	pub emit_events_to: EmitEvents,

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
	)]
	pub env: Vec<String>,

	/// Don't use a process group
	///
	/// By default, Watchexec will run the command in a process group, so that signals and
	/// terminations are sent to all processes in the group. Sometimes that's not what you want, and
	/// you can disable the behaviour with this option.
	#[arg(
		long,
		help_heading = OPTSET_COMMAND,
	)]
	pub no_process_group: bool,

	/// Testing only: exit Watchexec after the first run
	#[arg(short = '1', hide = true)]
	pub once: bool,

	/// Alert when commands start and end
	///
	/// With this, Watchexec will emit a desktop notification when a command starts and ends, on
	/// supported platforms. On unsupported platforms, it may silently do nothing, or log a warning.
	#[arg(long, short = 'N')]
	pub notify: bool,

	/// Set the project origin
	///
	/// Watchexec will attempt to discover the project's "origin" (or "root") by searching for a
	/// variety of markers, like files or directory patterns. It does its best but sometimes gets it
	/// it wrong, and you can override that with this option.
	///
	/// The project origin is used to determine the path of certain ignore files, which VCS is being
	/// used, the meaning of a leading `/` in filtering patterns, and maybe more in the future.
	///
	/// When set, Watchexec will also not bother searching, which can be significantly faster.
	#[arg(
		long,
		value_hint = ValueHint::DirPath,
		value_name = "DIRECTORY",
	)]
	pub project_origin: Option<PathBuf>,

	/// Set the working directory
	///
	/// By default, the working directory of the command is the working directory of Watchexec. You
	/// can change that with this option. Note that paths may be less intuitive to use with this.
	#[arg(
		long,
		value_hint = ValueHint::DirPath,
		value_name = "DIRECTORY",
	)]
	pub workdir: Option<PathBuf>,

	/// Filename extensions to filter to
	///
	/// This is a quick filter to only emit events for files with the given extensions. Extensions
	/// can be given with or without the leading dot (e.g. `js` or `.js`). Multiple extensions can
	/// be given by repeating the option or by separating them with commas.
	#[arg(
		long = "exts",
		short = 'e',
		help_heading = OPTSET_FILTERING,
		value_delimiter = ',',
		value_name = "EXTENSIONS",
	)]
	pub filter_extensions: Vec<String>,

	/// Filename patterns to filter to
	///
	/// Provide a glob-like filter pattern, and only events for files matching the pattern will be
	/// emitted. Multiple patterns can be given by repeating the option. Events that are not from
	/// files (e.g. signals, keyboard events) will pass through untouched.
	#[arg(
		long = "filter",
		short = 'f',
		help_heading = OPTSET_FILTERING,
		value_name = "PATTERN",
	)]
	pub filter_patterns: Vec<String>,

	/// Filename patterns to filter out
	///
	/// Provide a glob-like filter pattern, and events for files matching the pattern will be
	/// excluded. Multiple patterns can be given by repeating the option. Events that are not from
	/// files (e.g. signals, keyboard events) will pass through untouched.
	#[arg(
		long = "ignore",
		short = 'i',
		help_heading = OPTSET_FILTERING,
		value_name = "PATTERN",
	)]
	pub ignore_patterns: Vec<String>,

	/// Filesystem events to filter to
	///
	/// This is a quick filter to only emit events for the given types of filesystem changes. Choose
	/// from `access`, `create`, `remove`, `rename`, `modify`, `metadata`. Multiple types can be
	/// given by repeating the option or by separating them with commas. By default, this is all
	/// types except for `access`.
	///
	/// This may apply filtering at the kernel level when possible, which can be more efficient, but
	/// may be more confusing when reading the logs.
	#[arg(
		long = "fs-events",
		help_heading = OPTSET_FILTERING,
		default_value = "create,remove,rename,modify,metadata",
		value_delimiter = ',',
		hide_default_value = true,
		value_name = "EVENTS",
	)]
	pub filter_fs_events: Vec<FsEvent>,

	/// Don't emit fs events for metadata changes
	///
	/// This is a shorthand for `--fs-events create,remove,rename,modify`. Using it alongside the
	/// `--fs-events` option is non-sensical and not allowed.
	#[arg(
		long = "no-meta",
		help_heading = OPTSET_FILTERING,
		conflicts_with = "filter_fs_events",
	)]
	pub filter_fs_meta: bool,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum EmitEvents {
	#[default]
	Environment,
	Stdin,
	File,
	JsonStdin,
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
pub enum FsEvent {
	Access,
	Create,
	Remove,
	Rename,
	Modify,
	Metadata,
}

// TODO: FromStr or ValueParser
#[derive(Clone, Copy, Debug)]
pub struct TimeSpan<const UNITLESS_NANOS_MULTIPLIER: u64 = { 1_000_000_000 }>(pub Duration);

impl<const UNITLESS_NANOS_MULTIPLIER: u64> FromStr for TimeSpan<UNITLESS_NANOS_MULTIPLIER> {
	type Err = humantime::DurationError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s.parse::<u64>() {
			Ok(unitless) => Ok(Duration::from_nanos(unitless * UNITLESS_NANOS_MULTIPLIER)),
			Err(_) => humantime::parse_duration(s),
		}
		.map(TimeSpan)
	}
}

#[inline]
pub fn get_args() -> Args {
	if std::env::var("RUST_LOG").is_ok() {
		warn!("⚠ RUST_LOG environment variable set, logging options have no effect");
	}

	if let Ok(filt) = std::env::var("WATCHEXEC_FILTERER") {
		warn!("WATCHEXEC_FILTERER is deprecated");
		if filt == "tagged" {
			eprintln!("Tagged filterer has been removed. Open an issue if you have no workaround.");
		}
	}

	debug!("expanding @argfile arguments if any");
	let args = argfile::expand_args(argfile::parse_fromfile, argfile::PREFIX).unwrap();

	debug!("parsing arguments");
	let mut args = Args::parse_from(args);

	if args.signal.is_some() {
		args.on_busy_update = OnBusyUpdate::Signal;
	} else if args.restart {
		args.on_busy_update = OnBusyUpdate::Restart;
	} else if args.watch_when_idle {
		args.on_busy_update = OnBusyUpdate::DoNothing;
	}

	if args.kill {
		args.signal = Some(SubSignal::ForceStop);
	}

	if args.no_environment {
		args.emit_events_to = EmitEvents::None;
	}

	if args.filter_fs_meta {
		args.filter_fs_events = vec![
			FsEvent::Create,
			FsEvent::Remove,
			FsEvent::Rename,
			FsEvent::Modify,
		];
	}

	debug!(?args, "got arguments");
	args
}
