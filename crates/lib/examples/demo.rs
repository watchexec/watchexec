use std::{
	collections::{HashMap, HashSet},
	convert::Infallible,
	path::PathBuf,
	sync::{Arc, Mutex},
	time::Duration,
};

use miette::{IntoDiagnostic, Result};
use watchexec::{
	action::{Action, EventSet, Outcome},
	command::{Program, Shell, SupervisorId},
	config::{InitConfig, RuntimeConfig},
	fs::Watcher,
	handler::sync,
	Watchexec,
};
use watchexec_signals::Signal;

// Run with: `env RUST_LOG=debug cargo run --example print_out`
#[tokio::main]
async fn main() -> Result<()> {
	tracing_subscriber::fmt::init();

	let mut runtime = RuntimeConfig::default();
	runtime.pathset(["src", "dontexist", "examples"]);

	let wx = Watchexec::new(InitConfig::default(), runtime.clone())?;
	let w = wx.clone();

	let config = runtime.clone();
	let known_commands: Arc<Mutex<HashMap<PathBuf, SupervisorId>>> = Default::default();

	runtime.on_action(sync(move |action: Action| -> Result<(), Infallible> {
		let mut config = config.clone();
		let w = w.clone();
		let known_commands = known_commands.clone();
		eprintln!("Watchexec Action: {action:?}");

		// Signal handling: quit on SIGINT and switch backend on USR1/USR2
		if action.signals().any(|sig| sig == Signal::Interrupt) {
			action.quit();
			return Result::Ok(());
		} else if action.signals().any(|sig| sig == Signal::User1) {
			eprintln!("Switching to native for funsies");
			config.file_watcher(Watcher::Native);
			w.reconfigure(config).unwrap();
		} else if action.signals().any(|sig| sig == Signal::User2) {
			eprintln!("Switching to polling for funsies");
			config.file_watcher(Watcher::Poll(Duration::from_millis(50)));
			w.reconfigure(config).unwrap();
		}

		// We're going to spawn one call to the program per file changed, passing the path that
		// changed to that program, but only if we're not already running a call for that path.
		let paths_affected: HashSet<PathBuf> =
			action.paths().map(|(path, _)| path.into()).collect();
		for path in paths_affected {
			if let Some(id) = known_commands.lock().unwrap().get(&path) {
				// If we know of a program for that path that might be already running, tell it
				// to either keep running or, if it's not running, start again.
				action.apply(
					*id,
					Outcome::if_running(Outcome::DoNothing, Outcome::Start),
					EventSet::All,
				);
			} else {
				let id = action.create(
					Program::Shell {
						shell: Shell::new(if cfg!(windows) {
							"powershell.exe"
						} else {
							"bash"
						}),
						command: if cfg!(windows) { "Get-ChildItem" } else { "ls" }.into(),
						args: vec![path.display().to_string()],
					}
					.into(),
				);
				action.apply(id, Outcome::Start, EventSet::All);
				known_commands.lock().unwrap().insert(path, id);
			}
		}

		// Drop any SupervisorId *we* know of but Watchexec doesn't
		known_commands
			.lock()
			.unwrap()
			.retain(|_, id| action.supervisors.contains(id));

		Ok(())
	}));

	wx.reconfigure(runtime)?;
	wx.main().await.into_diagnostic()??;

	Ok(())
}
