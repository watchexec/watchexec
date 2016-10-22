use std::sync::Mutex;
use std::sync::mpsc::{channel, Receiver, Sender, SendError};

lazy_static! {
    static ref INTERRUPT_TX: Mutex<Option<Sender<()>>> = Mutex::new(None);
}

/// On Unix platforms, spawn a thread and use the signal crate
/// to relay signals back to the main thread.
#[cfg(unix)]
pub fn install() -> Receiver<()> {
    use std::thread;
    use nix::sys::signal::{SIGTERM, SIGINT};
    use signal::trap::Trap;

    let trap = Trap::trap(&[SIGTERM, SIGINT]);
    let rx = create_channel();

    thread::spawn(move || {
        for _ in trap {
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
        send_interrupt();
        TRUE
    }

    let rx = create_channel();
    unsafe {
        SetConsoleCtrlHandler(Some(ctrl_handler), TRUE);
    }

    rx
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
    if let Some(ref mut tx) = *INTERRUPT_TX.lock().unwrap() {
        tx.send(())
    } else {
        Err(SendError(()))
    }
}
