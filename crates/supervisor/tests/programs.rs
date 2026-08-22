use std::{sync::Arc, time::Duration};

use watchexec_supervisor::{
	command::{Command, Program, Shell, SpawnOptions},
	job::start_job,
	Signal,
};

#[tokio::test]
#[cfg(unix)]
async fn unix_shell_none() -> Result<(), std::io::Error> {
	assert!(Command {
		program: Program::Exec {
			prog: "echo".into(),
			args: vec!["hi".into()],
		},
		options: Default::default()
	}
	.to_spawnable()
	.spawn()?
	.wait()
	.await?
	.success());
	Ok(())
}

#[tokio::test]
#[cfg(unix)]
async fn unix_shell_sh() -> Result<(), std::io::Error> {
	assert!(Command {
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
	.await?
	.success());
	Ok(())
}

#[tokio::test]
#[cfg(unix)]
async fn unix_shell_alternate() -> Result<(), std::io::Error> {
	assert!(Command {
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
	.await?
	.success());
	Ok(())
}

#[tokio::test]
#[cfg(unix)]
async fn unix_shell_alternate_shopts() -> Result<(), std::io::Error> {
	assert!(Command {
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
	.await?
	.success());
	Ok(())
}

/// Regression test for https://github.com/watchexec/watchexec/issues/960
///
/// The command here runs a script through `sh`, wrapped by the `sh -c` created for
/// [`Program::Shell`]. Before the fix, that outer `sh` forks the script interpreter as its own
/// child instead of exec-ing into it; the outer `sh` has no signal handler of its own, so sending
/// the graceful-stop signal to the process group kills it immediately. Since that outer `sh` is
/// the direct child the job waits on, the job would consider the command finished and start a new
/// one right away - even though the actual script (a trapped, still-running child of that wrapper)
/// hadn't exited yet. With the fix, the outer shell `exec`s into the script interpreter instead of
/// forking it, so there's no separate un-trapped wrapper to kill early, and the job correctly waits
/// for the script's own trap-driven shutdown.
#[tokio::test]
#[cfg(unix)]
async fn restart_with_signal_waits_for_the_actual_command_to_exit() {
	let unique = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.expect("system time")
		.as_nanos();
	let script = std::env::temp_dir().join(format!(
		"watchexec-test-trap-{}-{unique}.sh",
		std::process::id()
	));
	let ready = script.with_extension("ready");
	std::fs::write(
		&script,
		"#!/bin/sh\ntrap 'sleep 1; exit 0' TERM\n: > \"$1\"\nsleep 30\n",
	)
	.expect("write test script");

	let (job, task) = start_job(Arc::new(Command {
		program: Program::Shell {
			shell: Shell::new("sh"),
			command: r#"sh "$0" "$1""#.into(),
			args: vec![
				script.to_string_lossy().into_owned(),
				ready.to_string_lossy().into_owned(),
			],
		},
		options: SpawnOptions {
			grouped: true,
			..Default::default()
		},
	}));

	job.start().await;

	// Wait until the script has installed its trap before signalling it.
	tokio::time::timeout(Duration::from_secs(5), async {
		while !ready.exists() {
			tokio::time::sleep(Duration::from_millis(10)).await;
		}
	})
	.await
	.expect("script did not become ready");

	let began = std::time::Instant::now();
	job.restart_with_signal(Signal::Terminate, Duration::from_secs(5))
		.await;
	let elapsed = began.elapsed();

	job.stop().await;
	task.abort();
	let _ = std::fs::remove_file(&script);
	let _ = std::fs::remove_file(&ready);

	// the trap sleeps for 1s before exiting; if the restart didn't actually wait for it,
	// this would resolve almost instantly instead
	assert!(
		elapsed >= Duration::from_millis(800),
		"restart resolved after {elapsed:?}, expected it to wait for the trapped shutdown (~1s)"
	);
}

#[tokio::test]
#[cfg(windows)]
async fn windows_shell_none() -> Result<(), std::io::Error> {
	assert!(Command {
		program: Program::Exec {
			prog: "echo".into(),
			args: vec!["hi".into()],
		},
		options: Default::default()
	}
	.to_spawnable()
	.spawn()?
	.wait()
	.await?
	.success());
	Ok(())
}

#[tokio::test]
#[cfg(windows)]
async fn windows_shell_cmd() -> Result<(), std::io::Error> {
	assert!(Command {
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
	.await?
	.success());
	Ok(())
}

#[tokio::test]
#[cfg(windows)]
async fn windows_shell_powershell() -> Result<(), std::io::Error> {
	assert!(Command {
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
	.await?
	.success());
	Ok(())
}
