#![feature(process_exec)]

#[macro_use] extern crate clap;
extern crate env_logger;
extern crate libc;
#[macro_use] extern crate log;
#[macro_use] extern crate lazy_static;
extern crate notify;

#[cfg(unix)] extern crate nix;
#[cfg(windows)] extern crate winapi;
#[cfg(windows)] extern crate kernel32;

mod args;
mod gitignore;
mod interrupt_handler;
mod notification_filter;
mod runner;
mod watcher;

use std::sync::mpsc::{channel, Receiver, RecvError};
use std::{env, thread, time};
use std::path::{Path, PathBuf};

use notification_filter::NotificationFilter;
use runner::Runner;
use watcher::{Event, Watcher};

// Starting at the specified path, search for gitignore files,
// stopping at the first one found.
fn find_gitignore_file(path: &Path) -> Option<PathBuf> {
    let mut gitignore_path = path.join(".gitignore");
    if gitignore_path.exists() {
        return Some(gitignore_path);
    }

    let p = path.to_owned();

    while let Some(p) = p.parent() {
        gitignore_path = p.join(".gitignore");
        if gitignore_path.exists() {
            return Some(gitignore_path);
        }
    }

    None
}

fn init_logger(debug: bool) {
    let mut log_builder = env_logger::LogBuilder::new();
    let level = if debug {
        log::LogLevelFilter::Debug
    } else {
		log::LogLevelFilter::Warn
    };

    log_builder
        .format(|r| format!("*** {}", r.args()))
        .filter(None, level);
    log_builder.init().expect("unable to initialize logger");
}

fn main() {
    let args = args::get_args();

    init_logger(args.debug);

    let cwd = env::current_dir()
        .expect("unable to get cwd")
        .canonicalize()
        .expect("unable to canonicalize cwd");

    let mut gitignore_file = None;
    if !args.no_vcs_ignore {
        if let Some(gitignore_path) = find_gitignore_file(&cwd) {
            debug!("Found .gitignore file: {:?}", gitignore_path);

            gitignore_file = gitignore::parse(&gitignore_path).ok();
        }
    }

    let mut filter = NotificationFilter::new(&cwd, gitignore_file).expect("unable to create notification filter");

    for f in args.filters {
        filter.add_filter(&f).expect("bad filter");
    }

    for i in args.ignores {
        filter.add_ignore(&i).expect("bad ignore pattern");
    }

    let (tx, rx) = channel();
    let mut watcher = Watcher::new(tx, args.poll, args.poll_interval)
        .expect("unable to create watcher");

    if watcher.is_polling() {
        warn!("Polling for changes every {} ms", args.poll_interval);
    }

    for path in args.paths {
        match Path::new(&path).canonicalize() {
            Ok(canonicalized)   => watcher.watch(canonicalized).expect("unable to watch path"),
            Err(_)              => {
                println!("invalid path: {}", path);
                return;
            }
        }
    }

    let cmd = args.cmd;
    let mut runner = Runner::new(args.restart, args.clear_screen);

    if args.run_initially {
        runner.run_command(&cmd, vec![]);
    }

    while !interrupt_handler::interrupt_requested() {
        let e = wait(&rx, &filter).expect("error when waiting for filesystem changes");

        debug!("{:?}: {:?}", e.op, e.path);

        // TODO: update wait to return all paths
        let updated: Vec<&str> = e.path
            .iter()
            .map(|p| p.to_str().unwrap())
            .collect();

        runner.run_command(&cmd, updated);
    }
}

fn wait(rx: &Receiver<Event>, filter: &NotificationFilter) -> Result<Event, RecvError> {
    loop {
        // Block on initial notification
        let e = try!(rx.recv());
        if let Some(ref path) = e.path {
            if filter.is_excluded(path) {
                continue;
            }
        }

        // Accumulate subsequent events
        thread::sleep(time::Duration::from_millis(250));

        // Drain rx buffer and drop them
        while let Ok(_) = rx.try_recv() {
			// nothing to do here
        }

        return Ok(e);
    }
}
