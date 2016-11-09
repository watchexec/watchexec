#[cfg(unix)]
pub fn install_handler<F>(handler: F)
    where F: Fn() + 'static + Send + Sync {

    use std::thread;
    use nix::sys::signal::*;

    // Mask all termination signals
    // These propagate to all threads started after this point
    let mut mask = SigSet::empty();
    mask.add(SIGTERM);
    mask.add(SIGINT);
    mask.thread_set_mask().expect("unable to set signal mask");

    // Spawn a thread to catch these signals
    thread::spawn(move || {
        let sig = mask.wait().expect("unable to sigwait");

        // Invoke closure
        handler();

        // Restore default behavior for received signal and unmask it
        unsafe {
            let _ = sigaction(sig, &SigAction::new(SigHandler::SigDfl, SaFlags::empty(), SigSet::empty()));
        }

        let mut new_mask = SigSet::empty();
        new_mask.add(sig);
        let _ = new_mask.thread_unblock();

        // Re-raise, killing the process
        let _ = raise(sig);
    });
}

/// On Windows, use SetConsoleCtrlHandler() to send an interrupt
/// SetConsoleCtrlHandler runs in it's own thread, so it's safe.
#[cfg(windows)]
pub fn install() -> Receiver<()> {
    use kernel32::SetConsoleCtrlHandler;
    use winapi::{BOOL, DWORD, TRUE};

    pub unsafe extern "system" fn ctrl_handler(_: DWORD) -> BOOL {
        let _ = send_interrupt();
        TRUE
    }

    let rx = create_channel();
    unsafe {
        SetConsoleCtrlHandler(Some(ctrl_handler), TRUE);
    }

    rx
}
