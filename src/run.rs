use cli::{clear_screen, Args};
use env_logger;
use error::{Error, Result};
use gitignore;
use log;
use notification_filter::NotificationFilter;
#[cfg(target_os = "linux")]
use notify;
use pathop::PathOp;
use process::{self, Process};
use signal::{self, Signal};
use std::{
    collections::HashMap,
    fs::canonicalize,
    io::Write,
    sync::{
        mpsc::{channel, Receiver},
        Arc, RwLock,
    },
    time::Duration,
};
use watcher::{Event, Watcher};

fn init_logger(debug: bool) {
    let mut log_builder = env_logger::Builder::new();
    let level = if debug {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Warn
    };

    log_builder
        .format(|buf, r| writeln!(buf, "*** {}", r.args()))
        .filter(None, level)
        .init();
}

pub trait Handler {
    /// Initialises the `Handler` with a copy of the arguments.
    fn new(args: Args) -> Result<Self>
    where
        Self: Sized;

    /// Called through a manual request, such as an initial run.
    ///
    /// # Returns
    ///
    /// A `Result` which means:
    ///
    /// - `Err`: an error has occurred while processing, quit.
    /// - `Ok(true)`: everything is fine and the loop can continue.
    /// - `Ok(false)`: everything is fine but we should gracefully stop.
    fn on_manual(&mut self) -> Result<bool>;

    /// Called through a file-update request.
    ///
    /// # Parameters
    ///
    /// - `ops`: The list of events that triggered this update.
    ///
    /// # Returns
    ///
    /// A `Result` which means:
    ///
    /// - `Err`: an error has occurred while processing, quit.
    /// - `Ok(true)`: everything is fine and the loop can continue.
    /// - `Ok(false)`: everything is fine but we should gracefully stop.
    fn on_update(&mut self, ops: &[PathOp]) -> Result<bool>;
}

/// Starts watching, and calls a handler when something happens.
///
/// Given an argument structure and a `Handler` type, starts the watcher
/// loop (blocking until done).
pub fn watch<H>(args: Args) -> Result<()>
where
    H: Handler,
{
    init_logger(args.debug);
    let mut handler = H::new(args.clone())?;

    let mut paths = vec![];
    for path in args.paths {
        paths.push(
            canonicalize(&path)
                .map_err(|e| Error::Canonicalization(path.to_string_lossy().into_owned(), e))?,
        );
    }

    let gitignore = gitignore::load(if args.no_vcs_ignore { &[] } else { &paths });
    let filter = NotificationFilter::new(&args.filters, &args.ignores, gitignore)?;

    let (tx, rx) = channel();
    let poll = args.poll.clone();
    #[cfg(target_os = "linux")]
    let poll_interval = args.poll_interval.clone();
    let watcher = Watcher::new(tx.clone(), &paths, args.poll, args.poll_interval).or_else(|err| {
        if poll {
            return Err(err);
        }

        #[cfg(target_os = "linux")]
        {
            use nix::libc;
            let mut fallback = false;
            if let notify::Error::Io(ref e) = err {
                if e.raw_os_error() == Some(libc::ENOSPC) {
                    warn!("System notification limit is too small, falling back to polling mode. For better performance increase system limit:\n\tsysctl fs.inotify.max_user_watches=524288");
                    fallback = true;
                }
            }

            if fallback {
                return Watcher::new(tx, &paths, true, poll_interval);
            }
        }

        Err(err)
    })?;

    if watcher.is_polling() {
        warn!("Polling for changes every {} ms", args.poll_interval);
    }

    // Call handler initially, if necessary
    if args.run_initially {
        if !handler.on_manual()? {
            return Ok(());
        }
    }

    loop {
        debug!("Waiting for filesystem activity");
        let paths = wait_fs(&rx, &filter, args.debounce);
        debug!("Paths updated: {:?}", paths);

        if !handler.on_update(&paths)? {
            break;
        }
    }

    Ok(())
}

pub struct ExecHandler {
    args: Args,
    signal: Option<Signal>,
    child_process: Arc<RwLock<Option<Process>>>,
}

impl ExecHandler {
    fn spawn(&mut self, ops: &[PathOp]) -> Result<()> {
        if self.args.clear_screen {
            clear_screen();
        }

        debug!("Launching child process");
        let mut guard = self.child_process.write()?;
        *guard = Some(process::spawn(&self.args.cmd, ops, self.args.no_shell)?);

        Ok(())
    }
}

impl Handler for ExecHandler {
    fn new(args: Args) -> Result<Self> {
        let child_process: Arc<RwLock<Option<Process>>> = Arc::new(RwLock::new(None));
        let weak_child = Arc::downgrade(&child_process);

        // Convert signal string to the corresponding integer
        let signal = signal::new(args.signal.clone());

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

        Ok(Self {
            args,
            signal,
            child_process,
        })
    }

    // Only returns Err() on lock poisoning.
    fn on_manual(&mut self) -> Result<bool> {
        if self.args.once {
            return Ok(true);
        }

        self.spawn(&[])?;
        Ok(true)
    }

    // Only returns Err() on lock poisoning.
    fn on_update(&mut self, ops: &[PathOp]) -> Result<bool> {
        // We have four scenarios here:
        //
        // 1. Send a specified signal to the child, wait for it to exit, then run the command again
        // 2. Send SIGTERM to the child, wait for it to exit, then run the command again
        // 3. Just send a specified signal to the child, do nothing more
        // 4. Make sure the previous run was ended, then run the command again
        //
        let scenario = (self.args.restart, self.signal.is_some());

        match scenario {
            // Custom restart behaviour (--restart was given, and --signal specified):
            // Send specified signal to the child, wait for it to exit, then run the command again
            (true, true) => {
                signal_process(&self.child_process, self.signal, true);
                self.spawn(ops)?;
            }

            // Default restart behaviour (--restart was given, but --signal wasn't specified):
            // Send SIGTERM to the child, wait for it to exit, then run the command again
            (true, false) => {
                let sigterm = signal::new(Some("SIGTERM".into()));
                signal_process(&self.child_process, sigterm, true);
                self.spawn(ops)?;
            }

            // SIGHUP scenario: --signal was given, but --restart was not
            // Just send a signal (e.g. SIGHUP) to the child, do nothing more
            (false, true) => signal_process(&self.child_process, self.signal, false),

            // Default behaviour (neither --signal nor --restart specified):
            // Make sure the previous run was ended, then run the command again
            (false, false) => {
                signal_process(&self.child_process, None, true);
                self.spawn(ops)?;
            }
        }

        // Handle once option for integration testing
        if self.args.once {
            signal_process(&self.child_process, self.signal, false);
            return Ok(false);
        }

        Ok(true)
    }
}

pub fn run(args: Args) -> Result<()> {
    watch::<ExecHandler>(args)
}

fn wait_fs(rx: &Receiver<Event>, filter: &NotificationFilter, debounce: u64) -> Vec<PathOp> {
    let mut paths = Vec::new();
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
