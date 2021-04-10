//! CLI arguments and library Args struct
//!
//! The [`Args`] struct is not constructable, use [`ArgsBuilder`].
//!
//! # Examples
//!
//! ```
//! # use watchexec::cli::ArgsBuilder;
//! ArgsBuilder::default()
//!     .cmd(vec!["echo hello world".into()])
//!     .paths(vec![".".into()])
//!     .build()
//!     .expect("mission failed");
//! ```

use crate::error;
use clap::{App, Arg, Error};
use std::{
    ffi::OsString,
    path::{PathBuf, MAIN_SEPARATOR},
    process::Command,
};

/// Arguments to the watcher
#[derive(Builder, Clone, Debug)]
#[builder(setter(into, strip_option))]
#[builder(build_fn(validate = "Self::validate"))]
#[non_exhaustive]
pub struct Args {
    /// Command to execute in popen3 format (first program, rest arguments).
    pub cmd: Vec<String>,
    /// List of paths to watch for changes.
    pub paths: Vec<PathBuf>,
    /// Positive filters (trigger only on matching changes). Glob format.
    #[builder(default)]
    pub filters: Vec<String>,
    /// Negative filters (do not trigger on matching changes). Glob format.
    #[builder(default)]
    pub ignores: Vec<String>,
    /// Clear the screen before each run.
    #[builder(default)]
    pub clear_screen: bool,
    /// If Some, send that signal (e.g. SIGHUP) to the child on change.
    #[builder(default)]
    pub signal: Option<String>,
    /// If true, kill the child if it's still running when a change comes in.
    #[builder(default)]
    pub restart: bool,
    /// Interval to debounce the changes. (milliseconds)
    #[builder(default = "500")]
    pub debounce: u64,
    /// Run the commands right after starting.
    #[builder(default = "true")]
    pub run_initially: bool,
    /// Do not wrap the commands in a shell.
    #[builder(default)]
    pub no_shell: bool,
    /// Ignore metadata changes.
    #[builder(default)]
    pub no_meta: bool,
    /// Do not set WATCHEXEC_*_PATH environment variables for child process.
    #[builder(default)]
    pub no_environment: bool,
    /// Skip auto-loading .gitignore files
    #[builder(default)]
    pub no_vcs_ignore: bool,
    /// Skip auto-loading .ignore files
    #[builder(default)]
    pub no_ignore: bool,
    /// For testing only, always set to false.
    #[builder(setter(skip))]
    #[builder(default)]
    #[doc(hidden)]
    pub once: bool,
    /// Force using the polling backend.
    #[builder(default)]
    pub poll: bool,
    /// Interval for polling. (milliseconds)
    #[builder(default = "1000")]
    pub poll_interval: u32,
    #[builder(default)]
    pub watch_when_idle: bool,
}

impl ArgsBuilder {
    fn validate(&self) -> Result<(), String> {
        if self.cmd.as_ref().map_or(true, Vec::is_empty) {
            return Err("cmd must not be empty".into());
        }

        if self.paths.as_ref().map_or(true, Vec::is_empty) {
            return Err("paths must not be empty".into());
        }

        Ok(())
    }

    #[deprecated(since = "1.15.0", note = "does nothing. set the log level instead")]
    pub fn debug(&mut self, _: impl Into<bool>) -> &mut Self {
        self
    }
}

/// Clear the screen.
#[cfg(target_family = "windows")]
pub fn clear_screen() {
// TODO: clearscreen with powershell?
    let _ = Command::new("cmd")
        .arg("/c")
        .arg("tput reset || cls")
        .status();
}

/// Clear the screen.
#[cfg(target_family = "unix")]
pub fn clear_screen() {
// TODO: clear screen via control codes instead
    let _ = Command::new("tput").arg("reset").status();
}

#[deprecated(since = "1.15.0", note = "this will be removed from the library API. use the builder")]
pub fn get_args() -> error::Result<Args> {
    get_args_impl(None::<&[&str]>)
}

#[deprecated(since = "1.15.0", note = "this will be removed from the library API. use the builder")]
pub fn get_args_from<I, T>(from: I) -> error::Result<Args>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    get_args_impl(Some(from))
}

fn get_args_impl<I, T>(from: Option<I>) -> error::Result<Args>
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
        .arg(Arg::with_name("no-shell")
                 .help("Do not wrap command in 'sh -c' resp. 'cmd.exe /C'")
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

    let cmd: Vec<String> = values_t!(args.values_of("command"), String)?;
    let paths = values_t!(args.values_of("path"), String)
        .unwrap_or_else(|_| vec![".".into()])
        .iter()
        .map(|string_path| string_path.into())
        .collect();

    // Treat --kill as --signal SIGKILL (for compatibility with older syntax)
    let signal = if args.is_present("kill") {
        Some("SIGKILL".to_string())
    } else {
        // Convert Option<&str> to Option<String>
        args.value_of("signal").map(str::to_string)
    };

    let mut filters = values_t!(args.values_of("filter"), String).unwrap_or_else(|_| Vec::new());

    if let Some(extensions) = args.values_of("extensions") {
        for exts in extensions {
            filters.extend(exts.split(',').filter_map(|ext| {
                if ext.is_empty() {
                    None
                } else {
                    Some(format!("*.{}", ext.replace(".", "")))
                }
            }));
        }
    }

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

    let poll_interval = if args.occurrences_of("poll") > 0 {
        value_t!(args.value_of("poll"), u32).unwrap_or_else(|e| e.exit())
    } else {
        1000
    };

    let debounce = if args.occurrences_of("debounce") > 0 {
        value_t!(args.value_of("debounce"), u64).unwrap_or_else(|e| e.exit())
    } else {
        500
    };

    if signal.is_some() && args.is_present("postpone") {
        // TODO: Error::argument_conflict() might be the better fit, usage was unclear, though
        Error::value_validation_auto("--postpone and --signal are mutually exclusive".to_string())
            .exit();
    }

    if signal.is_some() && args.is_present("kill") {
        // TODO: Error::argument_conflict() might be the better fit, usage was unclear, though
        Error::value_validation_auto("--kill and --signal is ambiguous.\n       Hint: Use only '--signal SIGKILL' without --kill".to_string())
            .exit();
    }

    Ok(Args {
        cmd,
        paths,
        filters,
        ignores,
        signal,
        clear_screen: args.is_present("clear"),
        restart: args.is_present("restart"),
        debounce,
        debug: args.is_present("verbose"),
        changes: args.is_present("changes"),
        run_initially: !args.is_present("postpone"),
        no_shell: args.is_present("no-shell"),
        no_meta: args.is_present("no-meta"),
        no_environment: args.is_present("no-environment"),
        no_vcs_ignore: args.is_present("no-vcs-ignore"),
        no_ignore: args.is_present("no-ignore"),
        once: args.is_present("once"),
        poll: args.occurrences_of("poll") > 0,
        poll_interval,
        watch_when_idle: args.is_present("watch-when-idle"),
    })
}
