use std::{
	env,
	ffi::OsString,
	fs::File,
	io::{BufRead, BufReader},
	path::Path,
};

use clap::{crate_version, Arg, ArgMatches, Command};
use miette::{Context, IntoDiagnostic, Result};

const OPTSET_FILTERING: &str = "Filtering options";
const OPTSET_COMMAND: &str = "Command options";
const OPTSET_DEBUGGING: &str = "Debugging options";
const OPTSET_OUTPUT: &str = "Output options";
const OPTSET_BEHAVIOUR: &str = "Behaviour options";

pub fn get_args(tagged_filterer: bool) -> Result<ArgMatches> {
	let app = Command::new("watchexec")
		.version(crate_version!())
		.about("Execute commands when watched files change")
		.after_help("Use @argfile as first argument to load arguments from the file `argfile` (one argument per line) which will be inserted in place of the @argfile (further arguments on the CLI will override or add onto those in the file).")
		.arg(Arg::new("command")
			.help_heading(Some(OPTSET_COMMAND))
			.help("Command to execute")
			.multiple_occurrences(true)
			.required(true))
		.arg(Arg::new("paths")
			.help_heading(Some(OPTSET_FILTERING))
			.help("Watch a specific file or directory")
			.short('w')
			.long("watch")
			.value_name("path")
			.number_of_values(1)
			.multiple_occurrences(true)
			.takes_value(true))
		.arg(Arg::new("clear")
			.help_heading(Some(OPTSET_OUTPUT))
			.help("Clear screen before executing command")
			.short('c')
			.long("clear"))
		.arg(Arg::new("on-busy-update")
			.help_heading(Some(OPTSET_BEHAVIOUR))
			.help("Select the behaviour to use when receiving events while the command is running. Current default is queue, will change to do-nothing in 2.0.")
			.takes_value(true)
			.possible_values(&["do-nothing", "queue", "restart", "signal"])
			.long("on-busy-update"))
		.arg(Arg::new("restart")
			.help_heading(Some(OPTSET_BEHAVIOUR))
			.help("Restart the process if it's still running. Shorthand for --on-busy-update=restart")
			.short('r')
			.long("restart"))
		.arg(Arg::new("signal")
			.help_heading(Some(OPTSET_BEHAVIOUR))
			.help("Specify the signal to send when using --on-busy-update=signal")
			.short('s')
			.long("signal")
			.takes_value(true)
			.value_name("signal"))
		.arg(Arg::new("kill")
			.help_heading(Some(OPTSET_BEHAVIOUR))
			.hide(true)
			.short('k')
			.long("kill"))
		.arg(Arg::new("debounce")
			.help_heading(Some(OPTSET_BEHAVIOUR))
			.help("Set the timeout between detected change and command execution, defaults to 50ms")
			.takes_value(true)
			.value_name("milliseconds")
			.short('d')
			.long("debounce"))
		.arg(Arg::new("verbose")
			.help_heading(Some(OPTSET_DEBUGGING))
			.help("Print debugging messages (-v, -vv, -vvv, -vvvv; use -vvv for bug reports)")
			.multiple_occurrences(true)
			.short('v')
			.long("verbose"))
		.arg(Arg::new("print-events")
			.help_heading(Some(OPTSET_DEBUGGING))
			.help("Print events that trigger actions")
			.long("print-events")
			.alias("changes-only")) // --changes-only is deprecated (remove at v2)
		.arg(Arg::new("no-vcs-ignore")
			.help_heading(Some(OPTSET_FILTERING))
			.help("Skip auto-loading of VCS (Git, etc) ignore files")
			.long("no-vcs-ignore"))
		.arg(Arg::new("no-project-ignore")
			.help_heading(Some(OPTSET_FILTERING))
			.help("Skip auto-loading of project ignore files (.gitignore, .ignore, etc)")
			.long("no-project-ignore")
			.alias("no-ignore")) // --no-ignore is deprecated (remove at v2)
		.arg(Arg::new("no-default-ignore")
			.help_heading(Some(OPTSET_FILTERING))
			.help("Skip auto-ignoring of commonly ignored globs")
			.long("no-default-ignore"))
		.arg(Arg::new("no-global-ignore")
			.help_heading(Some(OPTSET_FILTERING))
			.help("Skip auto-loading of global or environment-wide ignore files")
			.long("no-global-ignore"))
		.arg(Arg::new("postpone")
			.help_heading(Some(OPTSET_BEHAVIOUR))
			.help("Wait until first change to execute command")
			.short('p')
			.long("postpone"))
		.arg(Arg::new("poll")
			.help_heading(Some(OPTSET_BEHAVIOUR))
			.help("Force polling mode (interval in milliseconds)")
			.long("force-poll")
			.value_name("interval"))
		.arg(Arg::new("shell")
			.help_heading(Some(OPTSET_COMMAND))
			.help(if cfg!(windows) {
				"Use a different shell, or `none`. Try --shell=powershell, which will become the default in 2.0."
			} else {
			"Use a different shell, or `none`. Defaults to `sh` (until 2.0, where that will change to `$SHELL`). E.g. --shell=bash"
			})
			.takes_value(true)
			.long("shell"))
		// -n short form will not be removed, and instead become a shorthand for --shell=none
		.arg(Arg::new("no-shell")
			.help_heading(Some(OPTSET_COMMAND))
			.help("Do not wrap command in a shell. Deprecated: use --shell=none instead.")
			.short('n')
			.long("no-shell"))
		.arg(Arg::new("no-environment")
			.help_heading(Some(OPTSET_COMMAND))
			.help("Do not set WATCHEXEC_*_PATH environment variables for the command")
			.long("no-environment"))
		.arg(Arg::new("no-process-group")
			.help_heading(Some(OPTSET_COMMAND))
			.help("Do not use a process group when running the command")
			.long("no-process-group"))
		.arg(Arg::new("once").short('1').hide(true))
		.arg(Arg::new("watch-when-idle")
			.help_heading(Some(OPTSET_BEHAVIOUR))
			.help("Deprecated alias for --on-busy-update=do-nothing, which will become the default in 2.0.")
			.short('W')
			.hide(true)
			.long("watch-when-idle"))
		.arg(Arg::new("notif")
			.help_heading(Some(OPTSET_OUTPUT))
			.help("Send a desktop notification when the command ends")
			.short('N')
			.long("notify"))
		.arg(Arg::new("project-origin")
			.help_heading(Some(OPTSET_FILTERING))
			.help("Override the project origin: the directory from which ignore files are detected")
			.value_name("path")
			.long("project-origin"))
		.arg(Arg::new("command-workdir")
			.help_heading(Some(OPTSET_COMMAND))
			.help("Change the working directory of the command")
			.value_name("path")
			.long("workdir"))
		.arg(Arg::new("command-env")
			.help_heading(Some(OPTSET_COMMAND))
			.help("Add an environment variable to the command")
			.value_name("name=value")
			.long("env")
			.short('E')
			.number_of_values(1)
			.multiple_occurrences(true));

	let app = if tagged_filterer {
		app.arg(
			Arg::new("filter")
				.help_heading(Some(OPTSET_FILTERING))
				.help("Add tagged filter (e.g. 'path=foo*', 'type=dir', 'kind=Create(*)')")
				.short('f')
				.long("filter")
				.number_of_values(1)
				.multiple_occurrences(true)
				.takes_value(true)
				.value_name("tagged filter"),
		)
		.arg(
			Arg::new("filter-files")
				.help_heading(Some(OPTSET_FILTERING))
				.help("Load tagged filters from a file")
				.short('F')
				.long("filter-file")
				.number_of_values(1)
				.multiple_occurrences(true)
				.takes_value(true)
				.value_name("path"),
		)
		.arg(
			Arg::new("no-global-filters")
				.help_heading(Some(OPTSET_FILTERING))
				.help("Skip auto-loading of global or environment-wide ignore files")
				.long("no-default-filters"),
		)
		.arg(
			Arg::new("no-meta")
				.help_heading(Some(OPTSET_FILTERING))
				.help("Ignore metadata changes (equivalent of `-f 'kind*!Modify(Metadata(*))'`)")
				.long("no-meta"),
		)
	} else {
		app.arg(
			Arg::new("extensions")
				.help_heading(Some(OPTSET_FILTERING))
				.help("Comma-separated list of file extensions to watch (e.g. js,css,html)")
				.short('e')
				.long("exts")
				.takes_value(true),
		)
		.arg(
			Arg::new("filter")
				.help_heading(Some(OPTSET_FILTERING))
				.help("Ignore all modifications except those matching the pattern")
				.short('f')
				.long("filter")
				.number_of_values(1)
				.multiple_occurrences(true)
				.takes_value(true)
				.value_name("pattern"),
		)
		.arg(
			Arg::new("ignore")
				.help_heading(Some(OPTSET_FILTERING))
				.help("Ignore modifications to paths matching the pattern")
				.short('i')
				.long("ignore")
				.number_of_values(1)
				.multiple_occurrences(true)
				.takes_value(true)
				.value_name("pattern"),
		)
		.arg(
			Arg::new("no-meta")
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
