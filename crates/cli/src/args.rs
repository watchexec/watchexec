use std::{
	ffi::{OsStr, OsString},
	str::FromStr,
	time::Duration,
};

use clap::{Parser, ValueEnum, ValueHint};
use miette::Result;
use tracing::{debug, info, warn};
use tracing_appender::non_blocking::WorkerGuard;

pub(crate) mod command;
pub(crate) mod events;
pub(crate) mod filtering;
pub(crate) mod logging;
pub(crate) mod output;

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
/// Events are debounced and checked using a variety of mechanisms, which you can control using
/// the flags in the **Filtering** section. The order of execution is: internal prioritisation
/// (signals come before everything else, and SIGINT/SIGTERM are processed even more urgently),
/// then file event kind (`--fs-events`), then files explicitly watched with `-w`, then ignores
/// (`--ignore` and co), then filters (which includes `--exts`), then filter programs.
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
pub struct Args {
	/// Command (program and arguments) to run on changes
	///
	/// It's run when events pass filters and the debounce period (and once at startup unless
	/// '--postpone' is given). If you pass flags to the command, you should separate it with --
	/// though that is not strictly required.
	///
	/// Examples:
	///
	///   $ watchexec -w src npm run build
	///
	///   $ watchexec -w src -- rsync -a src dest
	///
	/// Take care when using globs or other shell expansions in the command. Your shell may expand
	/// them before ever passing them to Watchexec, and the results may not be what you expect.
	/// Compare:
	///
	///   $ watchexec echo src/*.rs
	///
	///   $ watchexec echo 'src/*.rs'
	///
	///   $ watchexec --shell=none echo 'src/*.rs'
	///
	/// Behaviour depends on the value of '--shell': for all except 'none', every part of the
	/// command is joined together into one string with a single ascii space character, and given to
	/// the shell as described in the help for '--shell'. For 'none', each distinct element the
	/// command is passed as per the execvp(3) convention: first argument is the program, as a path
	/// or searched for in the 'PATH' environment variable, rest are arguments.
	#[arg(
		trailing_var_arg = true,
		num_args = 1..,
		value_hint = ValueHint::CommandString,
		value_name = "COMMAND",
		required_unless_present_any = ["completions", "manual", "only_emit_events"],
	)]
	pub program: Vec<String>,

	/// Show the manual page
	///
	/// This shows the manual page for Watchexec, if the output is a terminal and the 'man' program
	/// is available. If not, the manual page is printed to stdout in ROFF format (suitable for
	/// writing to a watchexec.1 file).
	#[arg(
		long,
		conflicts_with_all = ["program", "completions", "only_emit_events"],
		display_order = 130,
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
		value_name = "SHELL",
		conflicts_with_all = ["program", "manual", "only_emit_events"],
		display_order = 30,
	)]
	pub completions: Option<ShellCompletion>,

	/// Only emit events to stdout, run no commands.
	///
	/// This is a convenience option for using Watchexec as a file watcher, without running any
	/// commands. It is almost equivalent to using `cat` as the command, except that it will not
	/// spawn a new process for each event.
	///
	/// This option implies `--emit-events-to=json-stdio`; you may also use the text mode by
	/// specifying `--emit-events-to=stdio`.
	#[arg(
		long,
		conflicts_with_all = ["program", "completions", "manual"],
		display_order = 150,
	)]
	pub only_emit_events: bool,

	/// Testing only: exit Watchexec after the first run and return the command's exit code
	#[arg(short = '1', hide = true)]
	pub once: bool,

	#[command(flatten)]
	pub command: command::CommandArgs,

	#[command(flatten)]
	pub events: events::EventsArgs,

	#[command(flatten)]
	pub filtering: filtering::FilteringArgs,

	#[command(flatten)]
	pub logging: logging::LoggingArgs,

	#[command(flatten)]
	pub output: output::OutputArgs,
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
					if unitless != 0 {
						eprintln!("Warning: unitless non-zero time span values are deprecated and will be removed in an upcoming version");
					}
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ShellCompletion {
	Bash,
	Elvish,
	Fish,
	Nu,
	Powershell,
	Zsh,
}

#[derive(Debug, Default)]
pub struct Guards {
	_log: Option<WorkerGuard>,
}

pub async fn get_args() -> Result<(Args, Guards)> {
	let prearg_logs = logging::preargs();
	if prearg_logs {
		warn!(
			"⚠ WATCHEXEC_LOG environment variable set or hardcoded, logging options have no effect"
		);
	}

	debug!("expanding @argfile arguments if any");
	let args = expand_args_up_to_doubledash().expect("while expanding @argfile");

	debug!("parsing arguments");
	let mut args = Args::parse_from(args);

	let _log = if !prearg_logs {
		logging::postargs(&args.logging).await?
	} else {
		None
	};

	args.output.normalise()?;
	args.command.normalise().await?;
	args.filtering.normalise(&args.command).await?;
	args.events
		.normalise(&args.command, &args.filtering, args.only_emit_events)?;

	info!(?args, "got arguments");
	Ok((args, Guards { _log }))
}

#[test]
fn verify_cli() {
	use clap::CommandFactory;
	Args::command().debug_assert()
}
