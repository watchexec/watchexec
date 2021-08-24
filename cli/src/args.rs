use std::{
	env,
	ffi::OsString,
	fs::File,
	io::{BufRead, BufReader},
	path::Path,
};

use clap::{crate_version, App, Arg, ArgMatches};
use color_eyre::eyre::{Context, Report, Result};

pub fn get_args() -> Result<ArgMatches<'static>> {
	let app = App::new("watchexec")
		.version(crate_version!())
		.about("Execute commands when watched files change")
		.after_help("Use @argfile as first argument to load arguments from the file `argfile` (one argument per line) which will be inserted in place of the @argfile (further arguments on the CLI will override or add onto those in the file).")
		.arg(Arg::with_name("command")
			.help("Command to execute")
			.multiple(true)
			.required(true))
		.arg(Arg::with_name("extensions") // TODO
			.help("Comma-separated list of file extensions to watch (e.g. js,css,html)")
			.short("e")
			.long("exts")
			.takes_value(true))
		.arg(Arg::with_name("paths")
			.help("Watch a specific file or directory")
			.short("w")
			.long("watch")
			.value_name("path")
			.number_of_values(1)
			.multiple(true)
			.takes_value(true))
		.arg(Arg::with_name("clear")
			.help("Clear screen before executing command")
			.short("c")
			.long("clear"))
		.arg(Arg::with_name("on-busy-update")
			.help("Select the behaviour to use when receiving events while the command is running. Current default is queue, will change to do-nothing in 2.0.")
			.takes_value(true)
			.possible_values(&["do-nothing", "queue", "restart", "signal"])
			.long("on-busy-update"))
		.arg(Arg::with_name("restart")
			.help("Restart the process if it's still running. Shorthand for --on-busy-update=restart")
			.short("r")
			.long("restart"))
		.arg(Arg::with_name("signal")
			.help("Specify the signal to send when using --on-busy-update=signal")
			.short("s")
			.long("signal")
			.takes_value(true)
			.value_name("signal")
			.default_value("SIGTERM")
			.hidden(cfg!(windows)))
		.arg(Arg::with_name("kill")
			.hidden(true)
			.short("k")
			.long("kill"))
		.arg(Arg::with_name("debounce")
			.help("Set the timeout between detected change and command execution, defaults to 100ms")
			.takes_value(true)
			.value_name("milliseconds")
			.short("d")
			.long("debounce"))
		.arg(Arg::with_name("verbose")
			.help("Print debugging messages (-v, -vv, -vvv; use -vvv for bug reports)")
			.multiple(true)
			.short("v")
			.long("verbose"))
		.arg(Arg::with_name("print-events")
			.help("Print events that trigger actions")
			.long("print-events")
			.alias("changes-only")) // --changes-only is deprecated (remove at v2)
		.arg(Arg::with_name("filter") // TODO
			.help("Ignore all modifications except those matching the pattern")
			.short("f")
			.long("filter")
			.number_of_values(1)
			.multiple(true)
			.takes_value(true)
			.value_name("pattern"))
		.arg(Arg::with_name("ignore") // TODO
			.help("Ignore modifications to paths matching the pattern")
			.short("i")
			.long("ignore")
			.number_of_values(1)
			.multiple(true)
			.takes_value(true)
			.value_name("pattern"))
		.arg(Arg::with_name("no-vcs-ignore") // TODO
			.help("Skip auto-loading of .gitignore files for filtering")
			.long("no-vcs-ignore"))
		.arg(Arg::with_name("no-ignore") // TODO
			.help("Skip auto-loading of ignore files (.gitignore, .ignore, etc.) for filtering")
			.long("no-ignore"))
		.arg(Arg::with_name("no-default-ignore") // TODO
			.help("Skip auto-ignoring of commonly ignored globs")
			.long("no-default-ignore"))
		.arg(Arg::with_name("postpone")
			.help("Wait until first change to execute command")
			.short("p")
			.long("postpone"))
		.arg(Arg::with_name("poll")
			.help("Force polling mode (interval in milliseconds)")
			.long("force-poll")
			.value_name("interval"))
		.arg(Arg::with_name("shell")
			.help(if cfg!(windows) {
				"Use a different shell, or `none`. Try --shell=powershell, which will become the default in 2.0."
			} else {
			"Use a different shell, or `none`. E.g. --shell=bash"
			})
			.takes_value(true)
			.long("shell"))
		// -n short form will not be removed, and instead become a shorthand for --shell=none
		.arg(Arg::with_name("no-shell")
			.help("Do not wrap command in a shell. Deprecated: use --shell=none instead.")
			.short("n")
			.long("no-shell"))
		.arg(Arg::with_name("no-meta") // TODO
			.help("Ignore metadata changes")
			.long("no-meta"))
		.arg(Arg::with_name("no-environment") // TODO
			.help("Do not set WATCHEXEC_*_PATH environment variables for the command")
			.long("no-environment"))
		.arg(Arg::with_name("no-process-group") // TODO
			.help("Do not use a process group when running the command")
			.long("no-process-group"))
		.arg(Arg::with_name("once").short("1").hidden(true))
		.arg(Arg::with_name("watch-when-idle")
			.help("Deprecated alias for --on-busy-update=do-nothing, which will become the default in 2.0.")
			.short("W")
			.long("watch-when-idle"))
		.arg(Arg::with_name("notif") // TODO
			.help("Send a desktop notification when watchexec notices a change (experimental, behaviour may change)")
			.short("N")
			.long("notify"));

	let mut raw_args: Vec<OsString> = env::args_os().collect();

	if let Some(first) = raw_args.get(1).and_then(|s| s.to_str()) {
		if let Some(arg_path) = first.strip_prefix('@').map(Path::new) {
			let arg_file = BufReader::new(
				File::open(arg_path)
					.wrap_err_with(|| format!("Failed to open argument file {:?}", arg_path))?,
			);

			let mut more_args: Vec<OsString> = arg_file
				.lines()
				.map(|l| l.map(OsString::from).map_err(Report::from))
				.collect::<Result<_>>()?;

			more_args.insert(0, raw_args.remove(0));
			more_args.extend(raw_args.into_iter().skip(1));
			raw_args = more_args;
		}
	}

	Ok(app.get_matches_from(raw_args))
}
