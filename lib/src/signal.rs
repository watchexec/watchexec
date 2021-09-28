//! Event source for signals / notifications sent to the main process.

use tokio::{select, sync::mpsc};
use tracing::{debug, trace};

use crate::{
	error::{CriticalError, RuntimeError},
	event::{Event, Source, Tag},
};

/// A notification sent to the main (watchexec) process.
///
/// On Windows, only [`Interrupt`][Signal::Interrupt] and [`Terminate`][Signal::Terminate] will be
/// produced: they are respectively `Ctrl-C` (SIGINT) and `Ctrl-Break` (SIGBREAK). `Ctrl-Close` (the
/// equivalent of `SIGHUP` on Unix, without the semantics of configuration reload) is not supported,
/// and on console close the process will be terminated by the OS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
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

/// Launch the signal event worker.
///
/// While you _can_ run several, you **must** only have one. This may be enforced later.
///
/// # Examples
///
/// Direct usage:
///
/// ```no_run
/// use tokio::sync::mpsc;
/// use watchexec::signal::worker;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let (ev_s, _) = mpsc::channel(1024);
///     let (er_s, _) = mpsc::channel(64);
///
///     worker(er_s, ev_s).await?;
///     Ok(())
/// }
/// ```
pub async fn worker(
	errors: mpsc::Sender<RuntimeError>,
	events: mpsc::Sender<Event>,
) -> Result<(), CriticalError> {
	imp_worker(errors, events).await
}

#[cfg(unix)]
async fn imp_worker(
	errors: mpsc::Sender<RuntimeError>,
	events: mpsc::Sender<Event>,
) -> Result<(), CriticalError> {
	use tokio::signal::unix::{signal, SignalKind};

	debug!("launching unix signal worker");

	macro_rules! listen {
		($sig:ident) => {{
			trace!(kind=%stringify!($sig), "listening for unix signal");
			signal(SignalKind::$sig())?
		}}
	}

	let mut s_hangup = listen!(hangup);
	let mut s_interrupt = listen!(interrupt);
	let mut s_quit = listen!(quit);
	let mut s_terminate = listen!(terminate);
	let mut s_user1 = listen!(user_defined1);
	let mut s_user2 = listen!(user_defined2);

	loop {
		let sig = select!(
			_ = s_hangup.recv() => Signal::Hangup,
			_ = s_interrupt.recv() => Signal::Interrupt,
			_ = s_quit.recv() => Signal::Quit,
			_ = s_terminate.recv() => Signal::Terminate,
			_ = s_user1.recv() => Signal::User1,
			_ = s_user2.recv() => Signal::User2,
		);

		debug!(?sig, "received unix signal");
		send_event(errors.clone(), events.clone(), sig).await?;
	}
}

#[cfg(windows)]
async fn imp_worker(
	errors: mpsc::Sender<RuntimeError>,
	events: mpsc::Sender<Event>,
) -> Result<(), CriticalError> {
	use tokio::signal::windows::{ctrl_break, ctrl_c};

	debug!("launching windows signal worker");

	macro_rules! listen {
		($sig:ident) => {{
			trace!(kind=%stringify!($sig), "listening for windows process notification");
			$sig()?
		}}
	}

	let mut sigint = listen!(ctrl_c);
	let mut sigbreak = listen!(ctrl_break);

	loop {
		let sig = select!(
			_ = sigint.recv() => Signal::Interrupt,
			_ = sigbreak.recv() => Signal::Terminate,
		);

		debug!(?sig, "received windows process notification");
		send_event(errors.clone(), events.clone(), sig).await?;
	}
}

// TODO: figure out how to prioritise signals.
// TODO: also, figure out why event processing is stalling when the channel limit is reached (when there are lots of fs events).
async fn send_event(
	errors: mpsc::Sender<RuntimeError>,
	events: mpsc::Sender<Event>,
	sig: Signal,
) -> Result<(), CriticalError> {
	let tags = vec![
		Tag::Source(if sig == Signal::Interrupt {
			Source::Keyboard
		} else {
			Source::Os
		}),
		Tag::Signal(sig),
	];

	let event = Event {
		tags,
		metadata: Default::default(),
	};

	trace!(?event, "processed signal into event");
	if let Err(err) = events.send(event).await {
		errors
			.send(RuntimeError::EventChannelSend {
				ctx: "signals",
				err,
			})
			.await?;
	}

	Ok(())
}
