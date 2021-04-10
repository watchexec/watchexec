use clap::{App, Arg, value_t, values_t, crate_version};
use log::LevelFilter;
use std::{
    ffi::OsString,
    path::{PathBuf, MAIN_SEPARATOR},
};

use crate::{error, Shell};
use crate::config::{Config, ConfigBuilder};

pub fn get_args() -> error::Result<(Config, LevelFilter)> {
    get_args_impl(None::<&[&str]>)
}

#[allow(dead_code)]
pub fn get_args_from<I, T>(from: I) -> error::Result<(Config, LevelFilter)>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    get_args_impl(Some(from))
}

fn get_args_impl<I, T>(from: Option<I>) -> error::Result<(Config, LevelFilter)>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let app = App::new("watchexec")
        .version(crate_version!())
        .about("Execute commands when watched files change")
        .arg(Arg::with_name("command")
                 .help("Command to execute")
                 .multiple(true)
                 .required(true))
        .arg(Arg::with_name("extensions")
                 .help("Comma-separated list of file extensions to watch (js,css,html)")
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
        .arg(Arg::with_name("restart")
                 .help("Restart the process if it's still running")
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
                 .help("Set the timeout between detected change and command execution, defaults to 500ms")
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
                 .help("Do not set WATCHEXEC_*_PATH environment variables for child process")
                 .long("no-environment"))
        .arg(Arg::with_name("once").short("1").hidden(true))
        .arg(Arg::with_name("watch-when-idle")
                 .help("Ignore events while the process is still running")
                 .short("W")
                 .long("watch-when-idle"));

    let args = match from {
        None => app.get_matches(),
        Some(i) => app.get_matches_from(i),
    };

    let mut builder = ConfigBuilder::default();

    let cmd: Vec<String> = values_t!(args.values_of("command"), String).map_err(|err| err.to_string())?;
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
        for exts in extensions { // TODO: refactor with flatten()
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
        builder.poll_interval(value_t!(args.value_of("poll"), u32).unwrap_or_else(|e| e.exit()));
    }

    if args.occurrences_of("debounce") > 0 {
        builder.debounce(value_t!(args.value_of("debounce"), u64).unwrap_or_else(|e| e.exit()));
    }

    let shell = if args.is_present("no-shell") {
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
    };

    builder.shell(shell);

    // TODO: check how postpone + signal behaves

    builder.clear_screen(args.is_present("clear"));
    builder.restart(args.is_present("restart"));
    builder.run_initially(!args.is_present("postpone"));
    builder.no_meta(args.is_present("no-meta"));
    builder.no_environment(args.is_present("no-environment"));
    builder.no_vcs_ignore(args.is_present("no-vcs-ignore"));
    builder.no_ignore(args.is_present("no-ignore"));
    builder.poll(args.occurrences_of("poll") > 0);
    builder.watch_when_idle(args.is_present("watch-when-idle"));

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

    Ok((config, loglevel))
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
