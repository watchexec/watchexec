use std::path::PathBuf;
use std::{fs, sync::OnceLock};

use miette::{Context, IntoDiagnostic, Result};
use rand::Rng;

static PLACEHOLDER_DATA: OnceLock<String> = OnceLock::new();
fn get_placeholder_data() -> &'static str {
	PLACEHOLDER_DATA.get_or_init(|| "PLACEHOLDER\n".repeat(500))
}

/// The amount of nesting that will be used for generated files
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GeneratedFileNesting {
	/// Only one level of files
	Flat,
	/// Random, up to a certiain maximum
	RandomToMax(usize),
}

/// Configuration for creating testing subfolders
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TestSubfolderConfiguration {
	/// The amount of nesting that will be used when folders are generated
	pub(crate) nesting: GeneratedFileNesting,

	/// Number of files the folder should contain
	pub(crate) file_count: usize,

	/// Subfolder name
	pub(crate) name: String,
}

/// Options for generating test files
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct GenerateTestFilesArgs {
	/// The path where the files should be generated
	/// if None, the current working directory will be used.
	pub(crate) path: Option<PathBuf>,

	/// Configurations for subfolders to generate
	pub(crate) subfolder_configs: Vec<TestSubfolderConfiguration>,
}

/// Generate test files
///
/// This returns the same number of paths that were requested via subfolder_configs.
pub(crate) fn generate_test_files(args: GenerateTestFilesArgs) -> Result<Vec<PathBuf>> {
	// Use or create a temporary directory for the test files
	let tmpdir = if let Some(p) = args.path {
		p
	} else {
		tempfile::tempdir()
			.into_diagnostic()
			.wrap_err("failed to build tempdir")?
			.into_path()
	};
	let mut paths = vec![tmpdir.clone()];

	// Generate subfolders matching each config
	for subfolder_config in args.subfolder_configs.iter() {
		// Create the subfolder path
		let subfolder_path = tmpdir.join(&subfolder_config.name);
		fs::create_dir(&subfolder_path)
			.into_diagnostic()
			.wrap_err(format!(
				"failed to create path for dir [{}]",
				subfolder_path.display()
			))?;
		paths.push(subfolder_path.clone());

		// Fill the subfolder with files
		match subfolder_config.nesting {
			GeneratedFileNesting::Flat => {
				for idx in 0..subfolder_config.file_count {
					// Write stub file contents
					fs::write(
						subfolder_path.join(format!("stub-file-{idx}")),
						get_placeholder_data(),
					)
					.into_diagnostic()
					.wrap_err(format!(
						"failed to write temporary file in subfolder {} @ idx {idx}",
						subfolder_path.display()
					))?;
				}
			}
			GeneratedFileNesting::RandomToMax(max_depth) => {
				let mut generator = rand::thread_rng();
				for idx in 0..subfolder_config.file_count {
					// Build a randomized path up to max depth
					let mut generated_path = subfolder_path.clone();
					let depth = generator.gen_range(0..max_depth);
					for _ in 0..depth {
						generated_path.push("stub-dir");
					}
					// Create the path
					fs::create_dir_all(&generated_path)
						.into_diagnostic()
						.wrap_err(format!(
							"failed to create randomly generated path [{}]",
							generated_path.display()
						))?;

					// Write stub file contents @ the new randomized path
					fs::write(
						generated_path.join(format!("stub-file-{idx}")),
						get_placeholder_data(),
					)
					.into_diagnostic()
					.wrap_err(format!(
						"failed to write temporary file in subfolder {} @ idx {idx}",
						subfolder_path.display()
					))?;
				}
			}
		}
	}

	Ok(paths)
}
