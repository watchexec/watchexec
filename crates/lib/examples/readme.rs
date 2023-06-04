use miette::{IntoDiagnostic, Result};
use watchexec::{
	action::{Action, EventSet, Outcome},
	command::Command,
	config::{InitConfig, RuntimeConfig},
	handler::PrintDebug,
	Watchexec,
};

#[tokio::main]
async fn main() -> Result<()> {
	let mut init = InitConfig::default();
	init.on_error(PrintDebug(std::io::stderr()));

	let mut runtime = RuntimeConfig::default();
	runtime.pathset(["watchexec.conf"]);

	let conf = YourConfigFormat::load_from_file("watchexec.conf")
		.await
		.into_diagnostic()?;
	conf.apply(&mut runtime);

	let mut commands = conf.commands();

	let we = Watchexec::new(init, runtime.clone())?;
	let w = we.clone();

	let c = runtime.clone();
	runtime.on_action(move |action: Action| {
		let mut c = c.clone();
		let w = w.clone();
		let commands: Vec<_> = commands.drain(..).collect();
		async move {
			for commands in commands {
				_ = action.create(commands, EventSet::All).await;
			}

			'fut: {
				for event in action.events.iter() {
					if event.paths().any(|(p, _)| p.ends_with("/watchexec.conf")) {
						let conf = YourConfigFormat::load_from_file("watchexec.conf").await?;

						conf.apply(&mut c);
						let _ = w.reconfigure(c.clone());
						for &sup in action.list() {
							action
								.delete(sup, EventSet::Some(vec![event.clone()]))
								.await;
						}
						for commands in conf.commands() {
							_ = action
								.create(commands, EventSet::Some(vec![event.clone()]))
								.await;
						}
						// tada! self-reconfiguring watchexec on config file change!

						break 'fut Ok::<(), std::io::Error>(());
					}
				}

				for &sup in action.list() {
					action
						.apply(
							Outcome::if_running(
								Outcome::DoNothing,
								Outcome::both(Outcome::Clear, Outcome::Start),
							),
							sup,
							EventSet::All,
						)
						.await;
				}

				Ok::<(), std::io::Error>(())
			}
		}
	});

	let _ = we.main().await.into_diagnostic()?;
	Ok(())
}

struct YourConfigFormat;
impl YourConfigFormat {
	async fn load_from_file(_path: impl AsRef<std::path::Path>) -> std::io::Result<Self> {
		Ok(Self)
	}

	fn apply(&self, _config: &mut RuntimeConfig) {
		// ...
	}

	// TODO(Felix) this was added to deal with the new api for creating commands/supervisors.
	// Is this along the lines of what you would like this to be, or is it too clunky?
	fn commands(&self) -> Vec<Vec<Command>> {
		#[allow(unused_mut)]
		let mut commands = vec![];
		// ...
		commands
	}
}
