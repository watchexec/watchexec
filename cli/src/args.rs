use std::{
    env,
    ffi::OsString,
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf, MAIN_SEPARATOR},
    time::Duration,
};

use clap::{crate_version, value_t, values_t, App, Arg};
use color_eyre::eyre::{Context, Report, Result};
use log::LevelFilter;
use watchexec::{config::ConfigBuilder, run::OnBusyUpdate, Shell};

use crate::handler::CliHandler;

pub fn get_args() -> Result<CliHandler> {
    let app = App::new("watchexec")
        .version(crate_version!())
        .about("Execute commands when watched files change")
        .after_help("Use @argfile as first argument to load arguments from the file `argfile` (one argument per line) which will be inserted in place of the @argfile (further arguments on the CLI will override or add onto those in the file).")
        .arg(Arg::with_name("command")
                 .help("Command to execute")
                 .multiple(true)
                 .required(true))
        .arg(Arg::with_name("extensions")
                 .help("Comma-separated list of file extensions to watch (e.g. js,css,html)")
                 .short("e")
                 .long("exts")
                 .takes_value(true))
        .arg(Arg::with_name("path")
                 .help("Watch a specific file or directory")
                 .short("w")
                 .long("watch")
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
                 .help("Send signal to process upon changes, e.g. SIGHUP")
                 .short("s")
                 .long("signal")
                 .takes_value(true)
                 .number_of_values(1)
                 .value_name("signal"))
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
                 .help("Print debugging messages to stderr")
                 .short("v")
                 .long("verbose"))
        .arg(Arg::with_name("changes")
                 .help("Only print path change information. Overridden by --verbose")
                 .long("changes-only"))
        .arg(Arg::with_name("filter")
                 .help("Ignore all modifications except those matching the pattern")
                 .short("f")
                 .long("filter")
                 .number_of_values(1)
                 .multiple(true)
                 .takes_value(true)
                 .value_name("pattern"))
        .arg(Arg::with_name("ignore")
                 .help("Ignore modifications to paths matching the pattern")
                 .short("i")
                 .long("ignore")
                 .number_of_values(1)
                 .multiple(true)
                 .takes_value(true)
                 .value_name("pattern"))
        .arg(Arg::with_name("no-vcs-ignore")
                 .help("Skip auto-loading of .gitignore files for filtering")
                 .long("no-vcs-ignore"))
        .arg(Arg::with_name("no-ignore")
                 .help("Skip auto-loading of ignore files (.gitignore, .ignore, etc.) for filtering")
                 .long("no-ignore"))
        .arg(Arg::with_name("no-default-ignore")
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
        .arg(Arg::with_name("no-meta")
                 .help("Ignore metadata changes")
                 .long("no-meta"))
        .arg(Arg::with_name("no-environment")
                 .help("Do not set WATCHEXEC_*_PATH environment variables for the command")
                 .long("no-environment"))
        .arg(Arg::with_name("no-process-group")
                 .help("Do not use a process group when running the command")
                 .long("no-process-group"))
        .arg(Arg::with_name("once").short("1").hidden(true))
        .arg(Arg::with_name("watch-when-idle")
                 .help("Deprecated alias for --on-busy-update=do-nothing, which will become the default in 2.0.")
                 .short("W")
                 .long("watch-when-idle"))
        .arg(Arg::with_name("notif")
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

    let args = app.get_matches_from(raw_args);
    let mut builder = ConfigBuilder::default();

    let cmd: Vec<String> = values_t!(args.values_of("command"), String)?;
    builder.cmd(cmd);

    let paths: Vec<PathBuf> = values_t!(args.values_of("path"), String)
        .unwrap_or_else(|_| vec![".".into()])
        .iter()
        .map(|string_path| string_path.into())
        .collect();
    builder.paths(paths);

    // Treat --kill as --signal SIGKILL (for compatibility with deprecated syntax)
    if args.is_present("kill") {
        builder.signal("SIGKILL");
    }

    if let Some(signal) = args.value_of("signal") {
        builder.signal(signal);
    }

    let mut filters = values_t!(args.values_of("filter"), String).unwrap_or_else(|_| Vec::new());
    if let Some(extensions) = args.values_of("extensions") {
        for exts in extensions {
            // TODO: refactor with flatten()
            filters.extend(exts.split(',').filter_map(|ext| {
                if ext.is_empty() {
                    None
                } else {
                    Some(format!("*.{}", ext.replace(".", "")))
                }
            }));
        }
    }

    builder.filters(filters);

    let mut ignores = vec![];
    let default_ignores = vec![
        format!("**{}.DS_Store", MAIN_SEPARATOR),
        String::from("*.py[co]"),
        String::from("#*#"),
        String::from(".#*"),
        String::from(".*.kate-swp"),
        String::from(".*.sw?"),
        String::from(".*.sw?x"),
        format!("**{}.git{}**", MAIN_SEPARATOR, MAIN_SEPARATOR),
        format!("**{}.hg{}**", MAIN_SEPARATOR, MAIN_SEPARATOR),
        format!("**{}.svn{}**", MAIN_SEPARATOR, MAIN_SEPARATOR),
    ];

    if args.occurrences_of("no-default-ignore") == 0 {
        ignores.extend(default_ignores)
    };
    ignores.extend(values_t!(args.values_of("ignore"), String).unwrap_or_else(|_| Vec::new()));

    builder.ignores(ignores);

    if args.occurrences_of("poll") > 0 {
        builder.poll_interval(Duration::from_millis(
            value_t!(args.value_of("poll"), u64).unwrap_or_else(|e| e.exit()),
        ));
    }

    if args.occurrences_of("debounce") > 0 {
        builder.debounce(Duration::from_millis(
            value_t!(args.value_of("debounce"), u64).unwrap_or_else(|e| e.exit()),
        ));
    }

    builder.on_busy_update(if args.is_present("restart") {
        OnBusyUpdate::Restart
    } else if args.is_present("watch-when-idle") {
        OnBusyUpdate::DoNothing
    } else if let Some(s) = args.value_of("on-busy-update") {
        match s.as_bytes() {
            b"do-nothing" => OnBusyUpdate::DoNothing,
            b"queue" => OnBusyUpdate::Queue,
            b"restart" => OnBusyUpdate::Restart,
            b"signal" => OnBusyUpdate::Signal,
            _ => unreachable!("clap restricts on-busy-updates values"),
        }
    } else {
        // will become DoNothing in v2.0
        OnBusyUpdate::Queue
    });

    builder.shell(if args.is_present("no-shell") {
        Shell::None
    } else if let Some(s) = args.value_of("shell") {
        if s.eq_ignore_ascii_case("powershell") {
            Shell::Powershell
        } else if s.eq_ignore_ascii_case("none") {
            Shell::None
        } else if s.eq_ignore_ascii_case("cmd") {
            cmd_shell(s.into())
        } else {
            Shell::Unix(s.into())
        }
    } else {
        default_shell()
    });

    builder.clear_screen(args.is_present("clear"));
    builder.run_initially(!args.is_present("postpone"));
    builder.no_meta(args.is_present("no-meta"));
    builder.no_environment(args.is_present("no-environment"));
    builder.no_vcs_ignore(args.is_present("no-vcs-ignore"));
    builder.no_ignore(args.is_present("no-ignore"));
    builder.poll(args.occurrences_of("poll") > 0);
    builder.use_process_group(!args.is_present("no-process-group"));

    let mut config = builder.build()?;
    if args.is_present("once") {
        config.once = true;
    }

    let loglevel = if args.is_present("verbose") {
        LevelFilter::Debug
    } else if args.is_present("changes") {
        LevelFilter::Info
    } else {
        LevelFilter::Warn
    };

    CliHandler::new(config, loglevel, args.is_present("notif"))
}

// until 2.0
#[cfg(windows)]
fn default_shell() -> Shell {
    Shell::Cmd
}

#[cfg(not(windows))]
fn default_shell() -> Shell {
    Shell::default()
}

// because Shell::Cmd is only on windows
#[cfg(windows)]
fn cmd_shell(_: String) -> Shell {
    Shell::Cmd
}

#[cfg(not(windows))]
fn cmd_shell(s: String) -> Shell {
    Shell::Unix(s)
}
