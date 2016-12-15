use std::sync::Mutex;

lazy_static! {
    static ref CLEANUP: Mutex<Option<Box<Fn(self::Signal) + Send>>> = Mutex::new(None);
}

pub enum Signal {
    Terminate,
    Stop,
    Continue
}

#[cfg(unix)]
pub fn install_handler<F>(handler: F)
    where F: Fn(self::Signal) + 'static + Send + Sync
{
    use std::thread;
    use nix::sys::signal::*;

    // Mask all signals interesting to us. The mask propagates
    // to all threads started after this point.
    let mut mask = SigSet::empty();
    mask.add(SIGTERM);
    mask.add(SIGINT);
    mask.add(SIGTSTP);
    mask.add(SIGCONT);
    mask.thread_set_mask().expect("unable to set signal mask");

    set_handler(handler);

    // Spawn a thread to catch these signals
    thread::spawn(move || {
        loop {
            let raw_signal = mask.wait().expect("unable to sigwait");

            let sig = match raw_signal {
                SIGTERM => self::Signal::Terminate,
                SIGINT  => self::Signal::Terminate,
                SIGTSTP => self::Signal::Stop,
                SIGCONT => self::Signal::Continue,
                _       => unreachable!()
            };

            // Invoke closure
            invoke(sig);

            // Restore default behavior for received signal and unmask it
            let default_action = SigAction::new(SigHandler::SigDfl, SaFlags::empty(), SigSet::empty());

            unsafe {
                let _ = sigaction(raw_signal, &default_action);
            }

            let mut new_mask = SigSet::empty();
            new_mask.add(raw_signal);
            let _ = new_mask.thread_unblock();

            // Re-raise
            let _ = raise(raw_signal);
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
