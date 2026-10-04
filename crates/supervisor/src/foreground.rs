use std::{fs::File, os::fd::OwnedFd};

use nix::{
	errno::Errno,
	sys::{
		signal::{self, SaFlags, SigAction, SigHandler, SigSet, Signal as NixSignal},
		termios::{self, SetArg, Termios},
	},
	unistd::{self, Pid},
};
use tracing::trace;

/// A held grant of the controlling terminal's foreground to a process group.
#[derive(Debug)]
pub struct ForegroundGrant {
	tty: OwnedFd,
	saved_termios: Termios,
	original_pgrp: Pid,
}

impl ForegroundGrant {
	/// Give the foreground of the controlling terminal to the process group `pgrp`, continuing
	/// it if it was stopped, and snapshot the terminal state for later restoration.
	///
	/// # Errors
	///
	/// Returns an error if there is no controlling terminal or a terminal operation failed; the
	/// caller should treat the feature as unavailable, leaving the stopped command untouched.
	pub fn acquire(pgrp: Pid) -> Result<Self, Errno> {
		let tty = open_controlling_tty()?;
		let saved_termios = termios::tcgetattr(&tty)?;
		let original_pgrp = unistd::getpgrp();
		with_sigtouu_ignored(|| unistd::tcsetpgrp(&tty, pgrp))?;
		signal::killpg(pgrp, NixSignal::SIGCONT)?;
		trace!(%pgrp, "granted terminal foreground");
		Ok(Self {
			tty,
			saved_termios,
			original_pgrp,
		})
	}

	/// Reclaim the foreground and restore the terminal state as of when the grant was made.
	///
	/// This is best-effort: errors are traced and swallowed, as it mostly runs on teardown paths
	/// (process exit, kill) where there is nothing to be done about a failure.
	pub fn release(self) {
		let Self {
			tty,
			saved_termios,
			original_pgrp,
		} = self;

		if let Err(error) = with_sigtouu_ignored(|| unistd::tcsetpgrp(&tty, original_pgrp)) {
			trace!(%error, "failed to reclaim the terminal foreground");
		}

		if let Err(error) = termios::tcsetattr(&tty, SetArg::TCSANOW, &saved_termios) {
			trace!(%error, "failed to restore the terminal state");
		} else {
			trace!("restored terminal state");
		}
	}
}

fn open_controlling_tty() -> Result<OwnedFd, Errno> {
	File::options()
		.read(true)
		.write(true)
		.open("/dev/tty")
		.map_err(|error| {
			trace!(%error, "could not open the controlling terminal");
			error.raw_os_error().map_or(Errno::ENOTTY, Errno::from_raw)
		})
		.map(OwnedFd::from)
}

/// Run `f` with SIGTTOU set to be ignored, restoring the previous handling afterwards.
///
/// The kernel permits terminal state changes from a background process group when the process
/// ignores or blocks SIGTTOU; this is the standard mechanism shells and other terminal-aware
/// programs use to change the foreground or termios while not in the foreground.
///
/// This is process-wide for the duration of `f`: it must never be active across a spawn, so
/// that commands always inherit default signal dispositions.
fn with_sigtouu_ignored<T>(f: impl FnOnce() -> T) -> T {
	// SAFETY: this installs SIG_IGN for SIGTTOU, which runs no handler code; the returned
	// previous action is only restored below, and was valid when it was installed.
	let restored = unsafe {
		signal::sigaction(
			NixSignal::SIGTTOU,
			&SigAction::new(SigHandler::SigIgn, SaFlags::empty(), SigSet::empty()),
		)
	};

	let result = f();

	if let Ok(restored) = restored {
		// SAFETY: as above: this restores the previously-installed action, which is either the
		// kernel default or another explicit choice made earlier in this process; both are
		// valid to install.
		let _ = unsafe { signal::sigaction(NixSignal::SIGTTOU, &restored) };
	}

	result
}
