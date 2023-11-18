use std::{
	io::Result,
	process::{ExitStatus, Output}, sync::Arc,
};

use tokio::{process::Command as TokioCommand, sync::Mutex, task::yield_now};

use crate::command::Command;

/// Mock version of [`ErasedChild`](command_group::ErasedChild).
#[derive(Debug)]
pub struct TestChild {
	pub grouped: bool,
	pub spawnable: TokioCommand,
	pub command: Command,
	pub calls: Arc<boxcar::Vec<TestChildCall>>,
	pub output: Mutex<Option<Output>>,
}

impl TestChild {
	pub fn new(command: Command, spawnable: TokioCommand) -> Self {
		Self {
			grouped: command.grouped,
			spawnable,
			command,
			calls: Arc::new(boxcar::Vec::new()),
			output: Mutex::new(None),
		}
	}
}

#[derive(Debug)]
pub enum TestChildCall {
	Id,
	Kill,
	StartKill,
	TryWait,
	Wait,
	Signal(command_group::Signal),
}

// Exact same signatures as ErasedChild
impl TestChild {
	pub fn id(&mut self) -> Option<u32> {
		self.calls.push(TestChildCall::Id);
		None
	}

	pub async fn kill(&mut self) -> Result<()> {
		self.calls.push(TestChildCall::Kill);
		Ok(())
	}

	pub fn start_kill(&mut self) -> Result<()> {
		self.calls.push(TestChildCall::StartKill);
		Ok(())
	}

	pub fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
		self.calls.push(TestChildCall::TryWait);
		Ok(self
			.output
			.try_lock()
			.ok()
			.and_then(|o| o.as_ref().map(|o| o.status)))
	}

	pub async fn wait(&mut self) -> Result<ExitStatus> {
		self.calls.push(TestChildCall::Wait);
		loop {
			let output = self.output.lock().await;
			if let Some(output) = output.as_ref() {
				return Ok(output.status);
			} else {
				yield_now().await;
			}
		}
	}

	pub async fn wait_with_output(self) -> Result<Output> {
		loop {
			let mut output = self.output.lock().await;
			if let Some(output) = output.take() {
				return Ok(output);
			} else {
				yield_now().await;
			}
		}
	}

	pub fn signal(&self, sig: command_group::Signal) -> Result<()> {
		self.calls.push(TestChildCall::Signal(sig));
		Ok(())
	}
}
