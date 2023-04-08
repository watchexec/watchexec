use std::{iter, marker::PhantomData};

use jaq_core::{
	parse::{self, filter::Filter, Def},
	Ctx, Definitions, RcIter, Val,
};
use miette::miette;
use tokio::{
	sync::{mpsc, oneshot},
	task::{block_in_place, spawn_blocking},
};
use tracing::{debug, error, trace, warn};
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
		let n_filters = args.filter_programs.len();
		let progs = args.filter_programs.clone();
		warn!("EXPERIMENTAL: filter programs are unstable and may change/vanish without notice");

		let (requester, mut receiver) = Requester::<Event, bool>::new(BUFFER);
		let task =
			spawn_blocking(move || {
				let mut defs = load_std_defs()?;
				load_watchexec_defs(&mut defs)?;
				load_user_progs(&mut defs, &progs)?;

				'chan: while let Some((event, sender)) = receiver.blocking_recv() {
					let val = serde_json::to_value(&event)
						.map_err(|err| miette!("failed to serialize event: {}", err))
						.map(Val::from)?;

					for n in 0..n_filters {
						trace!(?n, "trying filter program");

						let name = format!("__watchexec_filter_{n}");
						let filter = Filter::Call(name, Vec::new());
						let mut errs = Vec::new();
						let filter = defs.clone().finish(
							(Vec::new(), (filter, 0..0)),
							Vec::new(),
							&mut errs,
						);
						if !errs.is_empty() {
							error!(?errs, "failed to load filter program #{}", n);
							continue;
						}

						let inputs = RcIter::new(iter::once(Ok(val.clone())));
						let ctx = Ctx::new(Vec::new(), &inputs);
						let mut results = filter.run(ctx, val.clone());
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
									error!(?n, ?err, "filter program failed, so trying next");
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

fn load_std_defs() -> miette::Result<Definitions> {
	debug!("loading jaq core library");
	let mut defs = Definitions::core();

	debug!("loading jaq standard library");
	let mut errs = Vec::new();
	jaq_std::std()
		.into_iter()
		.for_each(|def| defs.insert(def, &mut errs));

	if !errs.is_empty() {
		return Err(miette!("failed to load jaq standard library: {:?}", errs));
	}
	Ok(defs)
}

fn load_watchexec_defs(defs: &mut Definitions) -> miette::Result<()> {
	debug!("loading jaq watchexec library");
	Ok(())
}

fn load_user_progs(all_defs: &mut Definitions, progs: &[String]) -> miette::Result<()> {
	debug!("loading jaq programs");
	for (n, prog) in progs.iter().enumerate() {
		trace!(?n, ?prog, "loading filter program");
		let (main, mut errs) = parse::parse(prog, parse::main());

		if let Some((defs, filter)) = main {
			let name = format!("__watchexec_filter_{}", n);
			trace!(?filter, ?name, "loading filter program into global as def");
			all_defs.insert(
				Def {
					name,
					args: Vec::new(),
					body: filter,
					defs,
				},
				&mut errs,
			);
		}

		if !errs.is_empty() {
			return Err(miette!("failed to load filter program #{}: {:?}", n, errs));
		}
	}

	Ok(())
}
