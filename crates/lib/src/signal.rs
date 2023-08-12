//! Event source for signals / notifications sent to the main process.

use async_priority_channel as priority;
use tokio::{select, sync::mpsc};
use tracing::{debug, trace};
use watchexec_events::{Event, Priority, Source, Tag};
use watchexec_signals::Signal;

use crate::error::{CriticalError, RuntimeError};

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
/// use async_priority_channel as priority;
/// use watchexec::signal::worker;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let (ev_s, _) = priority::bounded(1024);
///     let (er_s, _) = mpsc::channel(64);
///
///     worker(er_s, ev_s).await?;
///     Ok(())
/// }
/// ```
pub async fn worker(
	errors: mpsc::Sender<RuntimeError>,
	events: priority::Sender<Event, Priority>,
) -> Result<(), CriticalError> {
	imp_worker(errors, events).await
}

#[cfg(unix)]
async fn imp_worker(
	errors: mpsc::Sender<RuntimeError>,
	events: priority::Sender<Event, Priority>,
) -> Result<(), CriticalError> {
	use tokio::signal::unix::{signal, SignalKind};

	debug!("launching unix signal worker");

	macro_rules! listen {
	($sig:ident) => {{
		trace!(kind=%stringify!($sig), "listening for unix signal");
		signal(SignalKind::$sig()).map_err(|err| CriticalError::IoError {
		about: concat!("setting ", stringify!($sig), " signal listener"), err
	})?
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
	events: priority::Sender<Event, Priority>,
) -> Result<(), CriticalError> {
	use tokio::signal::windows::{ctrl_break, ctrl_c};

	debug!("launching windows signal worker");

	macro_rules! listen {
	($sig:ident) => {{
		trace!(kind=%stringify!($sig), "listening for windows process notification");
		$sig().map_err(|err| CriticalError::IoError {
			about: concat!("setting ", stringify!($sig), " signal listener"), err
		})?
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

async fn send_event(
	errors: mpsc::Sender<RuntimeError>,
	events: priority::Sender<Event, Priority>,
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
	if let Err(err) = events
		.send(
			event,
			match sig {
				Signal::Interrupt | Signal::Terminate => Priority::Urgent,
				_ => Priority::High,
			},
		)
		.await
	{
		errors
			.send(RuntimeError::EventChannelSend {
				ctx: "signals",
				err,
			})
			.await?;
	}

	Ok(())
}
