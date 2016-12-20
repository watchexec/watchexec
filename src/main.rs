#[macro_use]
extern crate clap;
extern crate globset;
extern crate env_logger;
extern crate libc;
#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;
extern crate notify;

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
mod notification_filter;
mod process;
mod signal;
mod watcher;

use std::collections::HashMap;
use std::env;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;
use std::path::PathBuf;

use notification_filter::NotificationFilter;
use process::Process;
use signal::Signal;
use watcher::{Event, Watcher};

fn find_gitignore(path: &Path) -> Option<PathBuf> {
    let mut p = path.to_owned();

    loop {
        let gitignore_path = p.join(".gitignore");
        if gitignore_path.exists() {
            return Some(gitignore_path);
        }

        // Stop if we see a .git directory
        if let Ok(metadata) = p.join(".git").metadata() {
            if metadata.is_dir() {
                break;
            }
        }

        if p.parent().is_none() {
            break;
        }

        p.pop();
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
    let args = cli::get_args();
    let child_process: Arc<RwLock<Option<Process>>> = Arc::new(RwLock::new(None));
    let weak_child = Arc::downgrade(&child_process);
    let kill = args.kill;

    signal::install_handler(move |sig: Signal| {
        if let Some(lock) = weak_child.upgrade() {
            let strong = lock.read().unwrap();
            if let Some(ref child) = *strong {
                match sig {
                    Signal::Terminate => {
                        if kill {
                            child.kill();
                        } else {
                            child.terminate();
                        }
                    }
                    Signal::Stop => child.pause(),
                    Signal::Continue => child.resume(),
                    Signal::ChildExit => child.reap(),
                }
            }
        }
    });

    init_logger(args.debug);

    let cwd = env::current_dir()
        .expect("unable to get cwd")
        .canonicalize()
        .expect("unable to canonicalize cwd");

    let mut gitignore_file = None;
    if !args.no_vcs_ignore {
        if let Some(gitignore_path) = find_gitignore(&cwd) {
            debug!("Found .gitignore file: {:?}", gitignore_path);

            gitignore_file = gitignore::parse(&gitignore_path).ok();
        }
    }

    let filter = NotificationFilter::new(args.filters, args.ignores, gitignore_file)
        .expect("unable to create notification filter");

    let (tx, rx) = channel();
    let mut watcher = Watcher::new(tx, args.poll, args.poll_interval)
        .expect("unable to create watcher");

    if watcher.is_polling() {
        warn!("Polling for changes every {} ms", args.poll_interval);
    }

    let result = watcher.watch(cwd);
    if let Err(e) = result {
        error!("Unable to watch directory/subdirectory: {}", e);
        return;
    }

    // Start child process initially, if necessary
    if args.run_initially {
        if args.clear_screen {
            cli::clear_screen();
        }

        let mut guard = child_process.write().unwrap();
        *guard = Some(process::spawn(&args.cmd, vec![]));
    }

    loop {
        debug!("Waiting for filesystem activity");
        let paths = wait(&rx, &filter);
        if let Some(path) = paths.get(0) {
            debug!("Path updated: {:?}", path);
        }

        // Wait for current child process to exit
        {
            let guard = child_process.read().unwrap();

            if let Some(ref child) = *guard {
                if args.restart {
                    debug!("Stopping child process");
                    if kill {
                        child.kill();
                    } else {
                        child.terminate();
                    }
                }

                debug!("Waiting for process to exit...");
                child.wait();
            }
        }

        // Launch child process
        if args.clear_screen {
            cli::clear_screen();
        }

        debug!("Launching child process");
        {
            let mut guard = child_process.write().unwrap();
            *guard = Some(process::spawn(&args.cmd, paths));
        }
    }
}

fn wait(rx: &Receiver<Event>, filter: &NotificationFilter) -> Vec<PathBuf> {
    let mut paths = vec![];
    let mut cache = HashMap::new();

    loop {
        let e = rx.recv().expect("error when reading event");

        if let Some(ref path) = e.path {
            // Ignore cache for the initial file. Otherwise, in
            // debug mode it's hard to track what's going on
            let excluded = filter.is_excluded(path);
            if !cache.contains_key(path) {
                cache.insert(path.to_owned(), excluded);
            }

            if !excluded {
                paths.push(path.to_owned());
                break;
            }
        }
    }

    // Wait for filesystem activity to cool off
    let timeout = Duration::from_millis(500);
    while let Ok(e) = rx.recv_timeout(timeout) {
        if let Some(ref path) = e.path {
            if cache.contains_key(path) {
                continue;
            }

            let excluded = filter.is_excluded(path);

            let p = path.to_owned();
            cache.insert(p.clone(), excluded);

            if !excluded {
                paths.push(p);
            }
        }
    }

    paths
}
