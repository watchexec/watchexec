use std::marker::PhantomData;

use miette::miette;
use tokio::{
	sync::{mpsc, oneshot},
	task::{block_in_place, spawn_blocking},
};
use tracing::{error, trace, warn};
use watchexec::error::RuntimeError;
use watchexec_events::Event;

use crate::args::Args;

const BUFFER: usize = 128;

#[derive(Debug)]
pub struct FilterProgs {
	channel: Requester<Event, bool>,
}

#[derive(Debug, Clone)]
pub struct Requester<S, R> {
	sender: mpsc::Sender<(S, oneshot::Sender<R>)>,
	_receiver: PhantomData<R>,
}

impl<S, R> Requester<S, R>
where
	S: Send + Sync,
	R: Send + Sync,
{
	pub fn new(capacity: usize) -> (Self, mpsc::Receiver<(S, oneshot::Sender<R>)>) {
		let (sender, receiver) = mpsc::channel(capacity);
		(
			Self {
				sender,
				_receiver: PhantomData,
			},
			receiver,
		)
	}

	pub fn call(&self, value: S) -> Result<R, RuntimeError> {
		// FIXME: this should really be async with a timeout, but that needs filtering in general
		// to be async, which should be done at some point
		block_in_place(|| {
			let (sender, receiver) = oneshot::channel();
			self.sender.blocking_send((value, sender)).map_err(|err| {
				RuntimeError::External(miette!("filter progs internal channel: {}", err).into())
			})?;
			receiver
				.blocking_recv()
				.map_err(|err| RuntimeError::External(Box::new(err)))
		})
	}
}

impl FilterProgs {
	pub fn check(&self, event: &Event) -> Result<bool, RuntimeError> {
		self.channel.call(event.clone())
	}

	pub fn new(args: &Args) -> miette::Result<Self> {
		let progs = args.filter_programs_parsed.clone();
		let (requester, mut receiver) = Requester::<Event, bool>::new(BUFFER);
		let task = spawn_blocking(move || {
			'chan: while let Some((event, sender)) = receiver.blocking_recv() {
				for (n, prog) in progs.iter().enumerate() {
					trace!(?n, "trying filter program");
					match prog.run(&event) {
						Ok(false) => {
							trace!(
								?n,
								verdict = false,
								"filter program finished; fail so stopping there"
							);
							sender
								.send(false)
								.unwrap_or_else(|_| warn!("failed to send filter result"));
							continue 'chan;
						}
						Ok(true) => {
							trace!(
								?n,
								verdict = true,
								"filter program finished; pass so trying next"
							);
							continue;
						}
						Err(err) => {
							error!(?n, error=%err, "filter program failed, so trying next");
							continue;
						}
					}
				}

				trace!("all filters failed, sending pass as default");
				sender
					.send(true)
					.unwrap_or_else(|_| warn!("failed to send filter result"));
			}

			Ok(()) as miette::Result<()>
		});

		tokio::spawn(async {
			match task.await {
				Ok(Ok(())) => {}
				Ok(Err(err)) => error!("filter progs task failed: {}", err),
				Err(err) => error!("filter progs task panicked: {}", err),
			}
		});

		Ok(Self { channel: requester })
	}
}
