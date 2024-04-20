use std::fmt;

use process_wrap::tokio::{TokioCommandWrap, KillOnDrop};
use tokio::process::Command as TokioCommand;
use tracing::trace;

use super::{Command, Program, SpawnOptions};

impl Command {
	/// Obtain a [`process_wrap::tokio::TokioCommandWrap`].
	pub fn to_spawnable(&self) -> TokioCommandWrap {
		trace!(program=?self.program, "constructing command");

		let cmd = match &self.program {
			Program::Exec { prog, args, .. } => {
				let mut c = TokioCommand::new(prog);
				c.args(args);
				c
			}

			Program::Shell {
				shell,
				args,
				command,
			} => {
				let mut c = TokioCommand::new(shell.prog.clone());

				// Avoid quoting issues on Windows by using raw_arg everywhere
				#[cfg(windows)]
				{
					for opt in &shell.options {
						c.raw_arg(opt);
					}
					if let Some(progopt) = &shell.program_option {
						c.raw_arg(progopt);
					}
					c.raw_arg(command);
					for arg in args {
						c.raw_arg(arg);
					}
				}

				#[cfg(not(windows))]
				{
					c.args(shell.options.clone());
					if let Some(progopt) = &shell.program_option {
						c.arg(progopt);
					}
					c.arg(command);
					for arg in args {
						c.arg(arg);
					}
				}

				c
			}
		};

		let mut cmd = TokioCommandWrap::from(cmd);
		cmd.wrap(KillOnDrop);

		match self.options {
			#[cfg(unix)]
			SpawnOptions { session: true, .. } => {
				cmd.wrap(process_wrap::tokio::ProcessSession);
			}
			#[cfg(unix)]
			SpawnOptions { grouped: true, .. } => {
				cmd.wrap(process_wrap::tokio::ProcessGroup::leader());
			}
			#[cfg(windows)]
			SpawnOptions { grouped: true, .. } | SpawnOptions { session: true, .. } => {
				cmd.wrap(process_wrap::tokio::JobObject);
			}
			_ => {}
		}

		#[cfg(unix)]
		if self.options.reset_sigmask {
			cmd.wrap(process_wrap::tokio::ResetSigmask);
		}

		cmd
	}
}

impl fmt::Display for Program {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Exec { prog, args, .. } => {
				write!(f, "{}", prog.display())?;
				for arg in args {
					write!(f, " {arg}")?;
				}

				Ok(())
			}
			Self::Shell { command, .. } => {
				write!(f, "{command}")
			}
		}
	}
}

impl fmt::Display for Command {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.program)
	}
}
