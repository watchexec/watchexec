use crate::cli::{clear_screen};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::gitignore;
use crate::ignore;
use crate::notification_filter::NotificationFilter;
use crate::pathop::PathOp;
use crate::process::{self, Process};
use crate::signal::{self, Signal};
use crate::watcher::{Event, Watcher};
use std::{
    collections::HashMap,
    fs::canonicalize,
    sync::{
        mpsc::{channel, Receiver},
        Arc, RwLock,
    },
    time::Duration,
};

pub trait Handler {
    /// Called through a manual request, such as an initial run.
    ///
    /// # Returns
    ///
    /// A `Result` which means:
    ///
    /// - `Err`: an error has occurred while processing, quit.
    /// - `Ok(true)`: everything is fine and the loop can continue.
    /// - `Ok(false)`: everything is fine but we should gracefully stop.
    fn on_manual(&self) -> Result<bool>;

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
    fn on_update(&self, ops: &[PathOp]) -> Result<bool>;

    /// Called once by `watch` at the very start.
    ///
    /// Not called again; any changes will never be picked up.
    ///
    /// The `Config` instance should be created using `ConfigBuilder` rather than direct initialisation
    /// to resist potential breaking changes (see semver policy on crate root).
    fn args(&self) -> Config;
}

/// Starts watching, and calls a handler when something happens.
///
/// Given an argument structure and a `Handler` type, starts the watcher loop, blocking until done.
pub fn watch<H>(handler: &H) -> Result<()>
where
    H: Handler,
{
    let args = handler.args();

    let mut paths = vec![];
    for path in &args.paths {
        paths.push(
            canonicalize(&path)
                .map_err(|e| Error::Canonicalization(path.to_string_lossy().into_owned(), e))?,
        );
    }

    let ignore = ignore::load(if args.no_ignore { &[] } else { &paths });
    let gitignore = gitignore::load(if args.no_vcs_ignore || args.no_ignore {
        &[]
    } else {
        &paths
    });
    let filter = NotificationFilter::new(&args.filters, &args.ignores, gitignore, ignore)?;

    let (tx, rx) = channel();
    let poll = args.poll;
    #[cfg(target_os = "linux")]
    let poll_interval = args.poll_interval;
    #[allow(clippy::redundant_clone)]
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
    if args.run_initially && !handler.on_manual()? {
        return Ok(());
    }

    loop {
        debug!("Waiting for filesystem activity");
        let paths = wait_fs(&rx, &filter, args.debounce, args.no_meta);
        info!("Paths updated: {:?}", paths);

        if !handler.on_update(&paths)? {
            break;
        }
    }

    Ok(())
}

pub struct ExecHandler {
    args: Config,
    signal: Option<Signal>,
    child_process: Arc<RwLock<Option<Process>>>,
}

impl ExecHandler {
    pub fn new(args: Config) -> Result<Self> {
        let child_process: Arc<RwLock<Option<Process>>> = Arc::new(RwLock::new(None));
        let weak_child = Arc::downgrade(&child_process);

        // Convert signal string to the corresponding integer
        let signal = signal::new(args.signal.clone());

        signal::install_handler(move |sig: Signal| {
            if let Some(lock) = weak_child.upgrade() {
                let strong = lock.read().expect("poisoned lock in install_handler");
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

    fn spawn(&self, ops: &[PathOp]) -> Result<()> {
        if self.args.clear_screen {
            clear_screen();
        }

        debug!("Launching command");
        let mut guard = self.child_process.write()?;
        *guard = Some(process::spawn(
            &self.args.cmd,
            ops,
            self.args.shell.clone(),
            !self.args.no_environment,
        )?);

        Ok(())
    }

    pub fn has_running_process(&self) -> bool {
        let guard = self
            .child_process
            .read()
            .expect("poisoned lock in has_running_process");

        (*guard).is_some()
    }
}

impl Handler for ExecHandler {
    fn args(&self) -> Config {
        self.args.clone()
    }

    // Only returns Err() on lock poisoning.
    fn on_manual(&self) -> Result<bool> {
        if self.args.once {
            return Ok(true);
        }

        self.spawn(&[])?;
        Ok(true)
    }

    // Only returns Err() on lock poisoning.
    fn on_update(&self, ops: &[PathOp]) -> Result<bool> {
        match (
            self.has_running_process(),
            self.args.restart, // kill the command if it's still running when a change comes in
            self.signal,
            self.args.watch_when_idle, // ignore events emitted while the command is running
        ) {
            // Spawn a process when there are no running processes in the background
            (false, _, _, _) => {
                self.spawn(ops)?;
            }

            // SIGHUP scenario: --signal was given, but --restart was not
            // Just send a signal (e.g. SIGHUP) to the command, do nothing more
            (true, false, Some(signal), _) => signal_process(&self.child_process, signal),

            // Custom restart behaviour (--restart was given, and --signal specified):
            // Send specified signal to the command, wait for it to exit, then run the command again
            (_, true, Some(signal), false) => {
                signal_process(&self.child_process, signal);
                wait_on_process(&self.child_process);
                self.spawn(ops)?;
            }

            // Default restart behaviour (--restart was given, but --signal wasn't specified):
            // Send SIGTERM to the command, wait for it to exit, then run the command again
            (_, true, None, false) => {
                signal_process(&self.child_process, Signal::SIGTERM);
                wait_on_process(&self.child_process);
                self.spawn(ops)?;
            }

            // Default behaviour (neither --signal nor --restart specified):
            // Make sure the previous run was ended, then run the command again
            (_, false, None, false) => {
                wait_on_process(&self.child_process);
                self.spawn(ops)?;
            }

            // Command is running and we're ignoring updates while it's running
            (true, _, _, true) => {}
        }

        // Handle once option for integration testing
        if self.args.once {
            if let Some(signal) = self.signal {
                signal_process(&self.child_process, signal);
            }

            wait_on_process(&self.child_process);

            return Ok(false);
        }

        Ok(true)
    }
}

pub fn run(args: Config) -> Result<()> {
    watch(&ExecHandler::new(args)?)
}

fn wait_fs(
    rx: &Receiver<Event>,
    filter: &NotificationFilter,
    debounce: u64,
    no_meta: bool,
) -> Vec<PathOp> {
    let mut paths = Vec::new();
    let mut cache = HashMap::new();

    loop {
        let e = rx.recv().expect("error when reading event");

        if let Some(ref path) = e.path {
            let pathop = PathOp::new(path, e.op.ok(), e.cookie);
            if let Some(op) = pathop.op {
                if no_meta && PathOp::is_meta(op) {
                    continue;
                }
            }

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

fn signal_process(process: &RwLock<Option<Process>>, signal: Signal) {
    let guard = process.read().expect("poisoned lock in signal_process");

    if let Some(ref child) = *guard {
        debug!("Signaling process with {}", signal);
        child.signal(signal);
    }
}

fn wait_on_process(process: &RwLock<Option<Process>>) {
    let guard = process.read().expect("poisoned lock in wait_on_process");

    if let Some(ref child) = *guard {
        debug!("Waiting for process to exit...");
        child.wait();
    }
}
