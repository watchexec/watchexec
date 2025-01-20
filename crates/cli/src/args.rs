use std::{
	ffi::{OsStr, OsString},
	str::FromStr,
	time::Duration,
};

use clap::{Parser, ValueEnum};
use miette::Result;
use tracing::{debug, info, warn};
use tracing_appender::non_blocking::WorkerGuard;

pub(crate) mod command;
pub(crate) mod events;
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
	pub events: events::EventsArgs,

	#[command(flatten)]
	pub filtering: filtering::FilteringArgs,

	#[command(flatten)]
	pub logging: logging::LoggingArgs,
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

	args.command.normalise()?;

	args.filtering.normalise(&args.command).await?;

	args.events.normalise(&args.command, &args.filtering)?;

	info!(?args, "got arguments");
	Ok((args, log_guard))
}
