use std::fmt;

use tokio::process::Command as TokioCommand;
use tracing::trace;

use super::{Command, Program};

impl Command {
	/// Obtain a [`tokio::process::Command`].
	pub fn to_spawnable(&self) -> TokioCommand {
		trace!(program=?self.program, "constructing command");

		let mut cmd = match &self.program {
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

		#[cfg(unix)]
		if self.options.reset_sigmask {
			use nix::sys::signal::{sigprocmask, SigSet, SigmaskHow};
			unsafe {
				cmd.pre_exec(|| {
					let mut oldset = SigSet::empty();
					let newset = SigSet::all();
					trace!(unblocking=?newset, "resetting process sigmask");
					sigprocmask(SigmaskHow::SIG_UNBLOCK, Some(&newset), Some(&mut oldset))?;
					trace!(?oldset, "sigmask reset");
					Ok(())
				});
			}
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
