use std::{
	ffi::{OsStr, OsString},
	str::FromStr,
	time::Duration,
};

use clap::Parser;
use miette::Result;
use tracing::{debug, info, warn};
use tracing_appender::non_blocking::WorkerGuard;

pub(crate) mod command;
pub(crate) mod debugging;
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
	#[command(flatten)]
	pub command: command::CommandArgs,

	#[command(flatten)]
	pub debugging: debugging::DebuggingArgs,

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

	args.output.normalise()?;
	args.command.normalise()?;
	args.filtering.normalise(&args.command).await?;
	args.events.normalise(&args.command, &args.filtering)?;

	info!(?args, "got arguments");
	Ok((args, log_guard))
}
