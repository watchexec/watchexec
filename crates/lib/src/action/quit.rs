use std::time::Duration;
use watchexec_signals::Signal;

/// How the Watchexec instance should quit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuitManner {
	/// Kill all processes and drop all jobs, then quit.
	Abort,

	/// Gracefully stop all jobs, then quit.
	Graceful {
		/// Signal to send immediately
		signal: Signal,
		/// Time to wait before forceful termination
		grace: Duration,
	},
}
