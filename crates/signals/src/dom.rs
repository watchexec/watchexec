//! Notifications (signals or Windows control events) sent to the main process.

/// A notification sent to the main (watchexec) process.
///
/// On Windows, only [`Interrupt`][MainSignal::Interrupt] and [`Terminate`][MainSignal::Terminate]
/// will be produced: they are respectively `Ctrl-C` (SIGINT) and `Ctrl-Break` (SIGBREAK).
/// `Ctrl-Close` (the equivalent of `SIGHUP` on Unix, without the semantics of configuration reload)
/// is not supported, and on console close the process will be terminated by the OS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
pub enum MainSignal {
	/// Received when the terminal is disconnected.
	///
	/// On Unix, this is `SIGHUP`. On Windows, it is not produced.
	///
	/// This signal is available because it is a common signal used to reload configuration files,
	/// and it is reasonable that either watchexec could make use of it, or that it should be passed
	/// on to a sub process.
	Hangup,

	/// Received to indicate that the process should stop.
	///
	/// On Unix, this is `SIGINT`. On Windows, this is `Ctrl+C`.
	///
	/// This signal is generally produced by the user, so it may be handled differently than a
	/// termination.
	Interrupt,

	/// Received to cause the process to stop and the kernel to dump its core.
	///
	/// On Unix, this is `SIGQUIT`. On Windows, it is not produced.
	///
	/// This signal is available because it is reasonable that it could be passed on to a sub
	/// process, rather than terminate watchexec itself.
	Quit,

	/// Received to indicate that the process should stop.
	///
	/// On Unix, this is `SIGTERM`. On Windows, this is `Ctrl+Break`.
	///
	/// This signal is available for cleanup, but will generally not be passed on to a sub process
	/// with no other consequence: it is expected the main process should terminate.
	Terminate,

	/// Received for a user or application defined purpose.
	///
	/// On Unix, this is `SIGUSR1`. On Windows, it is not produced.
	///
	/// This signal is available because it is expected that it most likely should be passed on to a
	/// sub process or trigger a particular action within watchexec.
	User1,

	/// Received for a user or application defined purpose.
	///
	/// On Unix, this is `SIGUSR2`. On Windows, it is not produced.
	///
	/// This signal is available because it is expected that it most likely should be passed on to a
	/// sub process or trigger a particular action within watchexec.
	User2,
}
