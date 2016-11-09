use std::sync::Mutex;

lazy_static! {
    static ref CLEANUP: Mutex<Option<Box<Fn() + Send>>> = Mutex::new(None);
}

#[cfg(unix)]
pub fn install_handler<F>(handler: F)
    where F: Fn() + 'static + Send + Sync
{

    use std::thread;
    use nix::sys::signal::*;

    // Mask all termination signals
    // These propagate to all threads started after this point
    let mut mask = SigSet::empty();
    mask.add(SIGTERM);
    mask.add(SIGINT);
    mask.thread_set_mask().expect("unable to set signal mask");

    set_handler(handler);

    // Spawn a thread to catch these signals
    thread::spawn(move || {
        let sig = mask.wait().expect("unable to sigwait");

        // Invoke closure
        invoke();

        // Restore default behavior for received signal and unmask it
        unsafe {
            let _ =
                sigaction(sig,
                          &SigAction::new(SigHandler::SigDfl, SaFlags::empty(), SigSet::empty()));
        }

        let mut new_mask = SigSet::empty();
        new_mask.add(sig);
        let _ = new_mask.thread_unblock();

        // Re-raise, killing the process
        let _ = raise(sig);
    });
}

#[cfg(windows)]
pub fn install_handler<F>(handler: F)
    where F: Fn() + 'static + Send + Sync
{

    use kernel32::SetConsoleCtrlHandler;
    use winapi::{BOOL, DWORD, FALSE, TRUE};

    pub unsafe extern "system" fn ctrl_handler(_: DWORD) -> BOOL {
        invoke();

        FALSE
    }

    set_handler(handler);

    unsafe {
        SetConsoleCtrlHandler(Some(ctrl_handler), TRUE);
    }
}

fn invoke() {
    if let Some(ref handler) = *CLEANUP.lock().unwrap() {
        handler()
    }
}

fn set_handler<F>(handler: F)
    where F: Fn() + 'static + Send + Sync
{

    *CLEANUP.lock().unwrap() = Some(Box::new(handler));
}
