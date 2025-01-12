use std::{
	future::Future,
	io::Result,
	path::Path,
	process::{ExitStatus, Output},
	sync::Arc,
	time::{Duration, Instant},
};

use process_wrap::tokio::TokioChildWrapper;
use tokio::{sync::Mutex, time::sleep};
use watchexec_events::ProcessEnd;

use crate::command::{Command, Program};

/// Mock implementation of [`TokioChildWrapper`](process_wrap::tokio::TokioChildWrapper).
#[derive(Debug, Clone)]
pub struct TestChild {
	#[allow(dead_code)]
	pub grouped: bool,
	pub command: Arc<Command>,
	pub calls: Arc<boxcar::Vec<TestChildCall>>,
	pub output: Arc<Mutex<Option<Output>>>,
	pub spawned: Instant,
}

impl TestChild {
	pub fn new(command: Arc<Command>) -> std::io::Result<Self> {
		if let Program::Exec { prog, .. } = &command.program {
			if prog == Path::new("/does/not/exist") {
				return Err(std::io::Error::new(
					std::io::ErrorKind::NotFound,
					"file not found",
				));
			}
		}

		Ok(Self {
			grouped: command.options.grouped || command.options.session,
			command,
			calls: Arc::new(boxcar::Vec::new()),
			output: Arc::new(Mutex::new(None)),
			spawned: Instant::now(),
		})
	}
}

#[derive(Debug)]
pub enum TestChildCall {
	Id,
	Kill,
	StartKill,
	TryWait,
	Wait,
	#[cfg(unix)]
	Signal(i32),
}

impl TokioChildWrapper for TestChild {
	fn inner(&self) -> &tokio::process::Child {
		unimplemented!("mock")
	}

	fn inner_mut(&mut self) -> &mut tokio::process::Child {
		unimplemented!("mock")
	}

	fn into_inner(self: Box<Self>) -> tokio::process::Child {
		unimplemented!("mock")
	}

	fn stdin(&mut self) -> &mut Option<tokio::process::ChildStdin> {
		unimplemented!("mock")
	}

	fn stdout(&mut self) -> &mut Option<tokio::process::ChildStdout> {
		unimplemented!("mock")
	}

	fn stderr(&mut self) -> &mut Option<tokio::process::ChildStderr> {
		unimplemented!("mock")
	}

	fn try_clone(&self) -> Option<Box<dyn TokioChildWrapper>> {
		Some(Box::new(self.clone()))
	}

	fn id(&self) -> Option<u32> {
		self.calls.push(TestChildCall::Id);
		None
	}

	fn kill(&mut self) -> Box<dyn Future<Output = Result<()>> + Send + '_> {
		self.calls.push(TestChildCall::Kill);
		Box::new(async { Ok(()) })
	}

	fn start_kill(&mut self) -> Result<()> {
		self.calls.push(TestChildCall::StartKill);
		Ok(())
	}

	fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
		self.calls.push(TestChildCall::TryWait);

		if let Program::Exec { prog, args } = &self.command.program {
			if prog == Path::new("sleep") {
				if let Some(time) = args
					.first()
					.and_then(|arg| arg.parse().ok())
					.map(Duration::from_millis)
				{
					if self.spawned.elapsed() < time {
						return Ok(None);
					}
				}
			}
		}

		Ok(self
			.output
			.try_lock()
			.ok()
			.and_then(|o| o.as_ref().map(|o| o.status)))
	}

	fn wait(&mut self) -> Box<dyn Future<Output = Result<ExitStatus>> + Send + '_> {
		self.calls.push(TestChildCall::Wait);
		Box::new(async {
			if let Program::Exec { prog, args } = &self.command.program {
				if prog == Path::new("sleep") {
					if let Some(time) = args
						.first()
						.and_then(|arg| arg.parse().ok())
						.map(Duration::from_millis)
					{
						if self.spawned.elapsed() < time {
							sleep(time - self.spawned.elapsed()).await;
							if let Ok(guard) = self.output.try_lock() {
								if let Some(output) = guard.as_ref() {
									return Ok(output.status);
								}
							}

							return Ok(ProcessEnd::Success.into_exitstatus());
						}
					}
				}
			}

			loop {
				eprintln!("[{:?}] child: output lock", Instant::now());
				let output = self.output.lock().await;
				if let Some(output) = output.as_ref() {
					return Ok(output.status);
				}
				eprintln!("[{:?}] child: output unlock", Instant::now());

				sleep(Duration::from_secs(1)).await;
			}
		})
	}

	fn wait_with_output(self: Box<TestChild>) -> Box<dyn Future<Output = Result<Output>> + Send> {
		Box::new(async move {
			loop {
				let mut output = self.output.lock().await;
				if let Some(output) = output.take() {
					return Ok(output);
				}

				sleep(Duration::from_secs(1)).await;
			}
		})
	}

	#[cfg(unix)]
	fn signal(&self, sig: i32) -> Result<()> {
		self.calls.push(TestChildCall::Signal(sig));
		Ok(())
	}
}
