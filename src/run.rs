use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

use cli;
use env_logger;
use gitignore;
use log;
use notification_filter::NotificationFilter;
use process::{self, Process};
use signal::{self, Signal};
use watcher::{Event, Watcher};
use pathop::PathOp;

type Result<T> = ::std::result::Result<T, Box<::std::error::Error>>;

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

pub fn run(args: cli::Args) -> Result<()> {
    let child_process: Arc<RwLock<Option<Process>>> = Arc::new(RwLock::new(None));
    let weak_child = Arc::downgrade(&child_process);

    // Convert signal string to the corresponding integer
    let signal = signal::new(args.signal);

    signal::install_handler(move |sig: Signal| {
        if let Some(lock) = weak_child.upgrade() {
            let strong = lock.read().unwrap();
            if let Some(ref child) = *strong {
                match sig {
                    Signal::SIGCHLD => child.reap(), // SIGCHLD is special, initiate reap()
                    _ => child.signal(sig),
                }
            }
        }
    });

    init_logger(args.debug);

    let paths: Result<Vec<PathBuf>> = args.paths
        .iter()
        .map(|p| {
                 Ok(Path::new(&p)
                     .canonicalize()
                     .map_err(|e| format!("Unable to canonicalize path: \"{}\", {}", p, e))?
                     .to_owned())
             })
        .collect();
    let paths = paths?;

    let gitignore = if !args.no_vcs_ignore {
        gitignore::load(&paths)
    } else {
        gitignore::load(&[])
    };

    let filter = NotificationFilter::new(args.filters, args.ignores, gitignore)
        .expect("unable to create notification filter");

    let (tx, rx) = channel();
    let watcher =
        Watcher::new(tx, &paths, args.poll, args.poll_interval).expect("unable to create watcher");

    if watcher.is_polling() {
        warn!("Polling for changes every {} ms", args.poll_interval);
    }

    // Start child process initially, if necessary
    if args.run_initially && !args.once {
        if args.clear_screen {
            cli::clear_screen();
        }

        let mut guard = child_process.write().unwrap();
        *guard = Some(process::spawn(&args.cmd, vec![], args.no_shell));
    }

    loop {
        debug!("Waiting for filesystem activity");
        let paths = wait_fs(&rx, &filter, args.debounce);
        if let Some(path) = paths.get(0) {
            debug!("Path updated: {:?}", path);
        }

        // We have three scenarios here:
        //
        // 1. Make sure the previous run was ended, then run the command again
        // 2. Just send a specified signal to the child, do nothing more
        // 3. Send SIGTERM to the child, wait for it to exit, then run the command again
        // 4. Send a specified signal to the child, wait for it to exit, then run the command again
        //
        let scenario = (args.restart, signal.is_some());

        match scenario {
            // Custom restart behaviour (--restart was given, and --signal specified):
            // Send specified signal to the child, wait for it to exit, then run the command again
            (true, true) => {
                signal_process(&child_process, signal, true);

                // Launch child process
                if args.clear_screen {
                    cli::clear_screen();
                }

                debug!("Launching child process");
                {
                    let mut guard = child_process.write().unwrap();
                    *guard = Some(process::spawn(&args.cmd, paths, args.no_shell));
                }
            }

            // Default restart behaviour (--restart was given, but --signal wasn't specified):
            // Send SIGTERM to the child, wait for it to exit, then run the command again
            (true, false) => {
                let sigterm = signal::new(Some("SIGTERM".to_owned()));
                signal_process(&child_process, sigterm, true);

                // Launch child process
                if args.clear_screen {
                    cli::clear_screen();
                }

                debug!("Launching child process");
                {
                    let mut guard = child_process.write().unwrap();
                    *guard = Some(process::spawn(&args.cmd, paths, args.no_shell));
                }
            }

            // SIGHUP scenario: --signal was given, but --restart was not
            // Just send a signal (e.g. SIGHUP) to the child, do nothing more
            (false, true) => signal_process(&child_process, signal, false),

            // Default behaviour (neither --signal nor --restart specified):
            // Make sure the previous run was ended, then run the command again
            (false, false) => {
                signal_process(&child_process, None, true);

                // Launch child process
                if args.clear_screen {
                    cli::clear_screen();
                }

                debug!("Launching child process");
                {
                    let mut guard = child_process.write().unwrap();
                    *guard = Some(process::spawn(&args.cmd, paths, args.no_shell));
                }
            }
        }

        // Handle once option for integration testing
        if args.once {
            signal_process(&child_process, signal, false);
            break;
        }
    }
    Ok(())
}

fn wait_fs(rx: &Receiver<Event>, filter: &NotificationFilter, debounce: u64) -> Vec<PathOp> {
    let mut paths = vec![];
    let mut cache = HashMap::new();

    loop {
        let e = rx.recv().expect("error when reading event");

        if let Some(ref path) = e.path {
            let pathop = PathOp::new(path, e.op.ok(), e.cookie);
            // Ignore cache for the initial file. Otherwise, in
            // debug mode it's hard to track what's going on
            let excluded = filter.is_excluded(path);
            if !cache.contains_key(&pathop) {
                cache.insert(pathop.clone(), excluded);
            }

            if !excluded {
                paths.push(pathop);
                break;
            }
        }
    }

    // Wait for filesystem activity to cool off
    let timeout = Duration::from_millis(debounce);
    while let Ok(e) = rx.recv_timeout(timeout) {
        if let Some(ref path) = e.path {
            let pathop = PathOp::new(path, e.op.ok(), e.cookie);
            if cache.contains_key(&pathop) {
                continue;
            }

            let excluded = filter.is_excluded(path);

            cache.insert(pathop.clone(), excluded);

            if !excluded {
                paths.push(pathop);
            }
        }
    }

    paths
}

// signal_process sends signal to process. It waits for the process to exit if wait is true
fn signal_process(process: &RwLock<Option<Process>>, signal: Option<Signal>, wait: bool) {
    let guard = process.read().unwrap();

    if let Some(ref child) = *guard {
        if let Some(s) = signal {
            child.signal(s);
        }

        if wait {
            debug!("Waiting for process to exit...");
            child.wait();
        }
    }
}
