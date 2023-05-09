use std::{
	num::{NonZeroI32, NonZeroI64},
	process::ExitStatus,
};

use watchexec_signals::Signal;

/// The end status of a process.
///
/// This is a sort-of equivalent of the [`std::process::ExitStatus`] type which, while
/// constructable, differs on various platforms. The native type is an integer that is interpreted
/// either through convention or via platform-dependent libc or kernel calls; our type is a more
/// structured representation for the purpose of being clearer and transportable.
///
/// On Unix, one can tell whether a process dumped core from the exit status; this is not replicated
/// in this structure; if that's desirable you can obtain it manually via `libc::WCOREDUMP` and the
/// `ExitSignal` variant.
///
/// On Unix and Windows, the exit status is a 32-bit integer; on Fuchsia it's a 64-bit integer. For
/// portability, we use `i64`. On all platforms, the "success" value is zero, so we special-case
/// that as a variant and use `NonZeroI*` to limit the other values.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "disposition", content = "code"))]
pub enum ProcessEnd {
	/// The process ended successfully, with exit status = 0.
	#[cfg_attr(feature = "serde", serde(rename = "success"))]
	Success,

	/// The process exited with a non-zero exit status.
	#[cfg_attr(feature = "serde", serde(rename = "error"))]
	ExitError(NonZeroI64),

	/// The process exited due to a signal.
	#[cfg_attr(feature = "serde", serde(rename = "signal"))]
	ExitSignal(Signal),

	/// The process was stopped (but not terminated) (`libc::WIFSTOPPED`).
	#[cfg_attr(feature = "serde", serde(rename = "stop"))]
	ExitStop(NonZeroI32),

	/// The process suffered an unhandled exception or warning (typically Windows only).
	#[cfg_attr(feature = "serde", serde(rename = "exception"))]
	Exception(NonZeroI32),

	/// The process was continued (`libc::WIFCONTINUED`).
	#[cfg_attr(feature = "serde", serde(rename = "continued"))]
	Continued,
}

impl From<ExitStatus> for ProcessEnd {
	#[cfg(unix)]
	fn from(es: ExitStatus) -> Self {
		use std::os::unix::process::ExitStatusExt;

		match (es.code(), es.signal(), es.stopped_signal()) {
			(Some(_), Some(_), _) => {
				unreachable!("exitstatus cannot both be code and signal?!")
			}
			(Some(code), None, _) => {
				NonZeroI64::try_from(i64::from(code)).map_or(Self::Success, Self::ExitError)
			}
			(None, Some(_), Some(stopsig)) => {
				NonZeroI32::try_from(stopsig).map_or(Self::Success, Self::ExitStop)
			}
			#[cfg(not(target_os = "vxworks"))]
			(None, Some(_), _) if es.continued() => Self::Continued,
			(None, Some(signal), _) => Self::ExitSignal(signal.into()),
			(None, None, _) => Self::Success,
		}
	}

	#[cfg(windows)]
	fn from(es: ExitStatus) -> Self {
		match es.code().map(NonZeroI32::try_from) {
			None | Some(Err(_)) => Self::Success,
			Some(Ok(code)) if code.get() < 0 => Self::Exception(code),
			Some(Ok(code)) => Self::ExitError(code.into()),
		}
	}

	#[cfg(not(any(unix, windows)))]
	fn from(es: ExitStatus) -> Self {
		if es.success() {
			Self::Success
		} else {
			Self::ExitError(NonZeroI64::new(1).unwrap())
		}
	}
}
