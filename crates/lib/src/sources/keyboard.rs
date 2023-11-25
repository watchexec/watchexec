//! Event source for keyboard input and related events
use std::sync::Arc;

use async_priority_channel as priority;
use futures::StreamExt;
use tokio::{
	io::AsyncReadExt,
	select, spawn,
	sync::{mpsc, oneshot},
};
use tracing::trace;
use watchexec_events::{Event, Keyboard, Priority, Source, Tag};

use crate::{
	error::{CriticalError, RuntimeError},
	Config,
};

/// Launch the filesystem event worker.
///
/// While you can run several, you should only have one.
///
/// Sends keyboard events via to the provided 'events' channel
pub async fn worker(
	config: Arc<Config>,
	errors: mpsc::Sender<RuntimeError>,
	events: priority::Sender<Event, Priority>,
) -> Result<(), CriticalError> {
	let mut send_close = None;
	let mut config_watch = config.watch();
	while config_watch.next().await.is_some() {
		match (config.keyboard_events.get(), &send_close) {
			// if we want to watch stdin and we're not already watching it then spawn a task to watch it
			(true, None) => {
				let (close_s, close_r) = oneshot::channel::<()>();

				send_close = Some(close_s);
				spawn(watch_stdin(errors.clone(), events.clone(), close_r));
			}
			// if we don't want to watch stdin but we are already watching it then send a close signal to end
			// the watching
			(false, Some(_)) => {
				// ignore send error as if channel is closed watch is already gone
				send_close
					.take()
					.expect("unreachable due to match")
					.send(())
					.ok();
			}
			// otherwise no action is required
			_ => {}
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
		select! {
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
