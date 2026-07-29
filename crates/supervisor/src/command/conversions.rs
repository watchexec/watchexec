use std::fmt;

use process_wrap::tokio::{CommandWrap, KillOnDrop};
use tokio::process::Command as TokioCommand;
use tracing::trace;

use super::{Command, Program, Shell, SpawnOptions};

impl Command {
	/// Obtain a [`process_wrap::tokio::CommandWrap`].
	pub fn to_spawnable(&self) -> CommandWrap {
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

				// Previously on Windows when you use git-bash and you're attempting to perform a multi-word command such as "npm run build-dev"
				// only "npm" would be passed, and the remainder would be thrown away if we treat the args the way we do for normal Windows shells
				// such as CMD or PowerShell.
				// To get around this, we added the ability to opt in to quoting, while still passing raw values by default on Windows so
				// cmd and PowerShell still work correctly without anyone having to change their workflows.
				#[cfg(windows)]
				{
					if shell.quote {
						pass_program_args_quoted(&mut c, shell, args, command);
					} else {
						pass_program_args_raw(&mut c, shell, args, command);
					}
				}

				#[cfg(not(windows))]
				{
					pass_program_args_quoted(&mut c, shell, args, command);
				}

				c
			}
		};

		let mut cmd = CommandWrap::from(cmd);
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

#[cfg(windows)]
fn pass_program_args_raw(
	command: &mut TokioCommand,
	shell: &Shell,
	args: &Vec<String>,
	command_str: &String,
) {
	// cmd and PowerShell don't work correctly if the args are quoted, so when not opting into quoting, we pass them as raw values.

	for opt in &shell.options {
		command.raw_arg(opt);
	}

	if let Some(progopt) = &shell.program_option {
		command.raw_arg(progopt);
	}

	command.raw_arg(command_str);

	for arg in args {
		command.raw_arg(arg);
	}
}

fn pass_program_args_quoted(
	command: &mut TokioCommand,
	shell: &Shell,
	args: &Vec<String>,
	command_str: &String,
) {
	command.args(shell.options.clone());

	if let Some(progopt) = &shell.program_option {
		command.arg(progopt);
	}

	command.arg(command_str);

	for arg in args {
		command.arg(arg);
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
