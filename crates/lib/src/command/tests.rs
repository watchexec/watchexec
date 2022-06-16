
use super::{Command, Shell};
use command_group::AsyncCommandGroup;

#[tokio::test]
#[cfg(unix)]
async fn unix_shell_none() -> Result<(), std::io::Error> {
	assert!(Command::Exec {
		prog: "echo".into(),
		args: vec!["hi".into()]
	}
	.to_spawnable()
	.unwrap()
	.group_status()
	.await?
	.success());
	Ok(())
}

#[tokio::test]
#[cfg(unix)]
async fn unix_shell_sh() -> Result<(), std::io::Error> {
	assert!(Command::Shell {
		shell: Shell::Unix("sh".into()),
		args: Vec::new(),
		command: "echo hi".into()
	}
	.to_spawnable()
	.unwrap()
	.group_status()
	.await?
	.success());
	Ok(())
}

#[tokio::test]
#[cfg(unix)]
async fn unix_shell_alternate() -> Result<(), std::io::Error> {
	assert!(Command::Shell {
		shell: Shell::Unix("bash".into()),
		args: Vec::new(),
		command: "echo hi".into()
	}
	.to_spawnable()
	.unwrap()
	.group_status()
	.await?
	.success());
	Ok(())
}

#[tokio::test]
#[cfg(unix)]
async fn unix_shell_alternate_shopts() -> Result<(), std::io::Error> {
	assert!(Command::Shell {
		shell: Shell::Unix("bash".into()),
		args: vec!["-o".into(), "errexit".into()],
		command: "echo hi".into()
	}
	.to_spawnable()
	.unwrap()
	.group_status()
	.await?
	.success());
	Ok(())
}

#[tokio::test]
#[cfg(windows)]
async fn windows_shell_none() -> Result<(), std::io::Error> {
	assert!(Command::Exec {
		prog: "echo".into(),
		args: vec!["hi".into()]
	}
	.to_spawnable()
	.unwrap()
	.group_status()
	.await?
	.success());
	Ok(())
}

#[tokio::test]
#[cfg(windows)]
async fn windows_shell_cmd() -> Result<(), std::io::Error> {
	assert!(Command::Shell {
		shell: Shell::Cmd,
		args: Vec::new(),
		command: "echo hi".into()
	}
	.to_spawnable()
	.unwrap()
	.group_status()
	.await?
	.success());
	Ok(())
}

#[tokio::test]
#[cfg(windows)]
async fn windows_shell_powershell() -> Result<(), std::io::Error> {
	assert!(Command::Shell {
		shell: Shell::Powershell,
		args: Vec::new(),
		command: "echo hi".into()
	}
	.to_spawnable()
	.unwrap()
	.group_status()
	.await?
	.success());
	Ok(())
}

#[tokio::test]
#[cfg(windows)]
async fn windows_shell_unix_style_powershell() -> Result<(), std::io::Error> {
	assert!(Command::Shell {
		shell: Shell::Unix("powershell.exe".into()),
		args: Vec::new(),
		command: "echo hi".into()
	}
	.to_spawnable()
	.unwrap()
	.group_status()
	.await?
	.success());
	Ok(())
}
