use std::fmt;

use tokio::process::Command as TokioCommand;
use tracing::trace;

use super::{Command, Program};

impl Program {
	/// Obtain a [`tokio::process::Command`].
	pub fn to_spawnable(&self) -> TokioCommand {
		trace!(program=?self, "constructing command");

		let mut cmd = match self {
			Self::Exec { prog, args, .. } => {
				let mut c = TokioCommand::new(prog);
				c.args(args);
				c
			}

			Self::Shell {
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
		{
			// Resets the sigmask of the process before we spawn it.
			//
			// Required from Rust 1.66:
			// https://github.com/rust-lang/rust/pull/101077
			//
			// Done before the spawn hook so it can be used to set a different mask if desired.
			use nix::sys::signal::{sigprocmask, SigSet, SigmaskHow, Signal};
			unsafe {
				cmd.pre_exec(|| {
					let mut oldset = SigSet::empty();
					let mut newset = SigSet::all();
					newset.remove(Signal::SIGHUP); // leave SIGHUP alone so nohup works
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

impl Command {
	/// Obtain a [`tokio::process::Command`].
	pub fn to_spawnable(&self) -> TokioCommand {
		self.program.to_spawnable()
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
