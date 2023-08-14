use std::{
	collections::{HashMap, HashSet},
	path::PathBuf,
	sync::{Arc, Mutex},
	time::Duration,
};

use miette::{IntoDiagnostic, Result};
use watchexec::{
	action::{Action, EventSet, Outcome},
	command::{Program, Shell, SupervisorId},
	fs::Watcher,
	Watchexec,
};
use watchexec_signals::Signal;

// Run with: `env RUST_LOG=debug cargo run --example print_out`
#[tokio::main]
async fn main() -> Result<()> {
	tracing_subscriber::fmt::init();

	let known_commands: Arc<Mutex<HashMap<PathBuf, SupervisorId>>> = Default::default();

	let wx = Watchexec::default();
	let config = wx.config.clone();
	wx.config.pathset(["src", "examples"]);
	wx.config.on_action({
		move |action: Action| {
			let known_commands = known_commands.clone();
			eprintln!("Watchexec Action: {action:?}");

			// Signal handling: quit on SIGINT and switch backend on USR1/USR2
			if action.signals().any(|sig| sig == Signal::Interrupt) {
				action.quit();
				return;
			} else if action.signals().any(|sig| sig == Signal::User1) {
				eprintln!("Switching to native for funsies");
				config.file_watcher(Watcher::Native);
			} else if action.signals().any(|sig| sig == Signal::User2) {
				eprintln!("Switching to polling for funsies");
				config.file_watcher(Watcher::Poll(Duration::from_millis(50)));
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
		}
	});

	wx.main().await.into_diagnostic()??;

	Ok(())
}
