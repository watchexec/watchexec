#![doc = include_str!("../README.md")]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]

use std::fmt;

#[cfg(feature = "fromstr")]
use std::str::FromStr;

#[cfg(unix)]
use nix::sys::signal::Signal as NixSignal;

/// A notification (signals or Windows control events) sent to a process.
///
/// This signal type in Watchexec is used for any of:
/// - signals sent to the main process by some external actor,
/// - signals received from a sub process by the main process,
/// - signals sent to a sub process by Watchexec.
///
/// On Windows, only some signals are supported, as described. Others will be ignored.
///
/// On Unix, there are several "first-class" signals which have their own variants, and a generic
/// [`Custom`][Signal::Custom] variant which can be used to send arbitrary signals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
	feature = "serde",
	serde(
		from = "serde_support::SerdeSignal",
		into = "serde_support::SerdeSignal"
	)
)]
pub enum Signal {
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
	/// The special value `0` is used to indicate an unknown signal. That is, a signal was received
	/// or parsed, but it is not known which. This is not a usual case, and should in general be
	/// ignored rather than hard-erroring.
	///
	/// # Examples
	///
	/// ```
	/// # #[cfg(unix)]
	/// # {
	/// use watchexec_signals::Signal;
	/// use nix::sys::signal::Signal as NixSignal;
	/// assert_eq!(Signal::Custom(6), Signal::from(NixSignal::SIGABRT as i32));
	/// # }
	/// ```
	///
	/// On Unix the [`from_nix`][Signal::from_nix] method should be preferred if converting from
	/// Nix's `Signal` type:
	///
	/// ```
	/// # #[cfg(unix)]
	/// # {
	/// use watchexec_signals::Signal;
	/// use nix::sys::signal::Signal as NixSignal;
	/// assert_eq!(Signal::Custom(6), Signal::from_nix(NixSignal::SIGABRT));
	/// # }
	/// ```
	Custom(i32),
}

impl Signal {
	/// Converts to a [`nix::Signal`][NixSignal] if possible.
	///
	/// This will return `None` if the signal is not supported on the current platform (only for
	/// [`Custom`][Signal::Custom], as the first-class ones are always supported).
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

	/// Converts from a [`nix::Signal`][NixSignal].
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

impl From<i32> for Signal {
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

#[cfg(feature = "fromstr")]
impl Signal {
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
	/// # use watchexec_signals::Signal;
	/// assert_eq!(Signal::Hangup, Signal::from_unix_str("hup").unwrap());
	/// assert_eq!(Signal::Interrupt, Signal::from_unix_str("SIGINT").unwrap());
	/// assert_eq!(Signal::ForceStop, Signal::from_unix_str("Kill").unwrap());
	/// ```
	///
	/// Using [`FromStr`] is recommended for practical use, as it will also parse Windows control
	/// events, see [`Signal::from_windows_str`].
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
	/// # use watchexec_signals::Signal;
	/// assert_eq!(Signal::Hangup, Signal::from_windows_str("ctrl+close").unwrap());
	/// assert_eq!(Signal::Interrupt, Signal::from_windows_str("C").unwrap());
	/// assert_eq!(Signal::ForceStop, Signal::from_windows_str("Stop").unwrap());
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

#[cfg(feature = "fromstr")]
impl FromStr for Signal {
	type Err = SignalParseError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Self::from_windows_str(s).or_else(|err| Self::from_unix_str(s).map_err(|_| err))
	}
}

/// Error when parsing a signal from string.
#[cfg(feature = "fromstr")]
#[cfg_attr(feature = "miette", derive(miette::Diagnostic))]
#[derive(Debug, thiserror::Error)]
#[error("invalid signal `{src}`: {err}")]
pub struct SignalParseError {
	// The string that was parsed.
	#[cfg_attr(feature = "miette", source_code)]
	src: String,

	// The error that occurred.
	err: String,

	// The span of the source which is in error.
	#[cfg_attr(feature = "miette", label = "invalid signal")]
	span: (usize, usize),
}

#[cfg(feature = "fromstr")]
impl SignalParseError {
	#[must_use]
	pub fn new(src: &str, err: &str) -> Self {
		Self {
			src: src.to_owned(),
			err: err.to_owned(),
			span: (0, src.len()),
		}
	}
}

impl fmt::Display for Signal {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(
			f,
			"{}",
			match (self, cfg!(windows)) {
				(Self::Hangup, false) => "SIGHUP",
				(Self::Hangup, true) => "CTRL-CLOSE",
				(Self::ForceStop, false) => "SIGKILL",
				(Self::ForceStop, true) => "STOP",
				(Self::Interrupt, false) => "SIGINT",
				(Self::Interrupt, true) => "CTRL-C",
				(Self::Quit, _) => "SIGQUIT",
				(Self::Terminate, false) => "SIGTERM",
				(Self::Terminate, true) => "CTRL-BREAK",
				(Self::User1, _) => "SIGUSR1",
				(Self::User2, _) => "SIGUSR2",
				(Self::Custom(n), _) => {
					return write!(f, "{n}");
				}
			}
		)
	}
}

#[cfg(feature = "serde")]
mod serde_support {
	use super::Signal;

	#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
	#[serde(untagged)]
	pub enum SerdeSignal {
		Named(NamedSignal),
		Number(i32),
	}

	#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
	#[serde(rename_all = "kebab-case")]
	pub enum NamedSignal {
		#[serde(rename = "SIGHUP")]
		Hangup,
		#[serde(rename = "SIGKILL")]
		ForceStop,
		#[serde(rename = "SIGINT")]
		Interrupt,
		#[serde(rename = "SIGQUIT")]
		Quit,
		#[serde(rename = "SIGTERM")]
		Terminate,
		#[serde(rename = "SIGUSR1")]
		User1,
		#[serde(rename = "SIGUSR2")]
		User2,
	}

	impl From<Signal> for SerdeSignal {
		fn from(signal: Signal) -> Self {
			match signal {
				Signal::Hangup => Self::Named(NamedSignal::Hangup),
				Signal::Interrupt => Self::Named(NamedSignal::Interrupt),
				Signal::Quit => Self::Named(NamedSignal::Quit),
				Signal::Terminate => Self::Named(NamedSignal::Terminate),
				Signal::User1 => Self::Named(NamedSignal::User1),
				Signal::User2 => Self::Named(NamedSignal::User2),
				Signal::ForceStop => Self::Named(NamedSignal::ForceStop),
				Signal::Custom(number) => Self::Number(number),
			}
		}
	}

	impl From<SerdeSignal> for Signal {
		fn from(signal: SerdeSignal) -> Self {
			match signal {
				SerdeSignal::Named(NamedSignal::Hangup) => Self::Hangup,
				SerdeSignal::Named(NamedSignal::ForceStop) => Self::ForceStop,
				SerdeSignal::Named(NamedSignal::Interrupt) => Self::Interrupt,
				SerdeSignal::Named(NamedSignal::Quit) => Self::Quit,
				SerdeSignal::Named(NamedSignal::Terminate) => Self::Terminate,
				SerdeSignal::Named(NamedSignal::User1) => Self::User1,
				SerdeSignal::Named(NamedSignal::User2) => Self::User2,
				SerdeSignal::Number(number) => Self::Custom(number),
			}
		}
	}
}
