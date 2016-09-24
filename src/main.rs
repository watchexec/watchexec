extern crate clap;
extern crate libc;
extern crate notify;

mod notification_filter;

use std::sync::mpsc::{channel, Receiver, RecvError};
use std::{env, io, thread, time};
use std::path::Path;
use std::process::{Child, Command};

use clap::{App, Arg};
use notify::{Event, RecommendedWatcher, Watcher};

use notification_filter::NotificationFilter;

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

fn invoke(cmd: &str) -> io::Result<Child> {
    if cfg!(target_os = "windows") {
        Command::new("cmd.exe")
                .arg("/C")
                .arg(cmd)
                .spawn()
    }
    else {
        Command::new("sh")
                .arg("-c")
                .arg(cmd)
                .spawn()
    }
}

fn wait(rx: &Receiver<Event>, filter: &NotificationFilter, verbose: bool) -> Result<Event, RecvError> {
    loop {
        // Block on initial notification
        let e = try!(rx.recv());
        if let Some(ref path) = e.path {
            if filter.is_excluded(&path) {
                if verbose {
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
        .version("0.10.0")
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
        .arg(Arg::with_name("verbose")
             .help("Prints diagnostic messages")
             .short("v")
             .long("verbose"))
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
        .get_matches();

    let verbose = args.is_present("verbose");

    let cwd = env::current_dir().unwrap();
    let mut filter = NotificationFilter::new(&cwd);

    // Add default ignore list
    let dotted_dirs = Path::new(".*").join("*");
    let default_filters = vec!["*.pyc", "*.swp", dotted_dirs.to_str().unwrap()];
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
    let restart = args.is_present("restart");
    let mut child_process: Option<Child> = None;

    loop {
        let e = wait(&rx, &filter, verbose).expect("error when waiting for filesystem changes");

        if let Some(mut child) = child_process {
            if restart {
                let _ = child.kill();
            }

            let _ = child.wait();
        }

        if args.is_present("clear") {
            clear();
        }

        if verbose {
            println!("*** {:?}: {:?}", e.op, e.path);
        }

        if verbose {
            println!("*** Executing: {}", cmd);
        }

        child_process = invoke(&cmd).ok();
    }
}
