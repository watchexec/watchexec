use std::sync::Mutex;
use nix::sys::signal::Signal;

lazy_static! {
    static ref CLEANUP: Mutex<Option<Box<Fn(self::Signal) + Send>>> = Mutex::new(None);
}

pub fn new(signal_name: &str) -> Signal {
    use nix::sys::signal::*;

    match signal_name {
        "SIGKILL" | "KILL" => SIGKILL,
        "SIGTERM" | "TERM" => SIGTERM,
        "SIGINT" | "INT" => SIGINT,
        "SIGHUP" | "HUP" => SIGHUP,
        "SIGSTOP" | "STOP" => SIGSTOP,
        "SIGCONT" | "CONT" => SIGCONT,
        "SIGCHLD" | "CHLD" => SIGCHLD,
        "SIGUSR1" | "USR1" => SIGUSR1,
        "SIGUSR2" | "USR2" => SIGUSR2,
        _ => panic!("unsupported signal: {}", signal_name),
    }
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
            let signal = mask.wait().expect("Unable to sigwait");
            debug!("Received {:?}", signal);

            // Invoke closure
            invoke(signal);

            // Restore default behavior for received signal and unmask it
            if signal != SIGCHLD {
                let default_action =
                    SigAction::new(SigHandler::SigDfl, SaFlags::empty(), SigSet::empty());

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
    where F: Fn(self::Signal) + 'static + Send + Sync
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
    if let Some(ref handler) = *CLEANUP.lock().unwrap() {
        handler(sig)
    }
}

fn set_handler<F>(handler: F)
    where F: Fn(self::Signal) + 'static + Send + Sync
{
    *CLEANUP.lock().unwrap() = Some(Box::new(handler));
}
