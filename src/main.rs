#![feature(mpsc_select)]
#![feature(process_exec)]

#[macro_use]
extern crate clap;
extern crate env_logger;
extern crate libc;
#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;
extern crate notify;
extern crate threadpool;

#[cfg(unix)]
extern crate nix;
#[cfg(windows)]
extern crate winapi;
#[cfg(windows)]
extern crate kernel32;

#[cfg(test)]
extern crate mktemp;

mod cli;
mod gitignore;
mod interrupt_handler;
mod notification_filter;
mod process;
mod watcher;

use std::env;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;
use std::path::{Path, PathBuf};

use notification_filter::NotificationFilter;
use process::{Process, ProcessReaper};
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

    log_builder.format(|r| format!("*** {}", r.args()))
        .filter(None, level);
    log_builder.init().expect("unable to initialize logger");
}

fn main() {
    let interrupt_rx = interrupt_handler::install();
    let args = cli::get_args();

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

    let mut filter = NotificationFilter::new(&cwd, gitignore_file)
        .expect("unable to create notification filter");

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
            Ok(canonicalized) => watcher.watch(canonicalized).expect("unable to watch path"),
            Err(_) => {
                println!("invalid path: {}", path);
                return;
            }
        }
    }

    let cmd = args.cmd;

    let (child_finish_tx, child_finish_rx) = channel();
    let reaper = ProcessReaper::new(child_finish_tx);

    let mut child_process = if args.run_initially {
        if args.clear_screen {
            cli::clear_screen();
        }

        Process::new(&cmd, vec![])
    } else { None };

    while !interrupt_handler::interrupt_requested() {
        if let Some(paths) = wait(&rx, &interrupt_rx, &filter) {
            let updated = paths.iter()
                .map(|p| p.to_str().unwrap())
                .collect();

            if let Some(mut child) = child_process {
                if args.restart {
                    debug!("Killing child process");
                    child.kill();
                }

                debug!("Waiting for process to exit...");
                reaper.wait_process(child);
                select! {
                    _ = child_finish_rx.recv() => {},
                    _ = interrupt_rx.recv() => break
                };
            }

            if args.clear_screen {
                cli::clear_screen();
            }

            child_process = Process::new(&cmd, updated);
        }
    }
}

fn wait(rx: &Receiver<Event>,
        interrupt_rx: &Receiver<()>,
        filter: &NotificationFilter)
        -> Option<Vec<PathBuf>> {
    let mut paths = vec![];

    loop {
        select! {
            _ = interrupt_rx.recv() => { return None; },
            ev = rx.recv() => {
                let e = ev.expect("error when reading event");

                if let Some(ref path) = e.path {
                    if !filter.is_excluded(path) {
                        paths.push(path.to_owned());
                        break;
                    }
                }
            }
        };
    }

    // Wait for filesystem activity to cool off
    // Unfortunately, we can't use select! with recv_timeout :(
    let timeout = Duration::from_millis(500);
    while let Ok(e) = rx.recv_timeout(timeout) {
        if interrupt_handler::interrupt_requested() {
            break;
        }

        if let Some(ref path) = e.path {
            paths.push(path.to_owned());
        }
    }

    Some(paths)
}
