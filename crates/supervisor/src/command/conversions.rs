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

	// On Unix, have the shell replace itself with the command via `exec` instead of forking
	// and waiting on it, when it's safe to do so (see `shell_command_is_execable`).
	//
	// Without this, the shell (e.g. `sh -c "the-command"`) is the direct child process that
	// watchexec spawns and waits on, and the actual command is the shell's own child. A shell
	// invoked this way has no signal handlers of its own, so when watchexec sends a graceful
	// stop signal to the process group (e.g. for `--restart`), the shell is killed immediately
	// while the real command - which may have its own signal handler to shut down gracefully -
	// keeps running as an orphan in the same group. watchexec only waits on the shell, so it
	// concludes the command has exited and starts a new one right away, even though the old
	// command is still alive. See https://github.com/watchexec/watchexec/issues/960.
	if cfg!(unix) && shell_command_is_execable(command_str) {
		command.arg(format!("exec {command_str}"));
	} else {
		command.arg(command_str);
	}

	for arg in args {
		command.arg(arg);
	}
}

/// Whether it's safe to prefix `command` with `exec ` so the wrapping shell replaces itself
/// with it, rather than running it as a child.
///
/// This is unsafe for commands that rely on the shell surviving past the first program it
/// runs: sequencing (`;`, newlines), conditionals (`&&`, `||`), pipes (`|`), and backgrounding
/// (`&`). `exec`ing into the first program of such a command would silently skip the rest of
/// it, since control never returns to the shell.
fn shell_command_is_execable(command: &str) -> bool {
	!command.contains([';', '&', '|', '\n'])
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn simple_commands_are_execable() {
		assert!(shell_command_is_execable("bun run src/test.ts"));
		assert!(shell_command_is_execable("make"));
		assert!(shell_command_is_execable("echo 'hello world'"));
		assert!(shell_command_is_execable("cmd --flag=a > out.log"));
		assert!(shell_command_is_execable("cmd $(echo sub)"));
	}

	#[test]
	fn compound_commands_are_not_execable() {
		assert!(!shell_command_is_execable("make build && make test"));
		assert!(!shell_command_is_execable("make build; make test"));
		assert!(!shell_command_is_execable("make build || make test"));
		assert!(!shell_command_is_execable("cat file | grep foo"));
		assert!(!shell_command_is_execable("server &"));
		assert!(!shell_command_is_execable("line one\nline two"));
	}

	fn shell() -> Shell {
		Shell::new("sh")
	}

	#[test]
	fn quoted_args_exec_prefix_for_simple_command() {
		let mut cmd = TokioCommand::new("sh");
		pass_program_args_quoted(
			&mut cmd,
			&shell(),
			&Vec::new(),
			&"bun run src/test.ts".to_string(),
		);

		let args: Vec<_> = cmd
			.as_std()
			.get_args()
			.map(|a| a.to_string_lossy().into_owned())
			.collect();

		if cfg!(unix) {
			assert_eq!(args, vec!["-c", "exec bun run src/test.ts"]);
		} else {
			assert_eq!(args, vec!["-c", "bun run src/test.ts"]);
		}
	}

	#[test]
	fn quoted_args_no_exec_prefix_for_compound_command() {
		let mut cmd = TokioCommand::new("sh");
		pass_program_args_quoted(
			&mut cmd,
			&shell(),
			&Vec::new(),
			&"make build && make test".to_string(),
		);

		let args: Vec<_> = cmd
			.as_std()
			.get_args()
			.map(|a| a.to_string_lossy().into_owned())
			.collect();

		assert_eq!(args, vec!["-c", "make build && make test"]);
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
