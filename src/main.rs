extern crate clap;
extern crate libc;
extern crate notify;

mod filter;

use std::ffi::CString;
use std::sync::mpsc::{channel, Receiver, RecvError};
use std::{env, thread, time};
use std::process::Command;

use clap::{App, Arg};
use libc::system;
use notify::{Event, RecommendedWatcher, Watcher};

use filter::Filter;

fn clear() {
    // TODO: determine better way to do this
    let clear_cmd;
    if cfg!(target_os = "windows") {
        clear_cmd = "cls";
    }
    else {
        clear_cmd = "clear";
    }

    let _ = Command::new(clear_cmd).status();
}

fn invoke(cmd: &str) {
    // TODO: determine a better way to get at system()
    let s = CString::new(cmd.clone()).unwrap();
    unsafe {
      system(s.as_ptr());
    }
}

fn wait(rx: &Receiver<Event>, filter: &Filter, debug: bool) -> Result<Event, RecvError> {
    loop {
        // Block on initial notification
        let e = try!(rx.recv());
        if let Some(ref path) = e.path {
            if filter.is_excluded(&path) {
                if debug {
                    println!("*** Ignoring {} due to filter", path.to_str().unwrap());
                }
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

fn main() {
    let args = App::new("watchexec")
        .version("0.9")
        .about("Runs a command when any of the specified files/directories are modified")
        .arg(Arg::with_name("path")
            .help("Path(s) to watch for changes")
            .short("w")
            .long("watch")
            .takes_value(true)
            .default_value("."))
        .arg(Arg::with_name("command")
            .help("Command to run")
            .multiple(true)
            .required(true))
        .arg(Arg::with_name("extensions")
             .help("Comma-separated list of file extensions to watch (js,css,html)")
             .short("e")
             .long("exts")
             .takes_value(true))
        .arg(Arg::with_name("clear")
            .help("Clear screen before running command")
            .short("c")
            .long("clear"))
        .arg(Arg::with_name("debug")
             .help("Enable debug messages")
             .short("d")
             .long("debug"))
        .arg(Arg::with_name("filter")
             .help("Ignore paths that do not match the pattern")
             .short("f")
             .long("filter")
             .number_of_values(1)
             .multiple(true)
             .takes_value(true)
             .value_name("pattern"))
        .arg(Arg::with_name("ignore")
             .help("Ignore events from paths matching a pattern")
             .short("i")
             .long("ignore")
             .number_of_values(1)
             .multiple(true)
             .takes_value(true)
             .value_name("pattern"))
        .get_matches();

    let debug = args.is_present("debug");

    let cwd = env::current_dir().unwrap();
    let mut filter = Filter::new(&cwd);

    // Ignore python bytecode and dotted directories by default
    let default_filters = vec!["*.pyc", ".*/*"];
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
        watcher.watch(path).expect("unable to watch path");
    }

    let cmd_parts: Vec<&str> = args.values_of("command").unwrap().collect();
    let cmd = cmd_parts.join(" ");

    loop {
        let e = wait(&rx, &filter, debug).expect("error when waiting for filesystem changes");

        if args.is_present("clear") {
            clear();
        }

        if debug {
            println!("*** {:?}: {:?}", e.op, e.path);
        }

        if debug {
            println!("*** Running: {}", cmd);
        }

        invoke(&cmd);
    }
}
