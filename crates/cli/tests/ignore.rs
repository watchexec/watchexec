use std::{
	ffi::OsStr,
	path::{Path, PathBuf},
	process::Stdio,
	time::Duration,
};

use assert_cmd::prelude::CommandCargoExt;
use miette::{Error, IntoDiagnostic, Result, WrapErr};
use tokio::{
	io::AsyncReadExt,
	process::{Child, Command},
	time::{timeout, Instant},
};
use tracing_test::traced_test;
use uuid::Uuid;

mod common;
use common::{generate_test_files, GenerateTestFilesArgs};

use crate::common::{GeneratedFileNesting, TestSubfolderConfiguration};

/// Directory name that will be sued for the dir that *should* be watched
const WATCH_DIR_NAME: &str = "watch";

/// The token that watch will echo every time a match is found
const WATCH_TOKEN: &str = "updated";

/// Ensure that watchexec runtime does not increase with the
/// number of *ignored* files in a given folder
///
/// This test creates two separate folders, one small and the other large
///
/// Each folder has two subfolders:
///   - a shallow one to be watched, with a few files of single depth (20 files)
///   - a deep one to be ignored, with many files at varying depths (small case 200 files, large case 200,000 files)
///
/// watchexec, when executed on *either* folder should *not* experience a more
/// than 10x degradation in performance, because the vast majority of the files
/// are supposed to be ignored to begin with.
///
/// When running the CLI on the root folders, it should *not* take a long time to start de
#[tokio::test]
#[traced_test]
async fn e2e_ignore_many_files_200_000() -> Result<()> {
	// Create a tempfile so that drop will clean it up
	let small_test_dir = tempfile::tempdir()
		.into_diagnostic()
		.wrap_err("failed to create tempdir for test use")?;

	// Determine the watchexec bin to use & build arguments
	let wexec_bin = std::env::var("TEST_WATCHEXEC_BIN").unwrap_or(
		option_env!("CARGO_BIN_EXE_watchexec")
			.map(std::string::ToString::to_string)
			.unwrap_or("watchexec".into()),
	);
	let token = format!("{WATCH_TOKEN}-{}", Uuid::new_v4());
	let args: Vec<String> = vec![
		"-1".into(), // exit as soon as watch completes
		"--watch".into(),
		WATCH_DIR_NAME.into(),
		"echo".into(),
		token.clone(),
	];

	// Generate a small directory of files containing dirs that *will* and will *not* be watched
	let [ref root_dir_path, _, _] = generate_test_files(GenerateTestFilesArgs {
		path: Some(PathBuf::from(small_test_dir.path())),
		subfolder_configs: vec![
			// Shallow folder will have a small number of files and won't be watched
			TestSubfolderConfiguration {
				name: "watch".into(),
				nesting: GeneratedFileNesting::Flat,
				file_count: 5,
			},
			// Deep folder will have *many* amll files and will be watched
			TestSubfolderConfiguration {
				name: "unrelated".into(),
				nesting: GeneratedFileNesting::RandomToMax(42),
				file_count: 200,
			},
		],
	})?[..] else {
		panic!("unexpected number of paths returned from generate_test_files");
	};

	// Get the number of elapsed
	let small_elapsed = run_watchexec_cmd_once(&wexec_bin, root_dir_path, args.clone()).await?;

	// Create a tempfile so that drop will clean it up
	let large_test_dir = tempfile::tempdir()
		.into_diagnostic()
		.wrap_err("failed to create tempdir for test use")?;

	// Generate a *large* directory of files
	let [ref root_dir_path, _, _] = generate_test_files(GenerateTestFilesArgs {
		path: Some(PathBuf::from(large_test_dir.path())),
		subfolder_configs: vec![
			// Shallow folder will have a small number of files and won't be watched
			TestSubfolderConfiguration {
				name: "watch".into(),
				nesting: GeneratedFileNesting::Flat,
				file_count: 5,
			},
			// Deep folder will have *many* amll files and will be watched
			TestSubfolderConfiguration {
				name: "unrelated".into(),
				nesting: GeneratedFileNesting::RandomToMax(42),
				file_count: 200_000,
			},
		],
	})?[..] else {
		panic!("unexpected number of paths returned from generate_test_files");
	};

	// Get the number of elapsed
	let large_elapsed = run_watchexec_cmd_once(&wexec_bin, root_dir_path, args.clone()).await?;

	// We expect the ignores to not impact watchexec startup time at all
	// whether there are 200 files in there or 200k
	assert!(
		large_elapsed < small_elapsed * 10,
		"200k ignore folder ({:?}) took more than 10x more time ({:?}) than 200 ignore folder ({:?})",
		large_elapsed,
		small_elapsed * 10,
		small_elapsed,
	);
	Ok(())
}

/// Run a watchexec command once
async fn run_watchexec_cmd_once(
	wexec_bin: impl AsRef<str>,
	dir: impl AsRef<Path>,
	args: impl Into<Vec<String>>,
) -> Result<Duration> {
	// Build the subprocess command
	let mut cmd = Command::new(wexec_bin.as_ref());
	cmd.args(args.into());
	cmd.current_dir(dir);
	cmd.stdout(Stdio::piped());
	cmd.stderr(Stdio::piped());

	let start = Instant::now();
	cmd.kill_on_drop(true)
		.output()
		.await
		.into_diagnostic()
		.wrap_err("fixed")?;

	Ok(start.elapsed())
}

fn start_watchexec_cmd<Element>(
	dir: impl AsRef<Path>,
	args: impl Into<Vec<Element>>,
) -> Result<Child>
where
	Element: AsRef<OsStr>,
{
	let mut cmd: Command = std::process::Command::cargo_bin("watchexec")
		.into_diagnostic()
		.wrap_err("Failed to create watchexec command")?
		.into();
	cmd.args(args.into());
	cmd.current_dir(dir);
	cmd.stdout(Stdio::piped());
	cmd.stderr(Stdio::piped());
	cmd.spawn()
		.into_diagnostic()
		.wrap_err("Failed to spawn watchexec")
}

async fn assert_stdout_and_clear(
	tmp: &mut Vec<u8>,
	timeout_duration: Duration,
	stdout: &mut (impl AsyncReadExt + std::marker::Unpin),
) {
	assert!(timeout(timeout_duration, stdout.read_u8()).await.is_ok());
	while let Ok(Ok(n)) = timeout(timeout_duration, stdout.read_buf(tmp)).await {
		if n == 0 {
			break;
		}

		tmp.clear();
	}
	assert!(timeout(timeout_duration, stdout.read_u8()).await.is_err());
}

#[tokio::test]
async fn watch_single_file_test() -> Result<()> {
	let test_dir = tempfile::tempdir()
		.into_diagnostic()
		.wrap_err("failed to create tempdir for test use")?;
	let dir_path = test_dir.path().to_path_buf();
	let file_path = dir_path.join("file");
	std::fs::File::create(file_path.clone()).into_diagnostic()?;
	let mut child = start_watchexec_cmd(
		dir_path,
		vec!["-w", file_path.to_str().unwrap(), "echo", "change"],
	)?;

	let timeout_duration = Duration::from_millis(50);
	let mut tmp = vec![];
	let mut stdout = child
		.stdout
		.take()
		.ok_or(Error::msg("Failed to take child stdout"))?;
	stdout.read_u8().await.into_diagnostic()?;
	while timeout(timeout_duration, stdout.read_to_end(&mut tmp))
		.await
		.is_ok()
	{
		tmp.clear();
	}

	let timeout_duration = Duration::from_millis(250);

	std::fs::remove_file(file_path.clone()).into_diagnostic()?;
	assert_stdout_and_clear(&mut tmp, timeout_duration, &mut stdout).await;

	std::fs::File::create(file_path.clone()).into_diagnostic()?;
	assert_stdout_and_clear(&mut tmp, timeout_duration, &mut stdout).await;

	std::fs::remove_file(file_path.clone()).into_diagnostic()?;
	assert_stdout_and_clear(&mut tmp, timeout_duration, &mut stdout).await;

	std::fs::File::create(file_path.clone()).into_diagnostic()?;
	assert_stdout_and_clear(&mut tmp, timeout_duration, &mut stdout).await;

	std::fs::remove_file(file_path.clone()).into_diagnostic()?;
	assert_stdout_and_clear(&mut tmp, timeout_duration, &mut stdout).await;

	child.kill().await.expect("Child is not dead :(");

	Ok(())
}
