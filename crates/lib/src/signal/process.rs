//! Types for cross-platform and cross-purpose handling of subprocess signals.

use std::str::FromStr;

#[cfg(unix)]
use nix::sys::signal::Signal as NixSignal;

use crate::error::SignalParseError;

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
	/// # #[cfg(unix)]
	/// # {
	/// use watchexec::signal::process::SubSignal;
	/// use nix::sys::signal::Signal;
	/// assert_eq!(SubSignal::Custom(6), SubSignal::from(Signal::SIGABRT as i32));
	/// # }
	/// ```
	///
	/// On Unix the [`from_nix`][SubSignal::from_nix] method should be preferred if converting from
	/// Nix's `Signal` type:
	///
	/// ```
	/// # #[cfg(unix)]
	/// # {
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
	#[must_use]
	pub fn to_nix(self) -> Option<NixSignal> {
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
	#[allow(clippy::missing_const_for_fn)]
	#[must_use]
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

impl SubSignal {
	/// Parse the input as a unix signal.
	///
	/// This parses the input as a signal name, or a signal number, in a case-insensitive manner.
	/// It supports integers, the short name of the signal (like `INT`, `HUP`, `USR1`, etc), and
	/// the long name of the signal (like `SIGINT`, `SIGHUP`, `SIGUSR1`, etc).
	///
	/// Note that this is entirely accurate only when used on unix targets; on other targets it
	/// falls back to a hardcoded approximation instead of looking up signal tables (via [`nix`]).
	///
	/// ```
	/// # use watchexec::signal::process::SubSignal;
	/// assert_eq!(SubSignal::Hangup, SubSignal::from_unix_str("hup").unwrap());
	/// assert_eq!(SubSignal::Interrupt, SubSignal::from_unix_str("SIGINT").unwrap());
	/// assert_eq!(SubSignal::ForceStop, SubSignal::from_unix_str("Kill").unwrap());
	/// assert_eq!(SubSignal::User2, SubSignal::from_unix_str("12").unwrap());
	/// ```
	///
	/// Using [`FromStr`] is recommended for practical use, as it will also parse Windows control
	/// events, see [`SubSignal::from_windows_str`].
	pub fn from_unix_str(s: &str) -> Result<Self, SignalParseError> {
		Self::from_unix_str_impl(s)
	}

	#[cfg(unix)]
	fn from_unix_str_impl(s: &str) -> Result<Self, SignalParseError> {
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

	#[cfg(not(unix))]
	fn from_unix_str_impl(s: &str) -> Result<Self, SignalParseError> {
		match s.to_ascii_uppercase().as_str() {
			"KILL" | "SIGKILL" | "9" => Ok(Self::ForceStop),
			"HUP" | "SIGHUP" | "1" => Ok(Self::Hangup),
			"INT" | "SIGINT" | "2" => Ok(Self::Interrupt),
			"QUIT" | "SIGQUIT" | "3" => Ok(Self::Quit),
			"TERM" | "SIGTERM" | "15" => Ok(Self::Terminate),
			"USR1" | "SIGUSR1" | "10" => Ok(Self::User1),
			"USR2" | "SIGUSR2" | "12" => Ok(Self::User2),
			number => match i32::from_str(number) {
				Ok(int) => Ok(Self::Custom(int)),
				Err(_) => Err(SignalParseError::new(s, "unsupported signal")),
			},
		}
	}

	/// Parse the input as a windows control event.
	///
	/// This parses the input as a control event name, in a case-insensitive manner.
	///
	/// The names matched are mostly made up as there's no standard for them, but should be familiar
	/// to Windows users. They are mapped to the corresponding unix concepts as follows:
	///
	/// - `CTRL-CLOSE`, `CTRL+CLOSE`, or `CLOSE` for a hangup
	/// - `CTRL-BREAK`, `CTRL+BREAK`, or `BREAK` for a terminate
	/// - `CTRL-C`, `CTRL+C`, or `C` for an interrupt
	/// - `STOP`, `FORCE-STOP` for a forced stop. This is also mapped to `KILL` and `SIGKILL`.
	///
	/// ```
	/// # use watchexec::signal::process::SubSignal;
	/// assert_eq!(SubSignal::Hangup, SubSignal::from_windows_str("ctrl+close").unwrap());
	/// assert_eq!(SubSignal::Interrupt, SubSignal::from_windows_str("C").unwrap());
	/// assert_eq!(SubSignal::ForceStop, SubSignal::from_windows_str("Stop").unwrap());
	/// ```
	///
	/// Using [`FromStr`] is recommended for practical use, as it will fall back to parsing as a
	/// unix signal, which can be helpful for portability.
	pub fn from_windows_str(s: &str) -> Result<Self, SignalParseError> {
		match s.to_ascii_uppercase().as_str() {
			"CTRL-CLOSE" | "CTRL+CLOSE" | "CLOSE" => Ok(Self::Hangup),
			"CTRL-BREAK" | "CTRL+BREAK" | "BREAK" => Ok(Self::Terminate),
			"CTRL-C" | "CTRL+C" | "C" => Ok(Self::Interrupt),
			"KILL" | "SIGKILL" | "FORCE-STOP" | "STOP" => Ok(Self::ForceStop),
			_ => Err(SignalParseError::new(s, "unknown control name")),
		}
	}
}

impl FromStr for SubSignal {
	type Err = SignalParseError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Self::from_windows_str(s).or_else(|err| Self::from_unix_str(s).map_err(|_| err))
	}
}
