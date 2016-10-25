use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, SendError};

lazy_static! {
    static ref INTERRUPT_TX: Mutex<Option<Sender<()>>> = Mutex::new(None);
    static ref INTERRUPT_REQUESTED: AtomicBool = AtomicBool::new(false);
}

/// On Unix platforms, mask reception of SIGINT/SIGTERM, spawn a thread,
/// and sigwait on those signals to safely relay them.
#[cfg(unix)]
pub fn install() -> Receiver<()> {
    use std::thread;
    use nix::sys::signal::{SigSet, SIGTERM, SIGINT};

    let mut mask = SigSet::empty();
    mask.add(SIGTERM).expect("unable to add SIGTERM to mask");
    mask.add(SIGINT).expect("unable to add SIGINT to mask");
    mask.thread_set_mask().expect("unable to set signal mask");

    let rx = create_channel();

    thread::spawn(move || {
        loop {
            let _ = mask.wait().expect("unable to sigwait");

            let result = send_interrupt();
            if result.is_err() {
                break;
            }
        }
    });

    rx
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

pub fn interrupt_requested() -> bool {
    INTERRUPT_REQUESTED.load(Ordering::Relaxed)
}

fn create_channel() -> Receiver<()> {
    let mut guard = INTERRUPT_TX.lock().unwrap();
    if (*guard).is_some() {
        panic!("interrupt_handler::install() already called!");
    }

    let (tx, rx) = channel();
    (*guard) = Some(tx);

    rx
}

fn send_interrupt() -> Result<(), SendError<()>> {
    INTERRUPT_REQUESTED.store(true, Ordering::Relaxed);

    if let Some(ref mut tx) = *INTERRUPT_TX.lock().unwrap() {
        tx.send(())
    } else {
        Err(SendError(()))
    }
}
