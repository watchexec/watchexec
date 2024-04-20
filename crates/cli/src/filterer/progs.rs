use std::{iter::empty, marker::PhantomData};

use jaq_interpret::{Ctx, FilterT, RcIter, Val};
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
		eprintln!(
			"EXPERIMENTAL: filter programs are unstable and may change/vanish without notice"
		);

		let (requester, mut receiver) = Requester::<Event, bool>::new(BUFFER);
		let task =
			spawn_blocking(move || {
				'chan: while let Some((event, sender)) = receiver.blocking_recv() {
					let val = serde_json::to_value(&event)
						.map_err(|err| miette!("failed to serialize event: {}", err))
						.map(Val::from)?;

					for (n, prog) in progs.iter().enumerate() {
						trace!(?n, "trying filter program");
						let mut jaq = super::proglib::jaq_lib()?;
						let filter = jaq.compile(prog.clone());
						if !jaq.errs.is_empty() {
							for (error, span) in jaq.errs {
								error!(%error, "failed to compile filter program #{n}@{}:{}", span.start, span.end);
							}
							continue;
						}

						let inputs = RcIter::new(empty());
						let mut results = filter.run((Ctx::new([], &inputs), val.clone()));
						if let Some(res) = results.next() {
							match res {
								Ok(Val::Bool(false)) => {
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
								Ok(Val::Bool(true)) => {
									trace!(
										?n,
										verdict = true,
										"filter program finished; pass so trying next"
									);
									continue;
								}
								Ok(val) => {
									error!(?n, ?val, "filter program returned non-boolean, ignoring and trying next");
									continue;
								}
								Err(err) => {
									error!(?n, error=%err, "filter program failed, so trying next");
									continue;
								}
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
