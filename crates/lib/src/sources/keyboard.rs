//! Event source for keyboard input and related events
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_priority_channel as priority;
use tokio::{
	spawn,
	sync::{mpsc, oneshot},
};
use tracing::trace;
use watchexec_events::{Event, KeyCode, Keyboard, Modifiers, Priority, Source, Tag};

use crate::{
	error::{CriticalError, RuntimeError},
	Config,
};

/// Launch the keyboard event worker.
///
/// While you can run several, you should only have one.
///
/// Sends keyboard events via to the provided 'events' channel
pub async fn worker(
	config: Arc<Config>,
	errors: mpsc::Sender<RuntimeError>,
	events: priority::Sender<Event, Priority>,
) -> Result<(), CriticalError> {
	let mut send_close = None;
	let mut config_watch = config.watch();
	loop {
		config_watch.next().await;
		let want_keyboard = config.keyboard_events.get();
		match (want_keyboard, &send_close) {
			// if we want to watch stdin and we're not already watching it then spawn a task to watch it
			(true, None) => {
				let (close_s, close_r) = oneshot::channel::<()>();

				send_close = Some(close_s);
				spawn(watch_stdin(errors.clone(), events.clone(), close_r));
			}
			// if we don't want to watch stdin but we are already watching it then send a close signal to end
			// the watching
			(false, Some(_)) => {
				// ignore send error as if channel is closed watch is already gone
				send_close
					.take()
					.expect("unreachable due to match")
					.send(())
					.ok();
			}
			// otherwise no action is required
			_ => {}
		}
	}
}

#[cfg(unix)]
mod raw_mode {
	use std::os::fd::AsRawFd;

	/// Stored original termios to restore on drop.
	pub struct RawModeGuard {
		fd: i32,
		original: libc::termios,
	}

	impl RawModeGuard {
		/// Switch stdin to raw mode. Returns None if stdin is not a TTY.
		pub fn enter() -> Option<Self> {
			let fd = std::io::stdin().as_raw_fd();
			// SAFETY: isatty, tcgetattr, cfmakeraw, and tcsetattr are POSIX standard
			// functions operating on a valid fd (stdin). We check return values before
			// proceeding. The original termios is saved and restored in Drop.
			unsafe {
				if libc::isatty(fd) == 0 {
					return None;
				}
				let mut original: libc::termios = std::mem::zeroed();
				if libc::tcgetattr(fd, &mut original) != 0 {
					return None;
				}
				let mut raw = original;
				libc::cfmakeraw(&mut raw);
				// Re-enable output post-processing so \n still maps to \r\n
				raw.c_oflag |= libc::OPOST;
				// Non-blocking reads: return after 100ms if no input available.
				// This ensures the tokio blocking thread doesn't park forever,
				// allowing graceful shutdown when the close signal is received.
				raw.c_cc[libc::VMIN] = 0;
				raw.c_cc[libc::VTIME] = 1;
				if libc::tcsetattr(fd, libc::TCSANOW, &raw) != 0 {
					return None;
				}
				Some(Self { fd, original })
			}
		}
	}

	impl Drop for RawModeGuard {
		fn drop(&mut self) {
			// SAFETY: restoring the original termios saved in enter() on the same fd.
			unsafe {
				libc::tcsetattr(self.fd, libc::TCSANOW, &self.original);
			}
		}
	}
}

#[cfg(windows)]
mod raw_mode {
	use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
	use windows_sys::Win32::System::Console::{
		GetConsoleMode, GetStdHandle, SetConsoleMode, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT,
		ENABLE_PROCESSED_INPUT, STD_INPUT_HANDLE,
	};

	/// Stored original console mode to restore on drop.
	pub struct RawModeGuard {
		handle: HANDLE,
		original_mode: u32,
	}

	// SAFETY: HANDLE is a process-global value (stdin) that is safe to use from any thread.
	unsafe impl Send for RawModeGuard {}

	impl RawModeGuard {
		/// Switch stdin to raw-like mode. Returns None if stdin is not a console.
		pub fn enter() -> Option<Self> {
			// SAFETY: GetStdHandle, GetConsoleMode, and SetConsoleMode are Windows Console
			// API functions. We check return values before proceeding. The handle is valid
			// for the lifetime of the process. The original mode is saved and restored in Drop.
			unsafe {
				let handle = GetStdHandle(STD_INPUT_HANDLE);
				if handle == INVALID_HANDLE_VALUE || handle.is_null() {
					return None;
				}
				let mut original_mode: u32 = 0;
				if GetConsoleMode(handle, &mut original_mode) == 0 {
					return None;
				}
				// Disable line input, echo, and Ctrl+C signal processing
				let raw_mode = original_mode
					& !(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT);
				if SetConsoleMode(handle, raw_mode) == 0 {
					return None;
				}
				Some(Self {
					handle,
					original_mode,
				})
			}
		}
	}

	impl Drop for RawModeGuard {
		fn drop(&mut self) {
			// SAFETY: restoring the original console mode saved in enter() on the same handle.
			unsafe {
				SetConsoleMode(self.handle, self.original_mode);
			}
		}
	}
}

fn byte_to_keyboard(byte: u8) -> Option<Keyboard> {
	match byte {
		// Ctrl-C / Ctrl-D
		3 | 4 => Some(Keyboard::Eof),
		// Enter (byte 13, before Ctrl range to avoid overlap)
		13 => Some(Keyboard::Key {
			key: KeyCode::Enter,
			modifiers: Modifiers::default(),
		}),
		// Ctrl+letter (1-26 excluding 3,4,13 handled above)
		b @ 1..=26 => Some(Keyboard::Key {
			key: KeyCode::Char((b + b'a' - 1) as char),
			modifiers: Modifiers {
				ctrl: true,
				..Default::default()
			},
		}),
		27 => Some(Keyboard::Key {
			key: KeyCode::Escape,
			modifiers: Modifiers::default(),
		}),
		b if char::from(b).is_ascii_graphic() || b == b' ' => Some(Keyboard::Key {
			key: KeyCode::Char(char::from(b)),
			modifiers: Modifiers::default(),
		}),
		_ => None,
	}
}

async fn watch_stdin(
	errors: mpsc::Sender<RuntimeError>,
	events: priority::Sender<Event, Priority>,
	close_r: oneshot::Receiver<()>,
) -> Result<(), CriticalError> {
	// Use an AtomicBool to signal the blocking reader to stop.
	// This avoids tokio::io::stdin() which uses blocking threads that can't be
	// interrupted, causing the process to hang on shutdown (issue #1017).
	let cancel = Arc::new(AtomicBool::new(false));
	let cancel_clone = cancel.clone();

	let (tx, mut rx) = mpsc::channel::<Result<Vec<u8>, ()>>(16);

	// Spawn a blocking task that reads stdin directly
	tokio::task::spawn_blocking(move || {
		#[cfg(any(unix, windows))]
		let _raw_guard = raw_mode::RawModeGuard::enter();

		let mut stdin = std::io::stdin().lock();
		let mut buffer = [0u8; 10];

		while !cancel_clone.load(Ordering::Relaxed) {
			match stdin.read(&mut buffer) {
				Ok(0) => {
					// EOF or VTIME timeout with no data
					// With VMIN=0/VTIME=1, this is a timeout - just loop and check cancel
					#[cfg(any(unix, windows))]
					if _raw_guard.is_some() {
						continue;
					}
					// Real EOF in non-raw mode
					let _ = tx.blocking_send(Ok(vec![]));
					break;
				}
				Ok(n) => {
					if tx.blocking_send(Ok(buffer[..n].to_vec())).is_err() {
						break;
					}
				}
				Err(_) => {
					let _ = tx.blocking_send(Err(()));
					break;
				}
			}
		}
	});

	// Wait for either data from stdin or the close signal
	tokio::select! {
		_ = async {
			'read: while let Some(result) = rx.recv().await {
				match result {
					Ok(bytes) if bytes.is_empty() => {
						// EOF
						let _ = send_event(errors.clone(), events.clone(), Keyboard::Eof).await;
						break;
					}
					Ok(bytes) => {
						for &byte in &bytes {
							if let Some(key) = byte_to_keyboard(byte) {
								let is_eof = matches!(key, Keyboard::Eof);
								let _ = send_event(errors.clone(), events.clone(), key).await;
								if is_eof {
									break 'read;
								}
							}
						}
					}
					Err(()) => break,
				}
			}
		} => {}
		_ = close_r => {}
	}

	// Always signal the blocking thread to stop when we exit
	cancel.store(true, Ordering::Relaxed);

	Ok(())
}

async fn send_event(
	errors: mpsc::Sender<RuntimeError>,
	events: priority::Sender<Event, Priority>,
	msg: Keyboard,
) -> Result<(), CriticalError> {
	let tags = vec![Tag::Source(Source::Keyboard), Tag::Keyboard(msg)];

	let event = Event {
		tags,
		metadata: Default::default(),
	};

	trace!(?event, "processed keyboard input into event");
	if let Err(err) = events.send(event, Priority::Normal).await {
		errors
			.send(RuntimeError::EventChannelSend {
				ctx: "keyboard",
				err,
			})
			.await?;
	}

	Ok(())
}
