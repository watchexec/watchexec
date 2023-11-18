use std::sync::{
	atomic::{AtomicBool, Ordering},
	Arc,
};

use tokio::task::JoinSet;

use crate::{
	command::{Command, Program},
	job::start_job,
};

fn command() -> Command {
	Command {
		program: Program::Exec {
			prog: "/does/not/exist".into(),
			args: Vec::new(),
		},
		grouped: true,
	}
}

#[tokio::test]
async fn sync_error_handler() {
	let mut joinset = JoinSet::new();
	let job = start_job(&mut joinset, command());
	let error_handler_called = Arc::new(AtomicBool::new(false));

	job.set_error_handler({
		let error_handler_called = error_handler_called.clone();
		move |_| {
			error_handler_called.store(true, Ordering::Relaxed);
		}
	})
	.unwrap()
	.await;

	job.start().unwrap().await;

	assert!(error_handler_called.load(Ordering::Relaxed));

	joinset.abort_all();
}

#[tokio::test]
async fn async_error_handler() {
	let mut joinset = JoinSet::new();
	let job = start_job(&mut joinset, command());
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
	.unwrap()
	.await;

	job.start().unwrap().await;

	assert!(error_handler_called.load(Ordering::Relaxed));

	joinset.abort_all();
}

#[tokio::test]
async fn unset_error_handler() {
	let mut joinset = JoinSet::new();
	let job = start_job(&mut joinset, command());
	let error_handler_called = Arc::new(AtomicBool::new(false));

	job.set_error_handler({
		let error_handler_called = error_handler_called.clone();
		move |_| {
			error_handler_called.store(true, Ordering::Relaxed);
		}
	})
	.unwrap()
	.await;

	job.unset_error_handler().unwrap().await;

	job.start().unwrap().await;

	assert!(!error_handler_called.load(Ordering::Relaxed));

	joinset.abort_all();
}

#[tokio::test]
async fn queue_ordering() {
	let mut joinset = JoinSet::new();
	let job = start_job(&mut joinset, command());
	let error_handler_called = Arc::new(AtomicBool::new(false));

	job.set_error_handler({
		let error_handler_called = error_handler_called.clone();
		move |_| {
			error_handler_called.store(true, Ordering::Relaxed);
		}
	})
	.unwrap();

	job.unset_error_handler().unwrap();

	// We're not awaiting until this one, but because the queue is processed in
	// order, it's effectively the same as waiting them all.
	job.start().unwrap().await;

	assert!(!error_handler_called.load(Ordering::Relaxed));

	joinset.abort_all();
}
