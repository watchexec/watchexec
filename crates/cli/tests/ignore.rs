use std::{
	path::{Path, PathBuf}, process::Stdio, time::Duration
};

use miette::{IntoDiagnostic, Result, WrapErr};
use tokio::{
	process::Command,
	time::Instant,
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
	let small_elapsed = run_watchexec_cmd(&wexec_bin, root_dir_path, args.clone()).await?;

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
	let large_elapsed = run_watchexec_cmd(&wexec_bin, root_dir_path, args.clone()).await?;

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