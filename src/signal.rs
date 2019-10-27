use std::sync::Mutex;

type CleanupFn = Box<dyn Fn(self::Signal) + Send>;
lazy_static! {
    static ref CLEANUP: Mutex<Option<CleanupFn>> = Mutex::new(None);
}

// Indicate interest in SIGCHLD by setting a dummy handler
#[cfg(unix)]
#[allow(clippy::missing_const_for_fn)]
pub extern "C" fn sigchld_handler(_: c_int) {}

#[cfg(unix)]
pub use nix::sys::signal::Signal;

// This is a dummy enum for Windows
#[cfg(windows)]
#[derive(Debug, Copy, Clone)]
pub enum Signal {
    SIGKILL,
    SIGTERM,
    SIGINT,
    SIGHUP,
    SIGSTOP,
    SIGCONT,
    SIGCHLD,
    SIGUSR1,
    SIGUSR2,
}

#[cfg(unix)]
use nix::libc::*;

#[cfg(unix)]
pub trait ConvertToLibc {
    fn convert_to_libc(self) -> c_int;
}

#[cfg(unix)]
impl ConvertToLibc for Signal {
    fn convert_to_libc(self) -> c_int {
        // Convert from signal::Signal enum to libc::* c_int constants
        match self {
            Self::SIGKILL => SIGKILL,
            Self::SIGTERM => SIGTERM,
            Self::SIGINT => SIGINT,
            Self::SIGHUP => SIGHUP,
            Self::SIGSTOP => SIGSTOP,
            Self::SIGCONT => SIGCONT,
            Self::SIGCHLD => SIGCHLD,
            Self::SIGUSR1 => SIGUSR1,
            Self::SIGUSR2 => SIGUSR2,
            _ => panic!("unsupported signal: {:?}", self),
        }
    }
}

pub fn new(signal_name: Option<String>) -> Option<Signal> {
    if let Some(signame) = signal_name {
        let signal = match signame.as_ref() {
            "SIGKILL" | "KILL" => Signal::SIGKILL,
            "SIGTERM" | "TERM" => Signal::SIGTERM,
            "SIGINT" | "INT" => Signal::SIGINT,
            "SIGHUP" | "HUP" => Signal::SIGHUP,
            "SIGSTOP" | "STOP" => Signal::SIGSTOP,
            "SIGCONT" | "CONT" => Signal::SIGCONT,
            "SIGCHLD" | "CHLD" => Signal::SIGCHLD,
            "SIGUSR1" | "USR1" => Signal::SIGUSR1,
            "SIGUSR2" | "USR2" => Signal::SIGUSR2,
            _ => panic!("unsupported signal: {}", signame),
        };

        Some(signal)
    } else {
        None
    }
}

#[cfg(unix)]
pub fn install_handler<F>(handler: F)
where
    F: Fn(self::Signal) + 'static + Send + Sync,
{
    use nix::sys::signal::*;
    use std::thread;

    // Mask all signals interesting to us. The mask propagates
    // to all threads started after this point.
    let mut mask = SigSet::empty();
    mask.add(SIGKILL);
    mask.add(SIGTERM);
    mask.add(SIGINT);
    mask.add(SIGHUP);
    mask.add(SIGSTOP);
    mask.add(SIGCONT);
    mask.add(SIGCHLD);
    mask.add(SIGUSR1);
    mask.add(SIGUSR2);
    mask.thread_set_mask().expect("unable to set signal mask");

    set_handler(handler);

    #[allow(unsafe_code)]
    unsafe {
        let _ = sigaction(
            SIGCHLD,
            &SigAction::new(
                SigHandler::Handler(sigchld_handler),
                SaFlags::empty(),
                SigSet::empty(),
            ),
        );
    }

    // Spawn a thread to catch these signals
    thread::spawn(move || {
        loop {
            let signal = mask.wait().expect("Unable to sigwait");
            debug!("Received {:?}", signal);

            // Invoke closure
            invoke(signal);

            // Restore default behavior for received signal and unmask it
            if signal != SIGCHLD {
                let default_action =
                    SigAction::new(SigHandler::SigDfl, SaFlags::empty(), SigSet::empty());

                #[allow(unsafe_code)]
                unsafe {
                    let _ = sigaction(signal, &default_action);
                }
            }

            let mut new_mask = SigSet::empty();
            new_mask.add(signal);

            // Re-raise with signal unmasked
            let _ = new_mask.thread_unblock();
            let _ = raise(signal);
            let _ = new_mask.thread_block();
        }
    });
}

#[cfg(windows)]
pub fn install_handler<F>(handler: F)
where
    F: Fn(self::Signal) + 'static + Send + Sync,
{
    use kernel32::SetConsoleCtrlHandler;
    use winapi::{BOOL, DWORD, FALSE, TRUE};

    pub unsafe extern "system" fn ctrl_handler(_: DWORD) -> BOOL {
        invoke(self::Signal::SIGTERM);

        FALSE
    }

    set_handler(handler);

    unsafe {
        SetConsoleCtrlHandler(Some(ctrl_handler), TRUE);
    }
}

fn invoke(sig: self::Signal) {
    if let Some(ref handler) = *CLEANUP.lock().expect("poisoned lock in signal::invoke") {
        handler(sig)
    }
}

fn set_handler<F>(handler: F)
where
    F: Fn(self::Signal) + 'static + Send + Sync,
{
    *CLEANUP.lock().expect("poisoned lock in signal::set_handler") = Some(Box::new(handler));
}
