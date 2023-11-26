use std::{
	borrow::Cow,
	collections::HashMap,
	env::current_dir,
	ffi::{OsStr, OsString},
	fs::File,
	path::Path,
	process::Stdio,
	sync::Arc,
};

use clearscreen::ClearScreen;
use miette::{miette, IntoDiagnostic, Report, Result};
use notify_rust::Notification;
use tokio::{process::Command as TokioCommand, time::sleep};
use tracing::{debug, debug_span, error};
use watchexec::{
	command::{Command, Program, Shell, SpawnOptions},
	error::RuntimeError,
	job::{CommandState, Job},
	sources::fs::Watcher,
	Config, ErrorHook, Id,
};
use watchexec_events::{Event, Keyboard, ProcessEnd, Tag};
use watchexec_signals::Signal;

use crate::{state::State, emits::events_to_simple_format};
use crate::{
	args::{Args, ClearMode, EmitEvents, OnBusyUpdate},
	state::RotatingTempFile,
};

pub fn make_config(args: &Args, state: &State) -> Result<Config> {
	let _span = debug_span!("args-runtime").entered();
	let config = Config::default();
	config.on_error(|err: ErrorHook| {
		if let RuntimeError::IoError {
			about: "waiting on process group",
			..
		} = err.error
		{
			// "No child processes" and such
			// these are often spurious, so condemn them to -v only
			error!("{}", err.error);
			return;
		}

		if cfg!(debug_assertions) {
			eprintln!("[[{:?}]]", err.error);
		}

		eprintln!("[[Error (not fatal)]]\n{}", Report::new(err.error));
	});

	config.pathset(if args.paths.is_empty() {
		vec![current_dir().into_diagnostic()?]
	} else if args.paths.len() == 1
		&& args
			.paths
			.first()
			.map_or(false, |p| p == Path::new("/dev/null"))
	{
		// special case: /dev/null means "don't start the fs event source"
		Vec::new()
	} else {
		args.paths.clone()
	});

	config.throttle(args.debounce.0);
	config.keyboard_events(args.stdin_quit);

	if let Some(interval) = args.poll {
		config.file_watcher(Watcher::Poll(interval.0));
	}

	let once = args.once;
	let clear = args.screen_clear;

	let emit_events_to = args.emit_events_to;
	let emit_file = state.emit_file.clone();

	if args.only_emit_events {
		config.on_action(move |mut action| {
			// if we got a terminate or interrupt signal, quit
			if action.signals().any(|sig| sig == Signal::Terminate || sig == Signal::Interrupt) {
				action.quit();
				return action;
			}

			// clear the screen before printing events
			if let Some(mode) = clear {
				match mode {
					ClearMode::Clear => {
						clearscreen::clear().ok();
					}
					ClearMode::Reset => {
						for cs in [
							ClearScreen::WindowsCooked,
							ClearScreen::WindowsVt,
							ClearScreen::VtLeaveAlt,
							ClearScreen::VtWellDone,
							ClearScreen::default(),
						] {
							cs.clear().ok();
						}
					}
				}
			}

			match emit_events_to {
				EmitEvents::Stdin => {
					println!("{}", events_to_simple_format(action.events.as_ref()).unwrap_or_default());
				}
				EmitEvents::JsonStdin => {
					for event in action.events.iter().filter(|e| !e.is_empty()) {
						println!("{}", serde_json::to_string(event).unwrap_or_default());
					}
				}
				other => unreachable!("emit_events_to should have been validated earlier: {:?}", other),
			}

			action
		});

		return Ok(config);
	}

	let delay_run = args.delay_run.map(|ts| ts.0);
	let on_busy = args.on_busy_update;

	let signal = args.signal;
	let stop_signal = args.stop_signal;
	let stop_timeout = args.stop_timeout.0;

	let notif = args.notify;
	let print_events = args.print_events;

	let workdir = Arc::new(args.workdir.clone());

	let mut add_envs = HashMap::new();
	for pair in &args.env {
		if let Some((k, v)) = pair.split_once('=') {
			add_envs.insert(k.to_owned(), OsString::from(v));
		} else {
			return Err(miette!("{pair} is not in key=value format"));
		}
	}
	debug!(
		?add_envs,
		"additional environment variables to add to command"
	);

	let id = Id::default();
	let command = interpret_command_args(args)?;

	config.on_action_async(move |mut action| {
		let add_envs = add_envs.clone();
		let command = command.clone();
		let emit_file = emit_file.clone();
		let workdir = workdir.clone();
		Box::new(async move {
			let add_envs = add_envs.clone();
			let command = command.clone();
			let emit_file = emit_file.clone();
			let workdir = workdir.clone();

			let job = action.get_or_create_job(id, move || command.clone());
			let events = action.events.clone();
			job.set_spawn_hook(move |command, _| {
				let add_envs = add_envs.clone();
				let emit_file = emit_file.clone();
				let events = events.clone();

				if let Some(ref workdir) = workdir.as_ref() {
					debug!(?workdir, "set command workdir");
					command.current_dir(workdir);
				}

				emit_events_to_command(command, events, emit_file, emit_events_to, add_envs);
			});

			let show_events = || {
				if print_events {
					for (n, event) in action.events.iter().enumerate() {
						eprintln!("[EVENT {n}] {event}");
					}
				}
			};

			if once {
				show_events();

				if let Some(delay) = delay_run {
					job.run_async(move |_| {
						Box::new(async move {
							sleep(delay).await;
						})
					});
				}

				// this blocks the event loop, but also this is a debug feature so i don't care
				job.start().await;
				job.to_wait().await;
				action.quit();
				return action;
			}

			let is_keyboard_eof = action
				.events
				.iter()
				.any(|e| e.tags.contains(&Tag::Keyboard(Keyboard::Eof)));
			if is_keyboard_eof {
				show_events();
				action.quit();
				return action;
			}

			let signals: Vec<Signal> = action.signals().collect();

			// if we got a terminate or interrupt signal, quit
			if signals.contains(&Signal::Terminate) || signals.contains(&Signal::Interrupt) {
				show_events();
				action.quit();
				return action;
			}

			// pass all other signals on
			for signal in signals {
				job.signal(signal);
			}

			// clear the screen before printing events
			if let Some(mode) = clear {
				match mode {
					ClearMode::Clear => {
						clearscreen::clear().ok();
					}
					ClearMode::Reset => {
						for cs in [
							ClearScreen::WindowsCooked,
							ClearScreen::WindowsVt,
							ClearScreen::VtLeaveAlt,
							ClearScreen::VtWellDone,
							ClearScreen::default(),
						] {
							cs.clear().ok();
						}
					}
				}
			}

			show_events();

			if let Some(delay) = delay_run {
				job.run_async(move |_| {
					Box::new(async move {
						sleep(delay).await;
					})
				});
			}

			job.run_async({
				let job = job.clone();
				move |context| {
					let job = job.clone();
					let is_running = matches!(context.current, CommandState::Running { .. });
					Box::new(async move {
						let innerjob = job.clone();
						if is_running {
							match on_busy {
								OnBusyUpdate::DoNothing => {}
								OnBusyUpdate::Signal => {
									job.signal(if cfg!(windows) {
										Signal::ForceStop
									} else {
										stop_signal.or(signal).unwrap_or(Signal::Terminate)
									});
								}
								OnBusyUpdate::Restart if cfg!(windows) => {
									job.restart();
									job.run(move |context| {
										setup_process(
											innerjob.clone(),
											context.command.clone(),
											notif,
										)
									});
								}
								OnBusyUpdate::Restart => {
									job.restart_with_signal(
										stop_signal.unwrap_or(Signal::Terminate),
										stop_timeout,
									);
									job.run(move |context| {
										setup_process(
											innerjob.clone(),
											context.command.clone(),
											notif,
										)
									});
								}
								OnBusyUpdate::Queue => {
									let job = job.clone();
									tokio::spawn(async move {
										job.to_wait().await;
										job.start();
										job.run(move |context| {
											setup_process(
												innerjob.clone(),
												context.command.clone(),
												notif,
											)
										});
									});
								}
							}
						} else {
							job.start();
							job.run(move |context| {
								setup_process(innerjob.clone(), context.command.clone(), notif)
							});
						}
					})
				}
			});

			action
		})
	});

	Ok(config)
}

fn interpret_command_args(args: &Args) -> Result<Arc<Command>> {
	let mut cmd = args.command.clone();
	if cmd.is_empty() {
		panic!("(clap) Bug: command is not present");
	}

	let shell = match if args.no_shell || args.no_shell_long {
		None
	} else {
		args.shell.as_deref().or(Some("default"))
	} {
		Some("") => return Err(RuntimeError::CommandShellEmptyShell).into_diagnostic(),

		Some("none") | None => None,

		#[cfg(windows)]
		Some("default") | Some("cmd") | Some("cmd.exe") | Some("CMD") | Some("CMD.EXE") => {
			Some(Shell::cmd())
		}

		#[cfg(not(windows))]
		Some("default") => Some(Shell::new("sh")),

		#[cfg(windows)]
		Some("powershell") => Some(Shell::new(available_powershell())),

		Some(other) => {
			let sh = other.split_ascii_whitespace().collect::<Vec<_>>();

			// UNWRAP: checked by Some("")
			#[allow(clippy::unwrap_used)]
			let (shprog, shopts) = sh.split_first().unwrap();

			Some(Shell {
				prog: shprog.into(),
				options: shopts.iter().map(|s| (*s).to_string()).collect(),
				program_option: Some(Cow::Borrowed(OsStr::new("-c"))),
			})
		}
	};

	let program = if let Some(shell) = shell {
		Program::Shell {
			shell,
			command: cmd.join(" "),
			args: Vec::new(),
		}
	} else {
		Program::Exec {
			prog: cmd.remove(0).into(),
			args: cmd,
		}
	};

	Ok(Arc::new(Command {
		program,
		options: SpawnOptions {
			grouped: !args.no_process_group,
			..Default::default()
		},
	}))
}

#[cfg(windows)]
fn available_powershell() -> String {
	todo!("figure out if powershell.exe is available, and use that, otherwise use pwsh.exe")
}

fn setup_process(job: Job, command: Arc<Command>, notif: bool) {
	if notif {
		Notification::new()
			.summary("Watchexec: change detected")
			.body(&format!("Running {command}"))
			.show()
			.map_or_else(
				|err| {
					eprintln!("[[Failed to send desktop notification: {err}]]");
				},
				drop,
			);
	}

	tokio::spawn(async move {
		job.to_wait().await;
		job.run(move |context| end_of_process(context.current, notif));
	});
}

fn end_of_process(state: &CommandState, notif: bool) {
	let CommandState::Finished {
		status,
		started,
		finished,
	} = state
	else {
		return;
	};

	let duration = *finished - *started;
	let msg = match status {
		ProcessEnd::ExitError(code) => {
			format!("Command exited with {code}, lasted {duration:?}")
		}
		ProcessEnd::ExitSignal(sig) => {
			format!("Command killed by {sig:?}, lasted {duration:?}")
		}
		ProcessEnd::ExitStop(sig) => {
			format!("Command stopped by {sig:?}, lasted {duration:?}")
		}
		ProcessEnd::Continued => format!("Command continued, lasted {duration:?}"),
		ProcessEnd::Exception(ex) => {
			format!("Command ended by exception {ex:#x}, lasted {duration:?}")
		}
		ProcessEnd::Success => format!("Command was successful, lasted {duration:?}"),
	};

	if notif {
		Notification::new()
			.summary("Watchexec: command ended")
			.body(&msg)
			.show()
			.map_or_else(
				|err| {
					eprintln!("[[Failed to send desktop notification: {err}]]");
				},
				drop,
			);
	}
}

fn emit_events_to_command(
	command: &mut TokioCommand,
	events: Arc<[Event]>,
	emit_file: RotatingTempFile,
	emit_events_to: EmitEvents,
	mut add_envs: HashMap<String, OsString>,
) {
	use crate::emits::*;

	let mut stdin = None;

	match emit_events_to {
		EmitEvents::Environment => {
			add_envs.extend(emits_to_environment(&events));
		}
		EmitEvents::Stdin => match emits_to_file(&emit_file, &events)
			.and_then(|path| File::open(path).into_diagnostic())
		{
			Ok(file) => {
				stdin.replace(Stdio::from(file));
			}
			Err(err) => {
				error!("Failed to write events to stdin, continuing without it: {err}");
			}
		},
		EmitEvents::File => match emits_to_file(&emit_file, &events) {
			Ok(path) => {
				add_envs.insert("WATCHEXEC_EVENTS_FILE".into(), path.into());
			}
			Err(err) => {
				error!("Failed to write WATCHEXEC_EVENTS_FILE, continuing without it: {err}");
			}
		},
		EmitEvents::JsonStdin => match emits_to_json_file(&emit_file, &events)
			.and_then(|path| File::open(path).into_diagnostic())
		{
			Ok(file) => {
				stdin.replace(Stdio::from(file));
			}
			Err(err) => {
				error!("Failed to write events to stdin, continuing without it: {err}");
			}
		},
		EmitEvents::JsonFile => match emits_to_json_file(&emit_file, &events) {
			Ok(path) => {
				add_envs.insert("WATCHEXEC_EVENTS_FILE".into(), path.into());
			}
			Err(err) => {
				error!("Failed to write WATCHEXEC_EVENTS_FILE, continuing without it: {err}");
			}
		},
		EmitEvents::None => {}
	}

	for (k, v) in add_envs {
		debug!(?k, ?v, "inserting environment variable");
		command.env(k, v);
	}

	if let Some(stdin) = stdin {
		debug!("set command stdin");
		command.stdin(stdin);
	}
}
