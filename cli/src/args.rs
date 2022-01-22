use std::{
	env,
	ffi::OsString,
	fs::File,
	io::{BufRead, BufReader},
	path::Path,
};

use clap::{crate_version, App, Arg, ArgMatches};
use miette::{Context, IntoDiagnostic, Result};

trait Clap3Compat {
	/// Does nothing for clap2, but remove this trait for clap3, and get cool new option groups!
	fn help_heading(self, _heading: impl Into<Option<&'static str>>) -> Self
	where
		Self: Sized,
	{
		self
	}
}

impl Clap3Compat for Arg<'_, '_> {}

const OPTSET_FILTERING: &str = "Filtering options:";
const OPTSET_COMMAND: &str = "Command options:";
const OPTSET_DEBUGGING: &str = "Debugging options:";
const OPTSET_OUTPUT: &str = "Output options:";
const OPTSET_BEHAVIOUR: &str = "Behaviour options:";

pub fn get_args(tagged_filterer: bool) -> Result<ArgMatches<'static>> {
	let app = App::new("watchexec")
		.version(crate_version!())
		.about("Execute commands when watched files change")
		.after_help("Use @argfile as first argument to load arguments from the file `argfile` (one argument per line) which will be inserted in place of the @argfile (further arguments on the CLI will override or add onto those in the file).")
		.arg(Arg::with_name("command")
			.help_heading(Some(OPTSET_COMMAND))
			.help("Command to execute")
			.multiple(true)
			.required(true))
		.arg(Arg::with_name("paths")
			.help_heading(Some(OPTSET_FILTERING))
			.help("Watch a specific file or directory")
			.short("w")
			.long("watch")
			.value_name("path")
			.number_of_values(1)
			.multiple(true)
			.takes_value(true))
		.arg(Arg::with_name("clear")
			.help_heading(Some(OPTSET_BEHAVIOUR))
			.help("Clear screen before executing command")
			.short("c")
			.long("clear"))
		.arg(Arg::with_name("on-busy-update")
			.help_heading(Some(OPTSET_BEHAVIOUR))
			.help("Select the behaviour to use when receiving events while the command is running. Current default is queue, will change to do-nothing in 2.0.")
			.takes_value(true)
			.possible_values(&["do-nothing", "queue", "restart", "signal"])
			.long("on-busy-update"))
		.arg(Arg::with_name("restart")
			.help_heading(Some(OPTSET_BEHAVIOUR))
			.help("Restart the process if it's still running. Shorthand for --on-busy-update=restart")
			.short("r")
			.long("restart"))
		.arg(Arg::with_name("signal")
			.help_heading(Some(OPTSET_BEHAVIOUR))
			.help("Specify the signal to send when using --on-busy-update=signal")
			.short("s")
			.long("signal")
			.takes_value(true)
			.value_name("signal")
			.default_value("SIGTERM")
			.hidden(cfg!(windows)))
		.arg(Arg::with_name("kill")
			.help_heading(Some(OPTSET_BEHAVIOUR))
			.hidden(true)
			.short("k")
			.long("kill"))
		.arg(Arg::with_name("debounce")
			.help_heading(Some(OPTSET_BEHAVIOUR))
			.help("Set the timeout between detected change and command execution, defaults to 50ms")
			.takes_value(true)
			.value_name("milliseconds")
			.short("d")
			.long("debounce"))
		.arg(Arg::with_name("verbose")
			.help_heading(Some(OPTSET_DEBUGGING))
			.help("Print debugging messages (-v, -vv, -vvv, -vvvv; use -vvv for bug reports)")
			.multiple(true)
			.short("v")
			.long("verbose"))
		.arg(Arg::with_name("print-events")
			.help_heading(Some(OPTSET_DEBUGGING))
			.help("Print events that trigger actions")
			.long("print-events")
			.alias("changes-only")) // --changes-only is deprecated (remove at v2)
		.arg(Arg::with_name("no-vcs-ignore")
			.help_heading(Some(OPTSET_FILTERING))
			.help("Skip auto-loading of VCS (Git, etc) ignore files")
			.long("no-vcs-ignore"))
		.arg(Arg::with_name("no-project-ignore")
			.help_heading(Some(OPTSET_FILTERING))
			.help("Skip auto-loading of project ignore files (.gitignore, .ignore, etc)")
			.long("no-project-ignore")
			.alias("no-ignore")) // --no-ignore is deprecated (remove at v2)
		.arg(Arg::with_name("no-default-ignore")
			.help_heading(Some(OPTSET_FILTERING))
			.help("Skip auto-ignoring of commonly ignored globs")
			.long("no-default-ignore"))
		.arg(Arg::with_name("no-global-ignore")
			.help_heading(Some(OPTSET_FILTERING))
			.help("Skip auto-loading of global or environment-wide ignore files")
			.long("no-global-ignore"))
		.arg(Arg::with_name("postpone")
			.help_heading(Some(OPTSET_BEHAVIOUR))
			.help("Wait until first change to execute command")
			.short("p")
			.long("postpone"))
		.arg(Arg::with_name("poll")
			.help_heading(Some(OPTSET_BEHAVIOUR))
			.help("Force polling mode (interval in milliseconds)")
			.long("force-poll")
			.value_name("interval"))
		.arg(Arg::with_name("shell")
			.help_heading(Some(OPTSET_COMMAND))
			.help(if cfg!(windows) {
				"Use a different shell, or `none`. Try --shell=powershell, which will become the default in 2.0."
			} else {
			"Use a different shell, or `none`. Defaults to `sh` (until 2.0, where that will change to `$SHELL`). E.g. --shell=bash"
			})
			.takes_value(true)
			.long("shell"))
		// -n short form will not be removed, and instead become a shorthand for --shell=none
		.arg(Arg::with_name("no-shell")
			.help_heading(Some(OPTSET_COMMAND))
			.help("Do not wrap command in a shell. Deprecated: use --shell=none instead.")
			.short("n")
			.long("no-shell"))
		.arg(Arg::with_name("no-environment")
			.help_heading(Some(OPTSET_OUTPUT))
			.help("Do not set WATCHEXEC_*_PATH environment variables for the command")
			.long("no-environment"))
		.arg(Arg::with_name("no-process-group")
			.help_heading(Some(OPTSET_COMMAND))
			.help("Do not use a process group when running the command")
			.long("no-process-group"))
		.arg(Arg::with_name("once").short("1").hidden(true))
		.arg(Arg::with_name("watch-when-idle")
			.help_heading(Some(OPTSET_BEHAVIOUR))
			.help("Deprecated alias for --on-busy-update=do-nothing, which will become the default in 2.0.")
			.short("W")
			.hidden(true)
			.long("watch-when-idle"))
		.arg(Arg::with_name("notif")
			.help_heading(Some(OPTSET_OUTPUT))
			.help("Send a desktop notification when the command ends")
			.short("N")
			.long("notify"));

	let app = if tagged_filterer {
		app.arg(
			Arg::with_name("filter")
				.help_heading(Some(OPTSET_FILTERING))
				.help("Add tagged filter (e.g. 'path=foo*', 'type=dir', 'kind=Create(*)')")
				.short("f")
				.long("filter")
				.number_of_values(1)
				.multiple(true)
				.takes_value(true)
				.value_name("tagged filter"),
		)
		.arg(
			Arg::with_name("filter-files")
				.help_heading(Some(OPTSET_FILTERING))
				.help("Load tagged filters from a file")
				.short("F")
				.long("filter-file")
				.number_of_values(1)
				.multiple(true)
				.takes_value(true)
				.value_name("path"),
		)
		.arg(
			Arg::with_name("no-global-filters")
				.help_heading(Some(OPTSET_FILTERING))
				.help("Skip auto-loading of global or environment-wide ignore files")
				.long("no-default-filters"),
		)
		.arg(
			Arg::with_name("no-meta")
				.help_heading(Some(OPTSET_FILTERING))
				.help("Ignore metadata changes (equivalent of `-f 'kind*!Modify(Metadata(*))'`)")
				.long("no-meta"),
		)
	} else {
		app.arg(
			Arg::with_name("extensions")
				.help_heading(Some(OPTSET_FILTERING))
				.help("Comma-separated list of file extensions to watch (e.g. js,css,html)")
				.short("e")
				.long("exts")
				.takes_value(true),
		)
		.arg(
			Arg::with_name("filter")
				.help_heading(Some(OPTSET_FILTERING))
				.help("Ignore all modifications except those matching the pattern")
				.short("f")
				.long("filter")
				.number_of_values(1)
				.multiple(true)
				.takes_value(true)
				.value_name("pattern"),
		)
		.arg(
			Arg::with_name("ignore")
				.help_heading(Some(OPTSET_FILTERING))
				.help("Ignore modifications to paths matching the pattern")
				.short("i")
				.long("ignore")
				.number_of_values(1)
				.multiple(true)
				.takes_value(true)
				.value_name("pattern"),
		)
		.arg(
			Arg::with_name("no-meta")
				.help_heading(Some(OPTSET_FILTERING))
				.help("Ignore metadata changes")
				.long("no-meta"),
		)
	};

	let mut raw_args: Vec<OsString> = env::args_os().collect();

	if let Some(first) = raw_args.get(1).and_then(|s| s.to_str()) {
		if let Some(arg_path) = first.strip_prefix('@').map(Path::new) {
			let arg_file = BufReader::new(
				File::open(arg_path)
					.into_diagnostic()
					.wrap_err_with(|| format!("Failed to open argument file {:?}", arg_path))?,
			);

			let mut more_args: Vec<OsString> = arg_file
				.lines()
				.map(|l| l.map(OsString::from).into_diagnostic())
				.collect::<Result<_>>()?;

			more_args.insert(0, raw_args.remove(0));
			more_args.extend(raw_args.into_iter().skip(1));
			raw_args = more_args;
		}
	}

	Ok(app.get_matches_from(raw_args))
}
