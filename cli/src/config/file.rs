use kdl::{parse_document, KdlNode, KdlValue};
use miette::{IntoDiagnostic, Report, Result};
use watchexec::command::Shell;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Config {
	pub commands: Vec<Command>,
}

impl Config {
	pub fn parse(input: &str) -> Result<Config> {
		let kdl = parse_document(input).into_diagnostic()?;
		let mut config = Config::default();

		for root in kdl {
			match root.name.as_str() {
				"command" => config.commands.push(Command::parse(root)?),
				otherwise => todo!("Root: {:?}", otherwise),
			}
		}

		Ok(config)
	}
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Command {
	pub name: String,
	pub run: Option<Run>,
}

impl Command {
	fn parse(node: KdlNode) -> Result<Self> {
		let name = node
			.values
			.first()
			.ok_or_else(|| Report::msg("Command has no name"))
			.and_then(|name| match name {
				KdlValue::String(s) => Ok(s.to_owned()),
				otherwise => Err(Report::msg("Command name is not a string")
					.wrap_err(format!("{:?}", otherwise))),
			})?;

		let mut runs = node
			.children
			.iter()
			.filter(|node| node.name == "run")
			.map(Run::parse)
			.collect::<Result<Vec<_>>>()?;

		if runs.len() > 1 {
			return Err(Report::msg("Command has multiple runs"));
		}

		Ok(Command {
			name,
			run: runs.pop(),
		})
	}
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Run {
	pub shell: Shell,
	pub args: Vec<String>,
}

impl Run {
	fn parse(node: &KdlNode) -> Result<Self> {
		let args = node
			.values
			.iter()
			.enumerate()
			.map(|(n, v)| match v {
				KdlValue::String(s) => Ok(s.to_owned()),
				otherwise => Err(Report::msg(format!("Run argument {n} is not a string"))
					.wrap_err(format!("{otherwise:?}"))),
			})
			.collect::<Result<Vec<_>>>()
			.and_then(|run| {
				if run.is_empty() {
					Err(Report::msg("Run has no arguments"))
				} else {
					Ok(run)
				}
			})?;

		let shell = node
			.properties
			.get("shell")
			.map(|shell| match shell {
				KdlValue::String(s) => Ok(s.to_owned()),
				otherwise => {
					Err(Report::msg("Run shell is not a string").wrap_err(format!("{otherwise:?}")))
				}
			})
			.transpose()?
			.map(|shell| match shell.as_str() {
				"powershell" | "pwsh" => Shell::Powershell,
				"none" => Shell::None,
				#[cfg(windows)]
				"cmd" => Shell::Cmd,
				unix => Shell::Unix(unix.to_owned()),
			})
			.unwrap_or_default();

		if args.len() > 1 && shell != Shell::None {
			Err(Report::msg("Run has more than one argument and a shell"))
		} else {
			Ok(Run { shell, args })
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn empty_command() {
		let config = Config::parse(r#"command "empty""#).unwrap();
		assert_eq!(config.commands.len(), 1);
		assert_eq!(config.commands[0].name, "empty");
	}

	#[test]
	fn empty_command_with_braces() {
		let config = Config::parse(r#"command "empty" {}"#).unwrap();
		assert_eq!(config.commands.len(), 1);
		assert_eq!(config.commands[0].name, "empty");
	}

	#[test]
	fn command_with_run_one() {
		let config = Config::parse(
			r#"command "running" {
			run "echo hello-world"
		}"#,
		)
		.unwrap();
		assert_eq!(config.commands.len(), 1);
		assert_eq!(config.commands[0].name, "running");
		assert_eq!(
			config.commands[0].run,
			Some(Run {
				args: vec!["echo hello-world".to_owned()],
				shell: Shell::default()
			})
		);
	}

	#[test]
	fn command_with_run_two() {
		let config = Config::parse(
			r#"command "running" {
			run "echo" "hello-world"
		}"#,
		)
		.unwrap();
		assert_eq!(config.commands.len(), 1);
		assert_eq!(config.commands[0].name, "running");
		assert_eq!(
			config.commands[0].run,
			Some(Run {
				args: vec!["echo".to_owned(), "hello-world".to_owned()],
				shell: Shell::default()
			})
		);
	}

	#[test]
	fn command_with_no_run() {
		let config = Config::parse(r#"command "running" {}"#).unwrap();
		assert_eq!(config.commands.len(), 1);
		assert_eq!(config.commands[0].name, "running");
		assert_eq!(config.commands[0].run, None);
	}

	#[test]
	#[should_panic]
	fn command_with_empty_run() {
		Config::parse(
			r#"command "running" {
			run
		}"#,
		)
		.unwrap();
	}

	#[test]
	fn run_with_default_shell() {
		let config = Config::parse(
			r#"command "running" {
			run "echo hello-world"
		}"#,
		)
		.unwrap();
		assert_eq!(
			config.commands[0].run.as_ref().unwrap().shell,
			Shell::default()
		);
		assert_eq!(
			config.commands[0].run.as_ref().unwrap().shell,
			Shell::None
		);
	}

	#[test]
	fn run_with_explicit_shell() {
		let config = Config::parse(
			r#"command "running" {
			run shell="bash" "echo hello-world"
		}"#,
		)
		.unwrap();
		assert_eq!(
			config.commands[0].run.as_ref().unwrap().shell,
			Shell::Unix("bash".to_owned())
		);
	}

	#[test]
	fn run_with_powershell() {
		let config = Config::parse(
			r#"command "running" {
			run shell="powershell" "echo hello-world"
		}"#,
		)
		.unwrap();
		assert_eq!(
			config.commands[0].run.as_ref().unwrap().shell,
			Shell::Powershell
		);

		let config = Config::parse(
			r#"command "running" {
			run shell="pwsh" "echo hello-world"
		}"#,
		)
		.unwrap();
		assert_eq!(
			config.commands[0].run.as_ref().unwrap().shell,
			Shell::Powershell
		);
	}

	#[cfg(unix)]
	#[test]
	fn run_with_cmd_unix() {
		let config = Config::parse(
			r#"command "running" {
			run shell="cmd" "echo hello-world"
		}"#,
		)
		.unwrap();
		assert_eq!(
			config.commands[0].run.as_ref().unwrap().shell,
			Shell::Unix("cmd".to_owned())
		);
	}

	#[cfg(windows)]
	#[test]
	fn run_with_cmd_windows() {
		let config = Config::parse(
			r#"command "running" {
			run shell="cmd" "echo hello-world"
		}"#,
		)
		.unwrap();
		assert_eq!(config.commands[0].run.as_ref().unwrap().shell, Shell::Cmd);
	}

	#[test]
	#[should_panic]
	fn multi_arg_run_with_shell() {
		Config::parse(
			r#"command "running" {
			run shell="bash" "echo" "hello-world"
		}"#,
		)
		.unwrap();
	}
}
