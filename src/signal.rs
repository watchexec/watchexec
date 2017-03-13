use std::sync::Mutex;
use std::fmt;

lazy_static! {
    static ref CLEANUP: Mutex<Option<Box<Fn(self::Signal) + Send>>> = Mutex::new(None);
}

// TODO: Probably a good idea to use
// https://nix-rust.github.io/nix/nix/sys/signal/enum.Signal.html
#[derive(Debug, Clone, Copy)]
pub enum Signal {
    // TODO: Probably a good idea to use original names here:
    // TODO: Add SIGUSR1+2 SIGHUP here?
    Terminate, // SIGTERM
    Stop, // SIGTSTP
    Continue, // SIGCONT
    ChildExit, // SIGCHLD
}

impl fmt::Display for Signal {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub fn new(signal_name: &str) -> Signal {
    println!("Using signal {}", signal_name);
    Signal::Terminate
}

#[cfg(unix)]
pub fn install_handler<F>(handler: F)
    where F: Fn(self::Signal) + 'static + Send + Sync
{
    use std::thread;
    use libc::c_int;
    use nix::sys::signal::*;

    // Mask all signals interesting to us. The mask propagates
    // to all threads started after this point.
    let mut mask = SigSet::empty();
    mask.add(SIGTERM);
    mask.add(SIGINT);
    // mask.add(SIGHUP);
    mask.add(SIGTSTP);
    mask.add(SIGCONT);
    mask.add(SIGCHLD);
    // mask.add(SIGUSR1);
    // mask.add(SIGUSR2);
    mask.thread_set_mask().expect("unable to set signal mask");

    set_handler(handler);

    // Indicate interest in SIGCHLD by setting a dummy handler
    pub extern "C" fn sigchld_handler(_: c_int) {}

    unsafe {
        let _ = sigaction(SIGCHLD,
                          &SigAction::new(SigHandler::Handler(sigchld_handler),
                                          SaFlags::empty(),
                                          SigSet::empty()));
    }

    // Spawn a thread to catch these signals
    thread::spawn(move || {
        loop {
            let raw_signal = mask.wait().expect("unable to sigwait");
            debug!("Received {:?}", raw_signal);

            let sig = match raw_signal {
                SIGTERM | SIGINT => self::Signal::Terminate,
                SIGTSTP => self::Signal::Stop,
                SIGCONT => self::Signal::Continue,
                SIGCHLD => self::Signal::ChildExit,
                _ => unreachable!(),
            };

            // Invoke closure
            invoke(sig);

            // Restore default behavior for received signal and unmask it
            if raw_signal != SIGCHLD {
                let default_action =
                    SigAction::new(SigHandler::SigDfl, SaFlags::empty(), SigSet::empty());

                unsafe {
                    let _ = sigaction(raw_signal, &default_action);
                }
            }

            let mut new_mask = SigSet::empty();
            new_mask.add(raw_signal);

            // Re-raise with signal unmasked
            let _ = new_mask.thread_unblock();
            let _ = raise(raw_signal);
            let _ = new_mask.thread_block();
        }
    });
}

#[cfg(windows)]
pub fn install_handler<F>(handler: F)
    where F: Fn(self::Signal) + 'static + Send + Sync
{
    use kernel32::SetConsoleCtrlHandler;
    use winapi::{BOOL, DWORD, FALSE, TRUE};

    pub unsafe extern "system" fn ctrl_handler(_: DWORD) -> BOOL {
        invoke(self::Signal::Terminate);

        FALSE
    }

    set_handler(handler);

    unsafe {
        SetConsoleCtrlHandler(Some(ctrl_handler), TRUE);
    }
}

fn invoke(sig: self::Signal) {
    if let Some(ref handler) = *CLEANUP.lock().unwrap() {
        handler(sig)
    }
}

fn set_handler<F>(handler: F)
    where F: Fn(self::Signal) + 'static + Send + Sync
{
    *CLEANUP.lock().unwrap() = Some(Box::new(handler));
}
