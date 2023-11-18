use std::{
	ffi::OsStr,
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc,
	},
};

use tokio::task::JoinSet;

use crate::{
	command::{Command, Program, Shell},
	job::start_job,
};

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
