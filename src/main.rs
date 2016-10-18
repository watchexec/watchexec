#[macro_use]
extern crate clap;
extern crate env_logger;
#[macro_use]
extern crate log;
extern crate notify;

mod gitignore;
mod notification_filter;
mod runner;

use std::sync::mpsc::{channel, Receiver, RecvError};
use std::{env, thread, time};
use std::path::Path;

use clap::{App, Arg, ArgMatches};
use notify::{Event, RecommendedWatcher, Watcher};

use notification_filter::NotificationFilter;
use runner::Runner;

fn get_args<'a>() -> ArgMatches<'a> {
    App::new("watchexec")
        .version(crate_version!())
        .about("Execute commands when watched files change")
        .arg(Arg::with_name("path")
            .help("Path to watch")
            .short("w")
            .long("watch")
            .number_of_values(1)
            .multiple(true)
            .takes_value(true)
            .default_value("."))
        .arg(Arg::with_name("command")
            .help("Command to execute")
            .multiple(true)
            .required(true))
        .arg(Arg::with_name("extensions")
             .help("Comma-separated list of file extensions to watch (js,css,html)")
             .short("e")
             .long("exts")
             .takes_value(true))
        .arg(Arg::with_name("clear")
            .help("Clear screen before executing command")
            .short("c")
            .long("clear"))
        .arg(Arg::with_name("restart")
             .help("Restart the process if it's still running")
             .short("r")
             .long("restart"))
        .arg(Arg::with_name("debug")
             .help("Print debugging messages to stderr")
             .short("d")
             .long("debug"))
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
        .arg(Arg::with_name("run-initially")
             .help("Run command initially, before first file change")
             .long("run-initially"))
        .get_matches()
}

fn init_logger(debug: bool) {
    let mut log_builder = env_logger::LogBuilder::new();
    let mut level = log::LogLevelFilter::Warn;
    if debug {
        level = log::LogLevelFilter::Debug;
    }

    log_builder
        .format(|r| format!("*** {}", r.args()))
        .filter(None, level);
    log_builder.init().expect("unable to initialize logger");
}

fn main() {
    let args = get_args();

    init_logger(args.is_present("debug"));

    let cwd = env::current_dir()
        .expect("unable to get cwd")
        .canonicalize()
        .expect("unable to canonicalize cwd");

    let mut gitignore_file = None;
    if !args.is_present("no-vcs-ignore") {
        let gitignore_path = cwd.join(".gitignore");
        if gitignore_path.exists() {
            debug!("Found .gitignore file: {:?}", gitignore_path);

            gitignore_file = gitignore::parse(&gitignore_path).ok();
        }
    }

    let mut filter = NotificationFilter::new(&cwd, gitignore_file).expect("unable to create notification filter");

    // Add default ignore list
    let dotted_dirs = Path::new(".*").join("*");
    let default_filters = vec!["*/.DS_Store", "*.pyc", "*.swp", dotted_dirs.to_str().unwrap()];
    for p in default_filters {
        filter.add_ignore(p).expect("bad default filter");
    }

    if let Some(extensions) = args.values_of("extensions") {
        for ext in extensions {
            filter.add_extension(ext).expect("bad extension");
        }
    }

    if let Some(filters) = args.values_of("filter") {
        for p in filters {
            filter.add_filter(p).expect("bad filter");
        }
    }

    if let Some(ignores) = args.values_of("ignore") {
        for i in ignores {
            filter.add_ignore(i).expect("bad ignore pattern");
        }
    }

    let (tx, rx) = channel();
    let mut watcher: RecommendedWatcher = Watcher::new(tx).expect("unable to create watcher");

    let paths = args.values_of("path").unwrap();
    for path in paths {
        match Path::new(path).canonicalize() {
            Ok(canonicalized)   => watcher.watch(canonicalized).expect("unable to watch path"),
            Err(_)              => {
                println!("invalid path: {}", path);
                return;
            }
        }
    }

    let cmd_parts: Vec<&str> = args.values_of("command").unwrap().collect();
    let cmd = cmd_parts.join(" ");
    let mut runner = Runner::new(args.is_present("restart"), args.is_present("clear"));

    loop {
        if args.is_present("run-initially") {
            runner.run_command(&cmd);
        }
        let e = wait(&rx, &filter).expect("error when waiting for filesystem changes");

        debug!("{:?}: {:?}", e.op, e.path);

        runner.run_command(&cmd);
    }
}

fn wait(rx: &Receiver<Event>, filter: &NotificationFilter) -> Result<Event, RecvError> {
    loop {
        // Block on initial notification
        let e = try!(rx.recv());
        if let Some(ref path) = e.path {
            if filter.is_excluded(&path) {
                continue;
            }
        }

        // Accumulate subsequent events
        thread::sleep(time::Duration::from_millis(250));

        // Drain rx buffer and drop them
        loop {
            match rx.try_recv() {
                Ok(_) => continue,
                Err(_) => break,
            }
        }

        return Ok(e);
    }
}
