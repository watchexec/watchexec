use std::{ffi::OsStr, path::PathBuf, process::Stdio, time::Duration};

use dunce::canonicalize;
use miette::{IntoDiagnostic, Result, WrapErr};
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
	let small_elapsed = run_watchexec_cmd(&wexec_bin, root_dir_path, args.clone(), None).await?;

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
	let large_elapsed = run_watchexec_cmd(&wexec_bin, root_dir_path, args.clone(), None).await?;

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
async fn run_watchexec_cmd(
	wexec_bin: impl AsRef<str>,
	dir: impl AsRef<Path>,
	args: impl Into<Vec<String>>,
	max_timeout: Option<Duration>,
) -> Result<Duration> {
	// Build the subprocess command
	let mut cmd = Command::new(wexec_bin.as_ref());
	cmd.args(args.into());
	cmd.current_dir(dir);
	cmd.stdout(Stdio::piped());
	cmd.stderr(Stdio::piped());

	let start = Instant::now();
	let child_future = cmd.kill_on_drop(true).output();
	let res = if let Some(time) = max_timeout {
		timeout(time, child_future).await
	} else {
		Ok(child_future.await)
	};

	res.into_diagnostic()
		.wrap_err("timeout err")?
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
	// Determine the watchexec bin to use & build arguments
	let wexec_bin = std::env::var("TEST_WATCHEXEC_BIN").unwrap_or(
		option_env!("CARGO_BIN_EXE_watchexec")
			.map(std::string::ToString::to_string)
			.unwrap_or("watchexec".into()),
	);
	println!("wexec binary: {wexec_bin}");

	let mut cmd = Command::new(wexec_bin);
	cmd.args(args.into());
	cmd.current_dir(dir);
	cmd.stdout(Stdio::piped());
	cmd.stderr(Stdio::piped());
	cmd.spawn()
		.into_diagnostic()
		.wrap_err("Failed to spawn watchexec")
}

async fn is_output_empty(
	tmp: &mut Vec<u8>,
	timeout_duration: Duration,
	stdout: &mut (impl AsyncReadExt + std::marker::Unpin),
) -> Option<String> {
	use std::io::Write;
	// assert!(timeout(timeout_duration, stdout.read_u8()).await.is_ok());
	let mut some_text = false;
	while let Ok(Ok(n)) = timeout(timeout_duration, stdout.read_buf(tmp)).await {
		if n == 0 {
			break;
		}
		some_text = true;
		if let Ok(str) = String::from_utf8(tmp.clone()) {
			println!("{str}");
			std::io::stdout().lock().flush();
		}
		tmp.clear();
	}
	if !some_text {
		return Some("No text output from the process".into());
	}

	if timeout(timeout_duration, stdout.read_u8()).await.is_ok() {
		return Some("There is still something left".into());
	}

	None
}

async fn assert_no_reaction(
	tmp: &mut Vec<u8>,
	timeout_duration: Duration,
	stdout: &mut (impl AsyncReadExt + std::marker::Unpin),
) {
	let res = is_output_empty(tmp, timeout_duration, stdout).await;
	assert!(res.is_some(), "Should be no output");
}

async fn assert_reaction(
	tmp: &mut Vec<u8>,
	timeout_duration: Duration,
	stdout: &mut (impl AsyncReadExt + std::marker::Unpin),
) {
	let res = is_output_empty(tmp, timeout_duration, stdout).await;
	assert!(res.is_none(), "{}", res.unwrap_or(String::new()));
}

#[tokio::test]
async fn watch_single_file_test() -> Result<()> {
	std::env::set_var("RUST_BACKTRACE", "1");
	let test_dir = tempfile::tempdir()
		.into_diagnostic()
		.wrap_err("failed to create tempdir for test use")?;
	let dir_path = canonicalize(test_dir.path()).expect("Failed to canonicalize tmp dir path");
	let file_path = dir_path.join("file");
	std::fs::File::create(file_path.clone()).into_diagnostic()?;

	let mut child = start_watchexec_cmd(
		dir_path.clone(),
		vec!["-w", file_path.to_str().unwrap(), "echo", "change"],
	)?;

	let timeout_duration = Duration::from_millis(400);
	// Start timeout is longer bc on windows a process starts slow
	let start_timeout_duration = Duration::from_millis(800);
	let mut tmp = vec![];
	let mut stdout = child
		.stdout
		.take()
		.ok_or(miette::Error::msg("Failed to take child stdout"))?;

	assert_reaction(&mut tmp, start_timeout_duration, &mut stdout).await;

	// Positive cases
	std::fs::remove_file(file_path.clone()).into_diagnostic()?;
	assert_reaction(&mut tmp, timeout_duration, &mut stdout).await;

	std::fs::File::create(file_path.clone()).into_diagnostic()?;
	assert_reaction(&mut tmp, timeout_duration, &mut stdout).await;

	std::fs::remove_file(file_path.clone()).into_diagnostic()?;
	assert_reaction(&mut tmp, timeout_duration, &mut stdout).await;

	std::fs::File::create(file_path.clone()).into_diagnostic()?;
	assert_reaction(&mut tmp, timeout_duration, &mut stdout).await;

	// Negative cases
	let file_path2 = dir_path.join("file2");
	std::fs::File::create(file_path2.clone()).into_diagnostic()?;
	assert_no_reaction(&mut tmp, timeout_duration, &mut stdout).await;

	std::fs::remove_file(file_path2.clone()).into_diagnostic()?;
	assert_no_reaction(&mut tmp, timeout_duration, &mut stdout).await;

	// Remove original file before nested tests
	std::fs::remove_file(file_path.clone()).into_diagnostic()?;
	assert_reaction(&mut tmp, timeout_duration, &mut stdout).await;

	// Nested matches
	// std::fs::create_dir(dir_path.join("file"))
	// 	.into_diagnostic()
	// 	.wrap_err("Creating Directory")?;
	// let file_nested = dir_path.join("file").join("file2");
	// std::fs::File::create(file_nested.clone()).into_diagnostic()?;
	// assert!(
	// 	timeout(timeout_duration, stdout.read_u8()).await.is_err(),
	// 	"Should be no output when creating a directory"
	// );

	// std::fs::remove_file(file_nested.clone()).into_diagnostic()?;
	// assert!(
	// 	timeout(timeout_duration, stdout.read_u8()).await.is_err(),
	// 	"Should be no output when removing a directory"
	// );

	child.kill().await.expect("Child is not dead :(");

	Ok(())
}
