//! Types for cross-platform and cross-purpose handling of subprocess signals.

use std::str::FromStr;

#[cfg(unix)]
use command_group::Signal as NixSignal;
use miette::Diagnostic;
use thiserror::Error;

use super::source::MainSignal;

/// A notification sent to a subprocess.
///
/// On Windows, only some signals are supported, as described. Others will be ignored.
///
/// On Unix, there are several "first-class" signals which have their own variants, and a generic
/// [`Custom`][SubSignal::Custom] variant which can be used to send arbitrary signals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubSignal {
	/// Indicate that the terminal is disconnected.
	///
	/// On Unix, this is `SIGHUP`. On Windows, this is ignored for now but may be supported in the
	/// future (see [#219](https://github.com/watchexec/watchexec/issues/219)).
	///
	/// Despite its nominal purpose, on Unix this signal is often used to reload configuration files.
	Hangup,

	/// Indicate to the kernel that the process should stop.
	///
	/// On Unix, this is `SIGKILL`. On Windows, this is `TerminateProcess`.
	///
	/// This signal is not handled by the process, but directly by the kernel, and thus cannot be
	/// intercepted. Subprocesses may exit in inconsistent states.
	ForceStop,

	/// Indicate that the process should stop.
	///
	/// On Unix, this is `SIGINT`. On Windows, this is ignored for now but may be supported in the
	/// future (see [#219](https://github.com/watchexec/watchexec/issues/219)).
	///
	/// This signal generally indicates an action taken by the user, so it may be handled
	/// differently than a termination.
	Interrupt,

	/// Indicate that the process is to stop, the kernel will then dump its core.
	///
	/// On Unix, this is `SIGQUIT`. On Windows, it is ignored.
	///
	/// This is rarely used.
	Quit,

	/// Indicate that the process should stop.
	///
	/// On Unix, this is `SIGTERM`. On Windows, this is ignored for now but may be supported in the
	/// future (see [#219](https://github.com/watchexec/watchexec/issues/219)).
	///
	/// On Unix, this signal generally indicates an action taken by the system, so it may be handled
	/// differently than an interruption.
	Terminate,

	/// Indicate an application-defined behaviour should happen.
	///
	/// On Unix, this is `SIGUSR1`. On Windows, it is ignored.
	///
	/// This signal is generally used to start debugging.
	User1,

	/// Indicate an application-defined behaviour should happen.
	///
	/// On Unix, this is `SIGUSR2`. On Windows, it is ignored.
	///
	/// This signal is generally used to reload configuration.
	User2,

	/// Indicate using a custom signal.
	///
	/// Internally, this is converted to a [`nix::Signal`](https://docs.rs/nix/*/nix/sys/signal/enum.Signal.html)
	/// but for portability this variant is a raw `i32`.
	///
	/// Invalid signals on the current platform will be ignored. Does nothing on Windows.
	///
	/// # Examples
	///
	/// ```
	/// # // we don't have a direct nix dependency, so we fake it... rather horribly
	/// # mod nix { pub mod sys { pub mod signal {
	/// # #[cfg(unix)] pub use command_group::Signal;
	/// # #[cfg(not(unix))] #[repr(i32)] pub enum Signal { SIGABRT = 6 }
	/// # } } }
	/// use watchexec::signal::process::SubSignal;
	/// use nix::sys::signal::Signal;
	/// assert_eq!(SubSignal::Custom(6), SubSignal::from(Signal::SIGABRT as i32));
	/// ```
	///
	/// On Unix the [`from_nix`][SubSignal::from_nix] method should be preferred if converting from
	/// Nix's `Signal` type:
	///
	/// ```
	/// # #[cfg(unix)]
	/// # {
	/// # // we don't have a direct nix dependency, so we fake it... rather horribly
	/// # mod nix { pub mod sys { pub mod signal { pub use command_group::Signal; } } }
	/// use watchexec::signal::process::SubSignal;
	/// use nix::sys::signal::Signal;
	/// assert_eq!(SubSignal::Custom(6), SubSignal::from_nix(Signal::SIGABRT));
	/// # }
	/// ```
	Custom(i32),
}

impl SubSignal {
	/// Converts to a [`nix::Signal`][command_group::Signal] if possible.
	///
	/// This will return `None` if the signal is not supported on the current platform (only for
	/// [`Custom`][SubSignal::Custom], as the first-class ones are always supported).
	#[cfg(unix)]
	pub fn to_nix(self) -> Option<NixSignal> {
		use std::convert::TryFrom;

		match self {
			Self::Hangup => Some(NixSignal::SIGHUP),
			Self::ForceStop => Some(NixSignal::SIGKILL),
			Self::Interrupt => Some(NixSignal::SIGINT),
			Self::Quit => Some(NixSignal::SIGQUIT),
			Self::Terminate => Some(NixSignal::SIGTERM),
			Self::User1 => Some(NixSignal::SIGUSR1),
			Self::User2 => Some(NixSignal::SIGUSR2),
			Self::Custom(sig) => NixSignal::try_from(sig).ok(),
		}
	}

	/// Converts from a [`nix::Signal`][command_group::Signal].
	#[cfg(unix)]
	pub fn from_nix(sig: NixSignal) -> Self {
		match sig {
			NixSignal::SIGHUP => Self::Hangup,
			NixSignal::SIGKILL => Self::ForceStop,
			NixSignal::SIGINT => Self::Interrupt,
			NixSignal::SIGQUIT => Self::Quit,
			NixSignal::SIGTERM => Self::Terminate,
			NixSignal::SIGUSR1 => Self::User1,
			NixSignal::SIGUSR2 => Self::User2,
			sig => Self::Custom(sig as _),
		}
	}
}

impl From<MainSignal> for SubSignal {
	fn from(main: MainSignal) -> Self {
		match main {
			MainSignal::Hangup => Self::Hangup,
			MainSignal::Interrupt => Self::Interrupt,
			MainSignal::Quit => Self::Quit,
			MainSignal::Terminate => Self::Terminate,
			MainSignal::User1 => Self::User1,
			MainSignal::User2 => Self::User2,
		}
	}
}

impl From<i32> for SubSignal {
	/// Converts from a raw signal number.
	///
	/// This uses hardcoded numbers for the first-class signals.
	fn from(raw: i32) -> Self {
		match raw {
			1 => Self::Hangup,
			2 => Self::Interrupt,
			3 => Self::Quit,
			9 => Self::ForceStop,
			10 => Self::User1,
			12 => Self::User2,
			15 => Self::Terminate,
			_ => Self::Custom(raw),
		}
	}
}

impl FromStr for SubSignal {
	type Err = SignalParseError;

	#[cfg(unix)]
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		use std::convert::TryFrom;

		if let Ok(sig) = i32::from_str(s) {
			if let Ok(sig) = NixSignal::try_from(sig) {
				return Ok(Self::from_nix(sig));
			}
		}

		if let Ok(sig) = NixSignal::from_str(&s.to_ascii_uppercase())
			.or_else(|_| NixSignal::from_str(&format!("SIG{}", s.to_ascii_uppercase())))
		{
			return Ok(Self::from_nix(sig));
		}

		Err(SignalParseError::new(s, "unsupported signal"))
	}

	#[cfg(windows)]
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s.to_ascii_uppercase().as_str() {
			"CTRL-CLOSE" | "CTRL+CLOSE" | "CLOSE" => Ok(Self::Hangup),
			"CTRL-BREAK" | "CTRL+BREAK" | "BREAK" => Ok(Self::Terminate),
			"CTRL-C" | "CTRL+C" | "C" => Ok(Self::Interrupt),
			"KILL" | "SIGKILL" | "FORCE-STOP" | "STOP" => Ok(Self::ForceStop),
			_ => Err(SignalParseError::new(s, "unknown control name")),
		}
	}

	#[cfg(not(any(unix, windows)))]
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Err(SignalParseError::new(s, "no signals supported"))
	}
}

/// Error when parsing a signal from string.
#[derive(Debug, Diagnostic, Error)]
#[error("invalid signal `{src}`: {err}")]
#[diagnostic(code(watchexec::signal::process::parse), url(docsrs))]
pub struct SignalParseError {
	// The string that was parsed.
	#[source_code]
	src: String,

	// The error that occurred.
	err: String,

	// The span of the source which is in error.
	#[label = "invalid signal"]
	span: (usize, usize),
}

impl SignalParseError {
	fn new(src: &str, err: &str) -> Self {
		Self {
			src: src.to_owned(),
			err: err.to_owned(),
			span: (0, src.len()),
		}
	}
}
