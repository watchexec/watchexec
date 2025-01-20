use std::{
	borrow::Cow,
	collections::HashMap,
	env::var,
	ffi::{OsStr, OsString},
	fs::File,
	io::{IsTerminal, Write},
	process::Stdio,
	sync::{
		atomic::{AtomicBool, AtomicU8, Ordering},
		Arc,
	},
	time::Duration,
};

use clearscreen::ClearScreen;
use miette::{miette, IntoDiagnostic, Report, Result};
use notify_rust::Notification;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};
use tokio::{process::Command as TokioCommand, time::sleep};
use tracing::{debug, debug_span, error, instrument, trace, trace_span, Instrument};
use watchexec::{
	action::ActionHandler,
	command::{Command, Program, Shell, SpawnOptions},
	error::RuntimeError,
	job::{CommandState, Job},
	sources::fs::Watcher,
	Config, ErrorHook, Id,
};
use watchexec_events::{Event, Keyboard, ProcessEnd, Tag};
use watchexec_signals::Signal;

use crate::{
	args::{
		command::WrapMode,
		events::{EmitEvents, OnBusyUpdate, SignalMapping},
		output::{ClearMode, ColourMode},
		Args,
	},
	state::RotatingTempFile,
};
use crate::{emits::events_to_simple_format, state::State};

#[derive(Clone, Copy, Debug)]
struct OutputFlags {
	quiet: bool,
	colour: ColorChoice,
	timings: bool,
	bell: bool,
	toast: bool,
}

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

	config.pathset(args.filtering.paths.clone());

	config.throttle(args.events.debounce.0);
	config.keyboard_events(args.events.stdin_quit);

	if let Some(interval) = args.events.poll {
		config.file_watcher(Watcher::Poll(interval.0));
	}

	let once = args.debugging.once;
	let clear = args.output.screen_clear;

	let emit_events_to = args.events.emit_events_to;
	let emit_file = state.emit_file.clone();

	if args.events.only_emit_events {
		config.on_action(move |mut action| {
			// if we got a terminate or interrupt signal, quit
			if action
				.signals()
				.any(|sig| sig == Signal::Terminate || sig == Signal::Interrupt)
			{
				// no need to be graceful as there's no commands
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
						reset_screen();
					}
				}
			}

			match emit_events_to {
				EmitEvents::Stdio => {
					println!(
						"{}",
						events_to_simple_format(action.events.as_ref()).unwrap_or_default()
					);
				}
				EmitEvents::JsonStdio => {
					for event in action.events.iter().filter(|e| !e.is_empty()) {
						println!("{}", serde_json::to_string(event).unwrap_or_default());
					}
				}
				other => unreachable!(
					"emit_events_to should have been validated earlier: {:?}",
					other
				),
			}

			action
		});

		return Ok(config);
	}

	let delay_run = args.command.delay_run.map(|ts| ts.0);
	let on_busy = args.events.on_busy_update;
	let stdin_quit = args.events.stdin_quit;

	let signal = args.events.signal;
	let stop_signal = args.command.stop_signal;
	let stop_timeout = args.command.stop_timeout.0;

	let print_events = args.debugging.print_events;
	let outflags = OutputFlags {
		quiet: args.output.quiet,
		colour: match args.output.color {
			ColourMode::Auto if !std::io::stdin().is_terminal() => ColorChoice::Never,
			ColourMode::Auto => ColorChoice::Auto,
			ColourMode::Always => ColorChoice::Always,
			ColourMode::Never => ColorChoice::Never,
		},
		timings: args.output.timings,
		bell: args.output.bell,
		toast: args.output.notify,
	};

	let workdir = Arc::new(args.command.workdir.clone());

	let mut add_envs = HashMap::new();
	for pair in &args.command.env {
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

	let signal_map: Arc<HashMap<Signal, Option<Signal>>> = Arc::new(
		args.events
			.signal_map
			.iter()
			.copied()
			.map(|SignalMapping { from, to }| (from, to))
			.collect(),
	);

	let queued = Arc::new(AtomicBool::new(false));
	let quit_again = Arc::new(AtomicU8::new(0));

	config.on_action_async(move |mut action| {
		let add_envs = add_envs.clone();
		let command = command.clone();
		let emit_file = emit_file.clone();
		let queued = queued.clone();
		let quit_again = quit_again.clone();
		let signal_map = signal_map.clone();
		let workdir = workdir.clone();
		Box::new(
			async move {
				trace!(events=?action.events, "handling action");

				let add_envs = add_envs.clone();
				let command = command.clone();
				let emit_file = emit_file.clone();
				let queued = queued.clone();
				let quit_again = quit_again.clone();
				let signal_map = signal_map.clone();
				let workdir = workdir.clone();

				trace!("set spawn hook for workdir and environment variables");
				let job = action.get_or_create_job(id, move || command.clone());
				let events = action.events.clone();
				job.set_spawn_hook(move |command, _| {
					let add_envs = add_envs.clone();
					let emit_file = emit_file.clone();
					let events = events.clone();

					if let Some(ref workdir) = workdir.as_ref() {
						debug!(?workdir, "set command workdir");
						command.command_mut().current_dir(workdir);
					}

					emit_events_to_command(
						command.command_mut(),
						events,
						emit_file,
						emit_events_to,
						add_envs,
					);
				});

				let show_events = {
					let events = action.events.clone();
					move || {
						if print_events {
							trace!("print events to stderr");
							for (n, event) in events.iter().enumerate() {
								eprintln!("[EVENT {n}] {event}");
							}
						}
					}
				};

				let clear_screen = {
					let events = action.events.clone();
					move || {
						if let Some(mode) = clear {
							match mode {
								ClearMode::Clear => {
									clearscreen::clear().ok();
									debug!("cleared screen");
								}
								ClearMode::Reset => {
									reset_screen();
									debug!("hard-reset screen");
								}
							}
						}

						// re-show events after clearing
						if print_events {
							trace!("print events to stderr");
							for (n, event) in events.iter().enumerate() {
								eprintln!("[EVENT {n}] {event}");
							}
						}
					}
				};

				let quit = |mut action: ActionHandler| {
					match quit_again.fetch_add(1, Ordering::Relaxed) {
						0 => {
							eprintln!("[Waiting {stop_timeout:?} for processes to exit before stopping...]");
							// eprintln!("[Waiting {stop_timeout:?} for processes to exit before stopping... Ctrl-C again to exit faster]");
							// see TODO in action/worker.rs
							action.quit_gracefully(
								stop_signal.unwrap_or(Signal::Terminate),
								stop_timeout,
							);
						}
						1 => {
							action.quit_gracefully(Signal::ForceStop, Duration::ZERO);
						}
						_ => {
							action.quit();
						}
					}

					action
				};

				if once {
					debug!("debug mode: run once and quit");
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
					return quit(action);
				}

				let is_keyboard_eof = action
					.events
					.iter()
					.any(|e| e.tags.contains(&Tag::Keyboard(Keyboard::Eof)));
				if stdin_quit && is_keyboard_eof {
					debug!("keyboard EOF, quit");
					show_events();
					return quit(action);
				}

				let signals: Vec<Signal> = action.signals().collect();
				trace!(?signals, "received some signals");

				// if we got a terminate or interrupt signal and they're not mapped, quit
				if (signals.contains(&Signal::Terminate)
					&& !signal_map.contains_key(&Signal::Terminate))
					|| (signals.contains(&Signal::Interrupt)
						&& !signal_map.contains_key(&Signal::Interrupt))
				{
					debug!("unmapped terminate or interrupt signal, quit");
					show_events();
					return quit(action);
				}

				// pass all other signals on
				for signal in signals {
					match signal_map.get(&signal) {
						Some(Some(mapped)) => {
							debug!(?signal, ?mapped, "passing mapped signal");
							job.signal(*mapped);
						}
						Some(None) => {
							debug!(?signal, "discarding signal");
						}
						None => {
							debug!(?signal, "passing signal on");
							job.signal(signal);
						}
					}
				}

				// only filesystem events below here (or empty synthetic events)
				if action.paths().next().is_none()
					&& !action.events.iter().any(watchexec_events::Event::is_empty)
				{
					debug!("no filesystem or synthetic events, skip without doing more");
					show_events();
					return action;
				}

				show_events();

				if let Some(delay) = delay_run {
					trace!("delaying run by sleeping inside the job");
					job.run_async(move |_| {
						Box::new(async move {
							sleep(delay).await;
						})
					});
				}

				trace!("querying job state via run_async");
				job.run_async({
					let job = job.clone();
					move |context| {
						let job = job.clone();
						let is_running = matches!(context.current, CommandState::Running { .. });
						Box::new(async move {
							let innerjob = job.clone();
							if is_running {
								trace!(?on_busy, "job is running, decide what to do");
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
											clear_screen();
											setup_process(
												innerjob.clone(),
												context.command.clone(),
												outflags,
											);
										});
									}
									OnBusyUpdate::Restart => {
										job.restart_with_signal(
											stop_signal.unwrap_or(Signal::Terminate),
											stop_timeout,
										);
										job.run(move |context| {
											clear_screen();
											setup_process(
												innerjob.clone(),
												context.command.clone(),
												outflags,
											);
										});
									}
									OnBusyUpdate::Queue => {
										let job = job.clone();
										let already_queued =
											queued.fetch_or(true, Ordering::SeqCst);
										if already_queued {
											debug!("next start is already queued, do nothing");
										} else {
											debug!("queueing next start of job");
											tokio::spawn({
												let queued = queued.clone();
												async move {
													trace!("waiting for job to finish");
													job.to_wait().await;
													trace!("job finished, starting queued");
													job.start();
													job.run(move |context| {
														clear_screen();
														setup_process(
															innerjob.clone(),
															context.command.clone(),
															outflags,
														);
													})
													.await;
													trace!("resetting queued state");
													queued.store(false, Ordering::SeqCst);
												}
											});
										}
									}
								}
							} else {
								trace!("job is not running, start it");
								job.start();
								job.run(move |context| {
									clear_screen();
									setup_process(
										innerjob.clone(),
										context.command.clone(),
										outflags,
									);
								});
							}
						})
					}
				});

				action
			}
			.instrument(trace_span!("action handler")),
		)
	});

	Ok(config)
}

#[instrument(level = "debug")]
fn interpret_command_args(args: &Args) -> Result<Arc<Command>> {
	let mut cmd = args.command.program.clone();
	assert!(!cmd.is_empty(), "(clap) Bug: command is not present");

	let shell = if args.command.no_shell {
		None
	} else {
		let shell = args.command.shell.clone().or_else(|| var("SHELL").ok());
		match shell
			.as_deref()
			.or_else(|| {
				if cfg!(not(windows)) {
					Some("sh")
				} else if var("POWERSHELL_DISTRIBUTION_CHANNEL").is_ok()
					&& (which::which("pwsh").is_ok() || which::which("pwsh.exe").is_ok())
				{
					trace!("detected pwsh");
					Some("pwsh")
				} else if var("PSModulePath").is_ok()
					&& (which::which("powershell").is_ok()
						|| which::which("powershell.exe").is_ok())
				{
					trace!("detected powershell");
					Some("powershell")
				} else {
					Some("cmd")
				}
			})
			.or(Some("default"))
		{
			Some("") => return Err(RuntimeError::CommandShellEmptyShell).into_diagnostic(),

			Some("none") | None => None,

			#[cfg(windows)]
			Some("cmd") | Some("cmd.exe") | Some("CMD") | Some("CMD.EXE") => Some(Shell::cmd()),

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
			grouped: matches!(args.command.wrap_process, WrapMode::Group),
			session: matches!(args.command.wrap_process, WrapMode::Session),
			..Default::default()
		},
	}))
}

#[instrument(level = "trace")]
fn setup_process(job: Job, command: Arc<Command>, outflags: OutputFlags) {
	if outflags.toast {
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

	if !outflags.quiet {
		let mut stderr = StandardStream::stderr(outflags.colour);
		stderr.reset().ok();
		stderr
			.set_color(ColorSpec::new().set_fg(Some(Color::Green)))
			.ok();
		writeln!(&mut stderr, "[Running: {command}]").ok();
		stderr.reset().ok();
	}

	tokio::spawn(async move {
		job.to_wait().await;
		job.run(move |context| end_of_process(context.current, outflags));
	});
}

#[instrument(level = "trace")]
fn end_of_process(state: &CommandState, outflags: OutputFlags) {
	let CommandState::Finished {
		status,
		started,
		finished,
	} = state
	else {
		return;
	};

	let duration = *finished - *started;
	let timing = if outflags.timings {
		format!(", lasted {duration:?}")
	} else {
		String::new()
	};
	let (msg, fg) = match status {
		ProcessEnd::ExitError(code) => (format!("Command exited with {code}{timing}"), Color::Red),
		ProcessEnd::ExitSignal(sig) => {
			(format!("Command killed by {sig:?}{timing}"), Color::Magenta)
		}
		ProcessEnd::ExitStop(sig) => (format!("Command stopped by {sig:?}{timing}"), Color::Blue),
		ProcessEnd::Continued => (format!("Command continued{timing}"), Color::Cyan),
		ProcessEnd::Exception(ex) => (
			format!("Command ended by exception {ex:#x}{timing}"),
			Color::Yellow,
		),
		ProcessEnd::Success => (format!("Command was successful{timing}"), Color::Green),
	};

	if outflags.toast {
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

	if !outflags.quiet {
		let mut stderr = StandardStream::stderr(outflags.colour);
		stderr.reset().ok();
		stderr.set_color(ColorSpec::new().set_fg(Some(fg))).ok();
		writeln!(&mut stderr, "[{msg}]").ok();
		stderr.reset().ok();
	}

	if outflags.bell {
		let mut stdout = std::io::stdout();
		stdout.write_all(b"\x07").ok();
		stdout.flush().ok();
	}
}

#[instrument(level = "trace")]
fn emit_events_to_command(
	command: &mut TokioCommand,
	events: Arc<[Event]>,
	emit_file: RotatingTempFile,
	emit_events_to: EmitEvents,
	mut add_envs: HashMap<String, OsString>,
) {
	use crate::emits::{emits_to_environment, emits_to_file, emits_to_json_file};

	let mut stdin = None;

	match emit_events_to {
		EmitEvents::Environment => {
			add_envs.extend(emits_to_environment(&events));
		}
		EmitEvents::Stdio => match emits_to_file(&emit_file, &events)
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
		EmitEvents::JsonStdio => match emits_to_json_file(&emit_file, &events)
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

pub fn reset_screen() {
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
