use watchexec_supervisor::command::{Command, Program, Shell};

#[tokio::test]
#[cfg(unix)]
async fn unix_shell_none() -> Result<(), std::io::Error> {
	assert!(Box::into_pin(
		Command {
			program: Program::Exec {
				prog: "echo".into(),
				args: vec!["hi".into()],
			},
			options: Default::default()
		}
		.to_spawnable()
		.spawn()?
		.wait()
	)
	.await?
	.success());
	Ok(())
}

#[tokio::test]
#[cfg(unix)]
async fn unix_shell_sh() -> Result<(), std::io::Error> {
	assert!(Box::into_pin(
		Command {
			program: Program::Shell {
				shell: Shell::new("sh"),
				command: "echo hi".into(),
				args: Vec::new(),
			},
			options: Default::default()
		}
		.to_spawnable()
		.spawn()?
		.wait()
	)
	.await?
	.success());
	Ok(())
}

#[tokio::test]
#[cfg(unix)]
async fn unix_shell_alternate() -> Result<(), std::io::Error> {
	assert!(Box::into_pin(
		Command {
			program: Program::Shell {
				shell: Shell::new("bash"),
				command: "echo".into(),
				args: vec!["--".into(), "hi".into()],
			},
			options: Default::default()
		}
		.to_spawnable()
		.spawn()?
		.wait()
	)
	.await?
	.success());
	Ok(())
}

#[tokio::test]
#[cfg(unix)]
async fn unix_shell_alternate_shopts() -> Result<(), std::io::Error> {
	assert!(Box::into_pin(
		Command {
			program: Program::Shell {
				shell: Shell {
					options: vec!["-o".into(), "errexit".into()],
					..Shell::new("bash")
				},
				command: "echo hi".into(),
				args: Vec::new(),
			},
			options: Default::default()
		}
		.to_spawnable()
		.spawn()?
		.wait()
	)
	.await?
	.success());
	Ok(())
}

#[tokio::test]
#[cfg(windows)]
async fn windows_shell_none() -> Result<(), std::io::Error> {
	assert!(Box::into_pin(
		Command {
			program: Program::Exec {
				prog: "echo".into(),
				args: vec!["hi".into()],
			},
			options: Default::default()
		}
		.to_spawnable()
		.spawn()?
		.wait()
	)
	.await?
	.success());
	Ok(())
}

#[tokio::test]
#[cfg(windows)]
async fn windows_shell_cmd() -> Result<(), std::io::Error> {
	assert!(Box::into_pin(
		Command {
			program: Program::Shell {
				shell: Shell::cmd(),
				args: Vec::new(),
				command: r#""echo" hi"#.into()
			},
			options: Default::default()
		}
		.to_spawnable()
		.spawn()?
		.wait()
	)
	.await?
	.success());
	Ok(())
}

#[tokio::test]
#[cfg(windows)]
async fn windows_shell_powershell() -> Result<(), std::io::Error> {
	assert!(Box::into_pin(
		Command {
			program: Program::Shell {
				shell: Shell::new("pwsh.exe"),
				args: Vec::new(),
				command: "echo hi".into()
			},
			options: Default::default()
		}
		.to_spawnable()
		.spawn()?
		.wait()
	)
	.await?
	.success());
	Ok(())
}
