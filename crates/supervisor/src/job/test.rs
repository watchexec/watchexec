use std::{
	num::NonZeroI64,
	process::{ExitStatus, Output},
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc, Mutex,
	},
};

use tokio::task::JoinSet;
use watchexec_events::ProcessEnd;

use crate::{
	command::{Command, Program},
	job::{start_job, CommandState, TestChildCall},
};

use super::{Control, Job, Priority, TestChild};

fn erroring_command() -> Command {
	Command {
		program: Program::Exec {
			prog: "/does/not/exist".into(),
			args: Vec::new(),
		},
		grouped: true,
	}
}

fn working_command() -> Command {
	Command {
		program: Program::Exec {
			prog: "/does/not/run".into(),
			args: Vec::new(),
		},
		grouped: true,
	}
}

#[tokio::test]
async fn sync_error_handler() {
	let mut joinset = JoinSet::new();
	let job = start_job(&mut joinset, erroring_command());
	let error_handler_called = Arc::new(AtomicBool::new(false));

	job.set_error_handler({
		let error_handler_called = error_handler_called.clone();
		move |_| {
			error_handler_called.store(true, Ordering::Relaxed);
		}
	})
	.await;

	job.start().await;

	assert!(
		error_handler_called.load(Ordering::Relaxed),
		"called on start"
	);

	joinset.abort_all();
}

#[tokio::test]
async fn async_error_handler() {
	let mut joinset = JoinSet::new();
	let job = start_job(&mut joinset, erroring_command());
	let error_handler_called = Arc::new(AtomicBool::new(false));

	job.set_async_error_handler({
		let error_handler_called = error_handler_called.clone();
		move |_| {
			let error_handler_called = error_handler_called.clone();
			Box::new(async move {
				error_handler_called.store(true, Ordering::Relaxed);
			})
		}
	})
	.await;

	job.start().await;

	assert!(
		error_handler_called.load(Ordering::Relaxed),
		"called on start"
	);

	joinset.abort_all();
}

#[tokio::test]
async fn unset_error_handler() {
	let mut joinset = JoinSet::new();
	let job = start_job(&mut joinset, erroring_command());
	let error_handler_called = Arc::new(AtomicBool::new(false));

	job.set_error_handler({
		let error_handler_called = error_handler_called.clone();
		move |_| {
			error_handler_called.store(true, Ordering::Relaxed);
		}
	})
	.await;

	job.unset_error_handler().await;

	job.start().await;

	assert!(
		!error_handler_called.load(Ordering::Relaxed),
		"not called even after start"
	);

	joinset.abort_all();
}

#[tokio::test]
async fn queue_ordering() {
	let mut joinset = JoinSet::new();
	let job = start_job(&mut joinset, working_command());
	let error_handler_called = Arc::new(AtomicBool::new(false));

	job.set_error_handler({
		let error_handler_called = error_handler_called.clone();
		move |_| {
			error_handler_called.store(true, Ordering::Relaxed);
		}
	});

	job.unset_error_handler();

	// We're not awaiting until this one, but because the queue is processed in
	// order, it's effectively the same as waiting them all.
	job.start().await;

	assert!(
		!error_handler_called.load(Ordering::Relaxed),
		"called after queue await"
	);

	joinset.abort_all();
}

#[tokio::test]
async fn sync_func() {
	let mut joinset = JoinSet::new();
	let job = start_job(&mut joinset, working_command());
	let func_called = Arc::new(AtomicBool::new(false));

	let ticket = job.run({
		let func_called = func_called.clone();
		move |_| {
			func_called.store(true, Ordering::Relaxed);
		}
	});

	assert!(
		!func_called.load(Ordering::Relaxed),
		"immediately after submit, likely before processed"
	);

	ticket.await;
	assert!(
		func_called.load(Ordering::Relaxed),
		"after it's been processed"
	);

	joinset.abort_all();
}

#[tokio::test]
async fn async_func() {
	let mut joinset = JoinSet::new();
	let job = start_job(&mut joinset, working_command());
	let func_called = Arc::new(AtomicBool::new(false));

	let ticket = job.run_async({
		let func_called = func_called.clone();
		move |_| {
			let func_called = func_called.clone();
			Box::new(async move {
				func_called.store(true, Ordering::Relaxed);
			})
		}
	});

	assert!(
		!func_called.load(Ordering::Relaxed),
		"immediately after submit, likely before processed"
	);

	ticket.await;
	assert!(
		func_called.load(Ordering::Relaxed),
		"after it's been processed"
	);

	joinset.abort_all();
}

// TODO: figure out how to test spawn hooks

async fn refresh_state(job: &Job, state: &Arc<Mutex<Option<CommandState>>>, current: bool) {
	job.send_controls(
		[Control::SyncFunc(Box::new({
			let state = state.clone();
			move |context| {
				if current {
					state.lock().unwrap().replace(context.current.clone());
				} else {
					*state.lock().unwrap() = context.previous.cloned();
				}
			}
		}))],
		Priority::Urgent,
	)
	.await;
}

async fn set_running_child_status(job: &Job, status: ExitStatus) {
	job.run_async({
		move |context| {
			let output_lock = if let CommandState::IsRunning { child, .. } = context.current {
				Some(child.output.clone())
			} else {
				None
			};

			Box::new(async move {
				if let Some(output_lock) = output_lock {
					*output_lock.lock().await = Some(Output {
						status,
						stdout: Vec::new(),
						stderr: Vec::new(),
					});
				}
			})
		}
	})
	.await;
}

macro_rules! expect_state {
	($current:literal, $job:expr, $expected:pat, $reason:literal) => {
		let state = Arc::new(Mutex::new(None));
		refresh_state(&$job, &state, $current).await;
		{
			let state = state.lock().unwrap();
			let reason = $reason;
			let reason = if reason.is_empty() {
				String::new()
			} else {
				format!(" ({reason})")
			};
			assert!(
				matches!(*state, Some($expected)),
				"expected Some({}), got {state:?}{reason}",
				stringify!($expected),
			);
		}
	};

	($job:expr, $expected:pat, $reason:literal) => {
		expect_state!(true, $job, $expected, $reason)
	};

	($job:expr, $expected:pat) => {
		expect_state!(true, $job, $expected, "")
	};

	(previous: $job:expr, $expected:pat, $reason:literal) => {
		expect_state!(false, $job, $expected, $reason)
	};

	(previous: $job:expr, $expected:pat) => {
		expect_state!(false, $job, $expected, "")
	};
}

async fn get_child(job: &Job) -> TestChild {
	let state = Arc::new(Mutex::new(None));
	refresh_state(job, &state, true).await;
	let state = state.lock().unwrap();
	let state = state.as_ref().expect("no state");
	match state {
		CommandState::IsRunning { ref child, .. } => child.clone(),
		_ => panic!("get_child: expected IsRunning, got {state:?}"),
	}
}

#[tokio::test]
async fn start() {
	let mut joinset = JoinSet::new();
	let job = start_job(&mut joinset, working_command());

	expect_state!(job, CommandState::ToRun);

	job.start().await;

	expect_state!(job, CommandState::IsRunning { .. });

	joinset.abort_all();
}

#[cfg(unix)]
#[tokio::test]
async fn signal_unix() {
	let mut joinset = JoinSet::new();
	let job = start_job(&mut joinset, working_command());

	expect_state!(job, CommandState::ToRun);

	job.start();
	job.signal(watchexec_signals::Signal::User1).await;

	let calls = get_child(&job).await.calls;
	assert!(calls
		.iter()
		.any(|(_, call)| matches!(call, TestChildCall::Signal(command_group::Signal::SIGUSR1))));

	joinset.abort_all();
}

#[tokio::test]
async fn stop() {
	let mut joinset = JoinSet::new();
	let job = start_job(&mut joinset, working_command());

	expect_state!(job, CommandState::ToRun);

	job.start().await;

	expect_state!(job, CommandState::IsRunning { .. });

	set_running_child_status(&job, ProcessEnd::Success.into_exitstatus()).await;

	job.stop().await;

	expect_state!(
		job,
		CommandState::Finished {
			status: ProcessEnd::Success,
			..
		}
	);

	joinset.abort_all();
}

#[tokio::test]
async fn stop_when_running() {
	let mut joinset = JoinSet::new();
	let job = start_job(&mut joinset, working_command());

	expect_state!(job, CommandState::ToRun);

	job.stop().await;

	expect_state!(job, CommandState::ToRun);

	job.start().await;

	expect_state!(job, CommandState::IsRunning { .. });

	joinset.abort_all();
}

#[tokio::test]
async fn stop_fail() {
	let mut joinset = JoinSet::new();
	let job = start_job(&mut joinset, working_command());

	expect_state!(job, CommandState::ToRun);

	job.start().await;

	expect_state!(job, CommandState::IsRunning { .. });

	set_running_child_status(
		&job,
		ProcessEnd::ExitError(NonZeroI64::new(1).unwrap()).into_exitstatus(),
	)
	.await;

	job.stop().await;

	expect_state!(
		job,
		CommandState::Finished {
			status: ProcessEnd::ExitError(_),
			..
		}
	);

	joinset.abort_all();
}

#[tokio::test]
async fn graceful_stop() {
	let mut joinset = JoinSet::new();
	let job = start_job(&mut joinset, working_command());

	expect_state!(job, CommandState::ToRun);

	job.start().await;

	expect_state!(job, CommandState::IsRunning { .. });

	set_running_child_status(&job, ProcessEnd::Success.into_exitstatus()).await;

	let stop = job.stop_with_signal(
		watchexec_signals::Signal::User1,
		std::time::Duration::from_millis(1),
	);

	expect_state!(
		job,
		CommandState::IsRunning { .. },
		"after USR1 but before delayed stop"
	);

	let calls = get_child(&job).await.calls;
	assert!(calls
		.iter()
		.any(|(_, call)| matches!(call, TestChildCall::Signal(command_group::Signal::SIGUSR1))));
	assert!(!calls
		.iter()
		.any(|(_, call)| matches!(call, TestChildCall::Wait)));

	stop.await;

	expect_state!(job, CommandState::Finished { .. });

	joinset.abort_all();
}

#[tokio::test]
async fn restart() {
	let mut joinset = JoinSet::new();
	let job = start_job(&mut joinset, working_command());

	expect_state!(job, CommandState::ToRun);

	job.start().await;

	expect_state!(job, CommandState::IsRunning { .. });

	set_running_child_status(
		&job,
		ProcessEnd::ExitError(NonZeroI64::new(1).unwrap()).into_exitstatus(),
	)
	.await;

	job.restart().await;

	expect_state!(job, CommandState::IsRunning { .. });

	set_running_child_status(&job, ProcessEnd::Success.into_exitstatus()).await;

	job.stop().await;

	expect_state!(
		previous: job,
		CommandState::Finished {
			status: ProcessEnd::ExitError(_),
			..
		}
	);

	expect_state!(
		job,
		CommandState::Finished {
			status: ProcessEnd::Success,
			..
		}
	);

	joinset.abort_all();
}

#[tokio::test]
async fn graceful_restart() {
	let mut joinset = JoinSet::new();
	let job = start_job(&mut joinset, working_command());

	expect_state!(job, CommandState::ToRun);

	job.start().await;

	expect_state!(job, CommandState::IsRunning { .. });

	set_running_child_status(
		&job,
		ProcessEnd::ExitError(NonZeroI64::new(1).unwrap()).into_exitstatus(),
	)
	.await;

	let restart = job.restart_with_signal(
		watchexec_signals::Signal::User1,
		std::time::Duration::from_millis(1),
	);

	expect_state!(
		job,
		CommandState::IsRunning { .. },
		"after USR1 but before delayed restart"
	);

	let calls = get_child(&job).await.calls;
	assert!(calls
		.iter()
		.any(|(_, call)| matches!(call, TestChildCall::Signal(command_group::Signal::SIGUSR1))));
	assert!(!calls
		.iter()
		.any(|(_, call)| matches!(call, TestChildCall::Wait)));

	restart.await;

	set_running_child_status(&job, ProcessEnd::Success.into_exitstatus()).await;

	job.stop().await;

	expect_state!(
		previous: job,
		CommandState::Finished {
			status: ProcessEnd::ExitError(_),
			..
		}
	);

	expect_state!(
		job,
		CommandState::Finished {
			status: ProcessEnd::Success,
			..
		}
	);

	joinset.abort_all();
}
