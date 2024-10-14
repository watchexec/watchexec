use std::{
	future::Future,
	io::Result,
	path::Path,
	process::{ExitStatus, Output},
	sync::Arc,
	time::{Duration, Instant},
};

use tokio::{sync::Mutex, time::sleep};
use watchexec_events::ProcessEnd;

use crate::command::{Command, Program};

/// Mock version of [`TokioChildWrapper`](process_wrap::tokio::TokioChildWrapper).
#[derive(Debug, Clone)]
pub struct TestChild {
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

// Exact same signatures as ErasedChild
impl TestChild {
	pub fn id(&mut self) -> Option<u32> {
		self.calls.push(TestChildCall::Id);
		None
	}

	pub fn kill(&mut self) -> Box<dyn Future<Output = Result<()>> + Send + '_> {
		self.calls.push(TestChildCall::Kill);
		Box::new(async { Ok(()) })
	}

	pub fn start_kill(&mut self) -> Result<()> {
		self.calls.push(TestChildCall::StartKill);
		Ok(())
	}

	pub fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
		self.calls.push(TestChildCall::TryWait);

		if let Program::Exec { prog, args } = &self.command.program {
			if prog == Path::new("sleep") {
				if let Some(time) = args
					.get(0)
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

	pub fn wait(&mut self) -> Box<dyn Future<Output = Result<ExitStatus>> + Send + '_> {
		self.calls.push(TestChildCall::Wait);
		Box::new(async {
			if let Program::Exec { prog, args } = &self.command.program {
				if prog == Path::new("sleep") {
					if let Some(time) = args
						.get(0)
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

	pub fn wait_with_output(self) -> Box<dyn Future<Output = Result<Output>> + Send> {
		Box::new(async move {
			loop {
				let mut output = self.output.lock().await;
				if let Some(output) = output.take() {
					return Ok(output);
				} else {
					sleep(Duration::from_secs(1)).await;
				}
			}
		})
	}

	#[cfg(unix)]
	pub fn signal(&self, sig: i32) -> Result<()> {
		self.calls.push(TestChildCall::Signal(sig));
		Ok(())
	}
}
