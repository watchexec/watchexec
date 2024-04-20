use std::{
	sync::{Arc, Mutex},
	time::Duration,
};

use miette::{IntoDiagnostic, Result};
use watchexec::{
	command::{Command, Program, Shell},
	job::CommandState,
	Watchexec,
};
use watchexec_events::{Event, Priority};
use watchexec_signals::Signal;

#[tokio::main]
async fn main() -> Result<()> {
	// this is okay to start with, but Watchexec logs a LOT of data,
	// even at error level. you will quickly want to filter it down.
	tracing_subscriber::fmt()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.init();

	// initialise Watchexec with a simple initial action handler
	let job = Arc::new(Mutex::new(None));
	let wx = Watchexec::new({
		let outerjob = job.clone();
		move |mut action| {
			let (_, job) = action.create_job(Arc::new(Command {
				program: Program::Shell {
					shell: Shell::new("bash"),
					command: "
						echo 'Hello world'
						trap 'echo Not quitting yet!' TERM
						read
					"
					.into(),
					args: Vec::new(),
				},
				options: Default::default(),
			}));

			// store the job outside this closure too
			*outerjob.lock().unwrap() = Some(job.clone());

			// block SIGINT
			#[cfg(unix)]
			job.set_spawn_hook(|cmd, _| {
				use nix::sys::signal::{sigprocmask, SigSet, SigmaskHow, Signal};
				unsafe {
					cmd.command_mut().pre_exec(|| {
						let mut newset = SigSet::empty();
						newset.add(Signal::SIGINT);
						sigprocmask(SigmaskHow::SIG_BLOCK, Some(&newset), None)?;
						Ok(())
					});
				}
			});

			// start the command
			job.start();

			action
		}
	})?;

	// start the engine
	let main = wx.main();

	// send an event to start
	wx.send_event(Event::default(), Priority::Urgent)
		.await
		.unwrap();
	// ^ this will cause the action handler we've defined above to run,
	//   creating and starting our little bash program, and storing it in the mutex

	// spin until we've got the job
	while job.lock().unwrap().is_none() {
		tokio::task::yield_now().await;
	}

	// watch the job and restart it when it exits
	let job = job.lock().unwrap().clone().unwrap();
	let auto_restart = tokio::spawn(async move {
		loop {
			job.to_wait().await;
			job.run(|context| {
				if let CommandState::Finished {
					status,
					started,
					finished,
				} = context.current
				{
					let duration = *finished - *started;
					eprintln!("[Program stopped with {status:?}; ran for {duration:?}]")
				}
			})
			.await;

			eprintln!("[Restarting...]");
			job.start().await;
		}
	});

	// now we change what the action does:
	let auto_restart_abort = auto_restart.abort_handle();
	wx.config.on_action(move |mut action| {
		// if we get Ctrl-C on the Watchexec instance, we quit
		if action.signals().any(|sig| sig == Signal::Interrupt) {
			eprintln!("[Quitting...]");
			auto_restart_abort.abort();
			action.quit_gracefully(Signal::ForceStop, Duration::ZERO);
			return action;
		}

		// if the action was triggered by file events, gracefully stop the program
		if action.paths().next().is_some() {
			// watchexec can manage ("supervise") more than one program;
			// here we only have one but we don't know its Id so we grab it out of the iterator
			if let Some(job) = action.list_jobs().next().map(|(_, job)| job.clone()) {
				eprintln!("[Asking program to stop...]");
				job.stop_with_signal(Signal::Terminate, Duration::from_secs(5));
			}

			// we could also use `action.get_or_create_job` initially and store its Id to use here,
			// see the CHANGELOG.md for an example under "3.0.0 > Action".
		}

		action
	});

	// and watch all files in the current directory:
	wx.config.pathset(["."]);

	// then keep running until Watchexec quits!
	let _ = main.await.into_diagnostic()?;
	auto_restart.abort();
	Ok(())
}
