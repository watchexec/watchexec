//! Event source for keyboard input and related events
use async_priority_channel as priority;
use tokio::{
	io::AsyncReadExt,
	sync::{mpsc, oneshot, watch},
};
use tracing::trace;

use crate::{
	error::{CriticalError, RuntimeError},
	event::{Event, Priority, Source, Tag},
};

/// The configuration of the [keyboard][self] worker.
///
/// This is marked non-exhaustive so new configuration can be added without breaking.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct WorkingData {
	/// Whether or not to watch for 'end of file' on stdin
	pub eof: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
/// Enumeration of different keyboard events
pub enum Keyboard {
	/// Event representing an 'end of file' on stdin
	Eof,
}

/// Launch the filesystem event worker.
///
/// While you can run several, you should only have one.
///
/// Sends keyboard events via to the provided 'events' channel
pub async fn worker(
	mut working: watch::Receiver<WorkingData>,
	errors: mpsc::Sender<RuntimeError>,
	events: priority::Sender<Event, Priority>,
) -> Result<(), CriticalError> {
	let mut send_close = None;
	while working.changed().await.is_ok() {
		let watch_for_eof = { working.borrow().eof };
		if watch_for_eof {
			// If we want to watch stdin and we're not already watching it then spawn a task to watch it
			// otherwise take no action
			if let None = send_close {
				let (close_s, close_r) = tokio::sync::oneshot::channel::<()>();

				send_close = Some(close_s);
				tokio::spawn(watch_stdin(errors.clone(), events.clone(), close_r));
			}
		} else {
			// If we don't want to watch stdin but we are already watching it then send a close signal to end the
			// watching, otherwise take no action
			if let Some(close_s) = send_close.take() {
				close_s.send(());
			}
		}
	}

	Ok(())
}

async fn watch_stdin(
	errors: mpsc::Sender<RuntimeError>,
	events: priority::Sender<Event, Priority>,
	mut close_r: oneshot::Receiver<()>,
) -> Result<(), CriticalError> {
	let mut stdin = tokio::io::stdin();
	let mut buffer = [0; 10];
	loop {
		tokio::select! {
			result = stdin.read(&mut buffer[..]) => {
				// Read from stdin and if we've read 0 bytes then we assume stdin has received an 'eof' so
				// we send that event into the system and break out of the loop as 'eof' means that there will
				// be no more information on stdin.
				match result {
					Ok(0) => {
						send_event(errors, events, Keyboard::Eof).await?;
						break;
					}
					Err(_) => break,
					_ => {
					}
				}
			}
			_ = &mut close_r => {
				// If we receive a close signal then break out of the loop and end which drops
				// our handle on stdin
				break;
			}
		}
	}

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
