use async_priority_channel as priority;
use tokio::{io::AsyncBufReadExt, sync::mpsc};
use tracing::trace;

use crate::{
	error::{CriticalError, RuntimeError},
	event::{Event, Priority, Source, Tag},
};

#[derive(Debug, Clone, Default)]
pub struct WorkingData {
	pub eof: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Keyboard {
	Eof,
}

pub async fn worker(
	errors: mpsc::Sender<RuntimeError>,
	events: priority::Sender<Event, Priority>,
) -> Result<(), CriticalError> {
	imp_worker(errors, events).await
}

async fn imp_worker(
	errors: mpsc::Sender<RuntimeError>,
	events: priority::Sender<Event, Priority>,
) -> Result<(), CriticalError> {
	let stdin = tokio::io::stdin();
	let mut reader = tokio::io::BufReader::new(stdin);

	// Keep reading lines from stdin and handle contents
	loop {
		let mut input = String::new();
		match reader.read_line(&mut input).await {
			Ok(0) => {
				// Zero bytes read - represents end of stream so we exit
				send_event(errors.clone(), events.clone(), Keyboard::Eof).await?;
				break;
			}
			Err(_) => {
				break;
			}
			_ => {
				// Ignore unexpected input on stdin
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
