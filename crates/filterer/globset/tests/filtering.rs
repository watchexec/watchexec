mod helpers;
use helpers::globset::*;
use ignore_files::IgnoreFile;
use std::{
	ffi::OsString,
	io::Write,
	path::{Path, PathBuf},
};
use watchexec::filter::Filterer;
use watchexec_filterer_globset::GlobsetFilterer;

fn assert_source_dir(filterer: &impl Filterer, path: &str, pass: bool) {
	let origin = std::fs::canonicalize(".").unwrap();
	assert_eq!(
		filterer.check_dir(&origin.join(path)).unwrap(),
		pass,
		"source directory {path:?} (expected {})",
		if pass { "pass" } else { "fail" }
	);
}

async fn direct_filt(
	origin: &Path,
	ignores: &[&str],
	whitelist: impl IntoIterator<Item = PathBuf>,
) -> GlobsetFilterer {
	direct_full_filt(origin, &[], ignores, whitelist).await
}

async fn direct_full_filt(
	origin: &Path,
	filters: &[&str],
	ignores: &[&str],
	whitelist: impl IntoIterator<Item = PathBuf>,
) -> GlobsetFilterer {
	GlobsetFilterer::new(
		origin,
		filters.iter().map(|filter| ((*filter).to_owned(), None)),
		ignores.iter().map(|ignore| ((*ignore).to_owned(), None)),
		whitelist,
		std::iter::empty::<IgnoreFile>(),
		std::iter::empty::<OsString>(),
	)
	.await
	.unwrap()
}

#[tokio::test]
async fn empty_filter_passes_everything() {
	let filterer = filt(&[], &[], &[], &[], &[]).await;

	filterer.file_does_pass("Cargo.toml");
	filterer.file_does_pass("Cargo.json");
	filterer.file_does_pass("Gemfile.toml");
	filterer.file_does_pass("FINAL-FINAL.docx");
	filterer.dir_does_pass("/test/Cargo.toml");
	filterer.dir_does_pass("/a/folder");
	filterer.file_does_pass("apples/carrots/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("apples/oranges/bananas");
	filterer.dir_does_pass("apples/carrots/oranges");
	filterer.dir_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.dir_does_pass("apples/oranges/bananas");
}

#[tokio::test]
async fn source_checks_manual_ignore_boundary_parent_and_negation() {
	let filterer = filt(&[], &["**/prunes"], &[], &[], &[]).await;
	assert_source_dir(&filterer, "apples", true);
	assert_source_dir(&filterer, "prunes", false);
	assert_source_dir(&filterer, "prunes/nested", false);

	let filterer = filt(&[], &["**/keep", "!**/keep"], &[], &[], &[]).await;
	assert_source_dir(&filterer, "keep", true);
}

#[tokio::test]
async fn source_checks_ignore_files() {
	let mut ignore_file = tempfile::NamedTempFile::new().unwrap();
	ignore_file.write_all(b"ignored\n").unwrap();
	let filterer = filt(&[], &[], &[], &[], &[ignore_file.path().to_path_buf()]).await;

	assert_source_dir(&filterer, "allowed", true);
	assert_source_dir(&filterer, "ignored", false);
	assert_source_dir(&filterer, "ignored/nested", false);
}

#[tokio::test]
async fn source_checks_git_boundary_and_descendants() {
	let filterer = filt(&[], &["**/.git", "**/.git/**"], &[], &[], &[]).await;
	assert_source_dir(&filterer, ".github", true);
	assert_source_dir(&filterer, ".git", false);
	assert_source_dir(&filterer, ".git/objects", false);
	assert_source_dir(&filterer, "project/.git", false);
	assert_source_dir(&filterer, "project/.git/objects", false);
}

#[tokio::test]
async fn source_checks_do_not_use_positive_filters_or_extensions() {
	let filterer = filt(&["**/*.rs"], &[], &[], &["rs"], &[]).await;

	filterer.dir_doesnt_pass("build");
	assert_source_dir(&filterer, "build", true);
	assert_source_dir(&filterer, "src", true);
}

#[tokio::test]
async fn exact_whitelist_is_event_only_for_source_checks() {
	let origin = std::fs::canonicalize(".").unwrap();
	let whitelist = origin.join(".git").display().to_string();
	let filterer = filt(&[], &["**/.git", "**/.git/**"], &[&whitelist], &[], &[]).await;

	filterer.dir_does_pass(".git");
	assert_source_dir(&filterer, ".git", false);
}

#[tokio::test]
async fn relative_origin_stops_ancestor_matching_at_project_boundary() {
	let sandbox = tempfile::tempdir_in(".").unwrap();
	let project = sandbox.path().join("rust").join("project");
	std::fs::create_dir_all(&project).unwrap();
	let origin = std::fs::canonicalize(&project).unwrap();
	let cwd = std::fs::canonicalize(".").unwrap();
	let relative_origin = origin.strip_prefix(cwd).unwrap().to_owned();
	let filterer = direct_filt(&relative_origin, &["rust"], Vec::new()).await;

	assert!(filterer.check_dir(&origin.join("src")).unwrap());
	assert!(!filterer.check_dir(&origin.join("rust")).unwrap());
}

#[tokio::test]
async fn out_of_origin_paths_do_not_match_external_ancestors() {
	let sandbox = tempfile::tempdir().unwrap();
	let origin = sandbox.path().join("project");
	let external = sandbox.path().join("rust").join("watched");
	std::fs::create_dir_all(&origin).unwrap();
	std::fs::create_dir_all(&external).unwrap();
	let filterer = direct_filt(&origin, &["rust"], Vec::new()).await;

	assert!(filterer.check_dir(&external.join("child")).unwrap());
	assert!(!filterer
		.check_dir(&origin.join("rust").join("child"))
		.unwrap());
}

#[tokio::test]
async fn direct_whitelist_normalises_constructor_and_event_aliases() {
	use watchexec_events::{Event, FileType, Tag};

	let origin = std::fs::canonicalize(".").unwrap();
	let whitelisted = origin.join("first").join("..").join("watched");
	let emitted = origin.join("second").join("..").join("watched");
	let filterer = direct_filt(&origin, &["watched"], vec![whitelisted]).await;
	let event = Event {
		tags: vec![Tag::Path {
			path: emitted.clone(),
			file_type: Some(FileType::Dir),
		}],
		metadata: Default::default(),
	};

	assert!(filterer.check_event(&event, Priority::Normal).unwrap());
	assert!(!filterer.check_dir(&emitted).unwrap());
}

#[tokio::test]
async fn direct_positive_filter_normalises_lexical_event_alias() {
	use watchexec_events::{Event, FileType, Tag};

	let origin = dunce::canonicalize(".").unwrap();
	let emitted = origin
		.join("alias")
		.join("..")
		.join("watched")
		.join("main.rs");
	let filterer =
		direct_full_filt(&origin, &["watched/main.rs"], &[], Vec::<PathBuf>::new()).await;
	let event = Event {
		tags: vec![Tag::Path {
			path: emitted,
			file_type: Some(FileType::File),
		}],
		metadata: Default::default(),
	};

	assert!(filterer.check_event(&event, Priority::Normal).unwrap());
}

#[cfg(windows)]
#[tokio::test]
async fn direct_whitelist_simplifies_verbatim_windows_path() {
	use watchexec_events::{Event, FileType, Tag};

	let origin = dunce::canonicalize(".").unwrap();
	let emitted = origin.join("watched");
	let verbatim = PathBuf::from(format!(r"\\?\{}", emitted.display()));
	let filterer = direct_filt(&origin, &["watched"], vec![verbatim]).await;
	let event = Event {
		tags: vec![Tag::Path {
			path: emitted,
			file_type: Some(FileType::Dir),
		}],
		metadata: Default::default(),
	};

	assert!(filterer.check_event(&event, Priority::Normal).unwrap());
}

#[cfg(windows)]
#[tokio::test]
async fn direct_positive_filter_simplifies_verbatim_windows_path() {
	use watchexec_events::{Event, FileType, Tag};

	let origin = dunce::canonicalize(".").unwrap();
	let emitted = origin.join("watched").join("main.rs");
	let verbatim = PathBuf::from(format!(r"\\?\{}", emitted.display()));
	let filterer =
		direct_full_filt(&origin, &["watched/main.rs"], &[], Vec::<PathBuf>::new()).await;
	let event = Event {
		tags: vec![Tag::Path {
			path: verbatim,
			file_type: Some(FileType::File),
		}],
		metadata: Default::default(),
	};

	assert!(filterer.check_event(&event, Priority::Normal).unwrap());
}

#[tokio::test]
async fn exact_filename() {
	let filterer = filt(&["Cargo.toml"], &[], &[], &[], &[]).await;

	filterer.file_does_pass("Cargo.toml");
	filterer.file_does_pass("/test/foo/bar/Cargo.toml");
	filterer.file_doesnt_pass("Cargo.json");
	filterer.file_doesnt_pass("Gemfile.toml");
	filterer.file_doesnt_pass("FINAL-FINAL.docx");
	filterer.dir_doesnt_pass("/a/folder");
	filterer.dir_does_pass("/test/Cargo.toml");
}

#[tokio::test]
async fn exact_filename_in_folder() {
	let filterer = filt(&["sub/Cargo.toml"], &[], &[], &[], &[]).await;

	filterer.file_doesnt_pass("Cargo.toml");
	filterer.file_does_pass("sub/Cargo.toml");
	filterer.file_doesnt_pass("/test/foo/bar/Cargo.toml");
	filterer.file_doesnt_pass("Cargo.json");
	filterer.file_doesnt_pass("Gemfile.toml");
	filterer.file_doesnt_pass("FINAL-FINAL.docx");
	filterer.dir_doesnt_pass("/a/folder");
	filterer.dir_does_pass("/test/sub/Cargo.toml");
}

#[tokio::test]
async fn exact_filename_in_hidden_folder() {
	let filterer = filt(&[".sub/Cargo.toml"], &[], &[], &[], &[]).await;

	filterer.file_doesnt_pass("Cargo.toml");
	filterer.file_does_pass(".sub/Cargo.toml");
	filterer.file_doesnt_pass("/test/foo/bar/Cargo.toml");
	filterer.file_doesnt_pass("Cargo.json");
	filterer.file_doesnt_pass("Gemfile.toml");
	filterer.file_doesnt_pass("FINAL-FINAL.docx");
	filterer.dir_doesnt_pass("/a/folder");
	filterer.dir_does_pass("/test/.sub/Cargo.toml");
}

#[tokio::test]
async fn exact_filenames_multiple() {
	let filterer = filt(&["Cargo.toml", "package.json"], &[], &[], &[], &[]).await;

	filterer.file_does_pass("Cargo.toml");
	filterer.file_does_pass("/test/foo/bar/Cargo.toml");
	filterer.file_does_pass("package.json");
	filterer.file_does_pass("/test/foo/bar/package.json");
	filterer.file_doesnt_pass("Cargo.json");
	filterer.file_doesnt_pass("package.toml");
	filterer.file_doesnt_pass("Gemfile.toml");
	filterer.file_doesnt_pass("FINAL-FINAL.docx");
	filterer.dir_doesnt_pass("/a/folder");
	filterer.dir_does_pass("/test/Cargo.toml");
	filterer.dir_does_pass("/test/package.json");
}

#[tokio::test]
async fn glob_single_final_ext_star() {
	let filterer = filt(&["Cargo.*"], &[], &[], &[], &[]).await;

	filterer.file_does_pass("Cargo.toml");
	filterer.file_does_pass("Cargo.json");
	filterer.file_doesnt_pass("Gemfile.toml");
	filterer.file_doesnt_pass("FINAL-FINAL.docx");
	filterer.dir_doesnt_pass("/a/folder");
	filterer.dir_does_pass("Cargo.toml");
}

#[tokio::test]
async fn glob_star_trailing_slash() {
	let filterer = filt(&["Cargo.*/"], &[], &[], &[], &[]).await;

	filterer.file_doesnt_pass("Cargo.toml");
	filterer.file_doesnt_pass("Cargo.json");
	filterer.file_doesnt_pass("Gemfile.toml");
	filterer.file_doesnt_pass("FINAL-FINAL.docx");
	filterer.dir_doesnt_pass("/a/folder");
	filterer.dir_does_pass("Cargo.toml");
	filterer.unk_doesnt_pass("Cargo.toml");
}

#[tokio::test]
async fn glob_star_leading_slash() {
	let filterer = filt(&["/Cargo.*"], &[], &[], &[], &[]).await;

	filterer.file_does_pass("Cargo.toml");
	filterer.file_does_pass("Cargo.json");
	filterer.dir_does_pass("Cargo.toml");
	filterer.unk_does_pass("Cargo.toml");
	filterer.file_doesnt_pass("foo/Cargo.toml");
	filterer.dir_doesnt_pass("foo/Cargo.toml");
}

#[tokio::test]
async fn glob_leading_double_star() {
	let filterer = filt(&["**/possum"], &[], &[], &[], &[]).await;

	filterer.file_does_pass("possum");
	filterer.file_does_pass("foo/bar/possum");
	filterer.file_does_pass("/foo/bar/possum");
	filterer.dir_does_pass("possum");
	filterer.dir_does_pass("foo/bar/possum");
	filterer.dir_does_pass("/foo/bar/possum");
	filterer.file_doesnt_pass("rat");
	filterer.file_doesnt_pass("foo/bar/rat");
	filterer.file_doesnt_pass("/foo/bar/rat");
}

#[tokio::test]
async fn glob_trailing_double_star() {
	let filterer = filt(&["possum/**"], &[], &[], &[], &[]).await;

	// these do work by expectation and in v1
	filterer.file_does_pass("/test/possum/foo/bar");
	filterer.dir_doesnt_pass("possum");
	filterer.dir_doesnt_pass("foo/bar/possum");
	filterer.dir_does_pass("possum/foo/bar");
	filterer.file_doesnt_pass("rat");
	filterer.file_doesnt_pass("foo/bar/rat");
	filterer.file_doesnt_pass("/foo/bar/rat");
}

#[tokio::test]
async fn glob_middle_double_star() {
	let filterer = filt(&["apples/**/oranges"], &[], &[], &[], &[]).await;

	filterer.dir_doesnt_pass("/a/folder");
	filterer.file_does_pass("apples/carrots/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.file_doesnt_pass("apples/oranges/bananas");
	filterer.dir_does_pass("apples/carrots/oranges");
	filterer.dir_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.dir_doesnt_pass("apples/oranges/bananas");
}

#[tokio::test]
async fn glob_double_star_trailing_slash() {
	let filterer = filt(&["apples/**/oranges/"], &[], &[], &[], &[]).await;

	filterer.dir_doesnt_pass("/a/folder");
	filterer.file_doesnt_pass("apples/carrots/oranges");
	filterer.file_doesnt_pass("apples/carrots/cauliflowers/oranges");
	filterer.file_doesnt_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.file_doesnt_pass("apples/oranges/bananas");
	filterer.dir_does_pass("apples/carrots/oranges");
	filterer.dir_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.dir_doesnt_pass("apples/oranges/bananas");
	filterer.unk_doesnt_pass("apples/carrots/oranges");
	filterer.unk_doesnt_pass("apples/carrots/cauliflowers/oranges");
	filterer.unk_doesnt_pass("apples/carrots/cauliflowers/artichokes/oranges");
}

#[tokio::test]
async fn ignore_exact_filename() {
	let filterer = filt(&[], &["Cargo.toml"], &[], &[], &[]).await;

	filterer.file_doesnt_pass("Cargo.toml");
	filterer.file_doesnt_pass("/test/foo/bar/Cargo.toml");
	filterer.file_does_pass("Cargo.json");
	filterer.file_does_pass("Gemfile.toml");
	filterer.file_does_pass("FINAL-FINAL.docx");
	filterer.dir_does_pass("/a/folder");
	filterer.dir_doesnt_pass("/test/Cargo.toml");
}

#[tokio::test]
async fn ignore_exact_filename_in_folder() {
	let filterer = filt(&[], &["sub/Cargo.toml"], &[], &[], &[]).await;

	filterer.file_does_pass("Cargo.toml");
	filterer.file_doesnt_pass("sub/Cargo.toml");
	filterer.file_does_pass("/test/foo/bar/Cargo.toml");
	filterer.file_does_pass("Cargo.json");
	filterer.file_does_pass("Gemfile.toml");
	filterer.file_does_pass("FINAL-FINAL.docx");
	filterer.dir_does_pass("/a/folder");
	filterer.dir_doesnt_pass("/test/sub/Cargo.toml");
}

#[tokio::test]
async fn ignore_exact_filename_in_hidden_folder() {
	let filterer = filt(&[], &[".sub/Cargo.toml"], &[], &[], &[]).await;

	filterer.file_does_pass("Cargo.toml");
	filterer.file_doesnt_pass(".sub/Cargo.toml");
	filterer.file_does_pass("/test/foo/bar/Cargo.toml");
	filterer.file_does_pass("Cargo.json");
	filterer.file_does_pass("Gemfile.toml");
	filterer.file_does_pass("FINAL-FINAL.docx");
	filterer.dir_does_pass("/a/folder");
	filterer.dir_doesnt_pass("/test/.sub/Cargo.toml");
}

#[tokio::test]
async fn ignore_exact_filenames_multiple() {
	let filterer = filt(&[], &["Cargo.toml", "package.json"], &[], &[], &[]).await;

	filterer.file_doesnt_pass("Cargo.toml");
	filterer.file_doesnt_pass("/test/foo/bar/Cargo.toml");
	filterer.file_doesnt_pass("package.json");
	filterer.file_doesnt_pass("/test/foo/bar/package.json");
	filterer.file_does_pass("Cargo.json");
	filterer.file_does_pass("package.toml");
	filterer.file_does_pass("Gemfile.toml");
	filterer.file_does_pass("FINAL-FINAL.docx");
	filterer.dir_does_pass("/a/folder");
	filterer.dir_doesnt_pass("/test/Cargo.toml");
	filterer.dir_doesnt_pass("/test/package.json");
}

#[tokio::test]
async fn ignore_glob_single_final_ext_star() {
	let filterer = filt(&[], &["Cargo.*"], &[], &[], &[]).await;

	filterer.file_doesnt_pass("Cargo.toml");
	filterer.file_doesnt_pass("Cargo.json");
	filterer.file_does_pass("Gemfile.toml");
	filterer.file_does_pass("FINAL-FINAL.docx");
	filterer.dir_does_pass("/a/folder");
	filterer.dir_doesnt_pass("Cargo.toml");
}

#[tokio::test]
async fn ignore_glob_star_trailing_slash() {
	let filterer = filt(&[], &["Cargo.*/"], &[], &[], &[]).await;

	filterer.file_does_pass("Cargo.toml");
	filterer.file_does_pass("Cargo.json");
	filterer.file_does_pass("Gemfile.toml");
	filterer.file_does_pass("FINAL-FINAL.docx");
	filterer.dir_does_pass("/a/folder");
	filterer.dir_doesnt_pass("Cargo.toml");
	filterer.unk_does_pass("Cargo.toml");
}

#[tokio::test]
async fn ignore_glob_star_leading_slash() {
	let filterer = filt(&[], &["/Cargo.*"], &[], &[], &[]).await;

	filterer.file_doesnt_pass("Cargo.toml");
	filterer.file_doesnt_pass("Cargo.json");
	filterer.dir_doesnt_pass("Cargo.toml");
	filterer.unk_doesnt_pass("Cargo.toml");
	filterer.file_does_pass("foo/Cargo.toml");
	filterer.dir_does_pass("foo/Cargo.toml");
}

#[tokio::test]
async fn ignore_glob_leading_double_star() {
	let filterer = filt(&[], &["**/possum"], &[], &[], &[]).await;

	filterer.file_doesnt_pass("possum");
	filterer.file_doesnt_pass("foo/bar/possum");
	filterer.file_doesnt_pass("/foo/bar/possum");
	filterer.dir_doesnt_pass("possum");
	filterer.dir_doesnt_pass("foo/bar/possum");
	filterer.dir_doesnt_pass("/foo/bar/possum");
	filterer.file_does_pass("rat");
	filterer.file_does_pass("foo/bar/rat");
	filterer.file_does_pass("/foo/bar/rat");
}

#[tokio::test]
async fn ignore_glob_trailing_double_star() {
	let filterer = filt(&[], &["possum/**"], &[], &[], &[]).await;

	filterer.file_does_pass("possum");
	filterer.file_doesnt_pass("possum/foo/bar");
	filterer.file_does_pass("/possum/foo/bar");
	filterer.file_doesnt_pass("/test/possum/foo/bar");
	filterer.dir_does_pass("possum");
	filterer.dir_does_pass("foo/bar/possum");
	filterer.dir_does_pass("/foo/bar/possum");
	filterer.dir_doesnt_pass("possum/foo/bar");
	filterer.dir_does_pass("/possum/foo/bar");
	filterer.dir_doesnt_pass("/test/possum/foo/bar");
	filterer.file_does_pass("rat");
	filterer.file_does_pass("foo/bar/rat");
	filterer.file_does_pass("/foo/bar/rat");
}

#[tokio::test]
async fn ignore_glob_middle_double_star() {
	let filterer = filt(&[], &["apples/**/oranges"], &[], &[], &[]).await;

	filterer.dir_does_pass("/a/folder");
	filterer.file_doesnt_pass("apples/carrots/oranges");
	filterer.file_doesnt_pass("apples/carrots/cauliflowers/oranges");
	filterer.file_doesnt_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.file_doesnt_pass("apples/oranges/bananas");
	filterer.dir_doesnt_pass("apples/carrots/oranges");
	filterer.dir_doesnt_pass("apples/carrots/cauliflowers/oranges");
	filterer.dir_doesnt_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.dir_doesnt_pass("apples/oranges/bananas");
}

#[tokio::test]
async fn ignore_glob_double_star_trailing_slash() {
	let filterer = filt(&[], &["apples/**/oranges/"], &[], &[], &[]).await;

	filterer.dir_does_pass("/a/folder");
	filterer.file_does_pass("apples/carrots/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.file_doesnt_pass("apples/oranges/bananas");
	filterer.dir_doesnt_pass("apples/carrots/oranges");
	filterer.dir_doesnt_pass("apples/carrots/cauliflowers/oranges");
	filterer.dir_doesnt_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.dir_doesnt_pass("apples/oranges/bananas");
	filterer.unk_does_pass("apples/carrots/oranges");
	filterer.unk_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.unk_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
}

#[tokio::test]
async fn ignores_take_precedence() {
	let filterer = filt(
		&["*.docx", "*.toml", "*.json"],
		&["*.toml", "*.json"],
		&[],
		&[],
		&[],
	)
	.await;

	filterer.file_doesnt_pass("Cargo.toml");
	filterer.file_doesnt_pass("/test/foo/bar/Cargo.toml");
	filterer.file_doesnt_pass("package.json");
	filterer.file_doesnt_pass("/test/foo/bar/package.json");
	filterer.dir_doesnt_pass("/test/Cargo.toml");
	filterer.dir_doesnt_pass("/test/package.json");
	filterer.file_does_pass("FINAL-FINAL.docx");
}

#[tokio::test]
async fn extensions_fail_dirs() {
	let filterer = filt(&[], &[], &[], &["py"], &[]).await;

	filterer.file_does_pass("Cargo.py");
	filterer.file_doesnt_pass("Cargo.toml");
	filterer.dir_doesnt_pass("Cargo");
	filterer.dir_doesnt_pass("Cargo.toml");
	filterer.dir_doesnt_pass("Cargo.py");
}

#[tokio::test]
async fn extensions_fail_extensionless() {
	let filterer = filt(&[], &[], &[], &["py"], &[]).await;

	filterer.file_does_pass("Cargo.py");
	filterer.file_doesnt_pass("Cargo");
}

#[tokio::test]
async fn multipath_allow_on_any_one_pass() {
	use watchexec::filter::Filterer;
	use watchexec_events::{Event, FileType, Tag};

	let filterer = filt(&[], &[], &[], &["py"], &[]).await;
	let origin = tokio::fs::canonicalize(".").await.unwrap();

	let event = Event {
		tags: vec![
			Tag::Path {
				path: origin.join("Cargo.py"),
				file_type: Some(FileType::File),
			},
			Tag::Path {
				path: origin.join("Cargo.toml"),
				file_type: Some(FileType::File),
			},
			Tag::Path {
				path: origin.join("Cargo.py"),
				file_type: Some(FileType::Dir),
			},
		],
		metadata: Default::default(),
	};

	assert!(filterer.check_event(&event, Priority::Normal).unwrap());
}

#[tokio::test]
async fn extensions_and_filters_glob() {
	let filterer = filt(&["*/justfile"], &[], &[], &["md", "css"], &[]).await;

	filterer.file_does_pass("foo/justfile");
	filterer.file_does_pass("bar.md");
	filterer.file_does_pass("qux.css");
	filterer.file_doesnt_pass("nope.py");

	// Watchexec 1.x buggy behaviour, should not pass
	#[cfg(unix)]
	filterer.file_does_pass("justfile");
}

#[tokio::test]
async fn extensions_and_filters_slash() {
	let filterer = filt(&["/justfile"], &[], &[], &["md", "css"], &[]).await;

	filterer.file_does_pass("justfile");
	filterer.file_does_pass("bar.md");
	filterer.file_does_pass("qux.css");
	filterer.file_doesnt_pass("nope.py");
}

#[tokio::test]
async fn leading_single_glob_file() {
	let filterer = filt(&["*/justfile"], &[], &[], &[], &[]).await;

	filterer.file_does_pass("foo/justfile");
	filterer.file_doesnt_pass("notfile");
	filterer.file_doesnt_pass("not/thisfile");

	// Watchexec 1.x buggy behaviour, should not pass
	#[cfg(unix)]
	filterer.file_does_pass("justfile");
}

#[tokio::test]
async fn nonpath_event_passes() {
	use watchexec::filter::Filterer;
	use watchexec_events::{Event, Source, Tag};

	let filterer = filt(&[], &[], &[], &["py"], &[]).await;

	assert!(filterer
		.check_event(
			&Event {
				tags: vec![Tag::Source(Source::Internal)],
				metadata: Default::default(),
			},
			Priority::Normal
		)
		.unwrap());

	assert!(filterer
		.check_event(
			&Event {
				tags: vec![Tag::Source(Source::Keyboard)],
				metadata: Default::default(),
			},
			Priority::Normal
		)
		.unwrap());
}

// Folder ignore patterns reject descendant events when their source directory would be pruned.

#[tokio::test]
async fn ignore_folder_with_bare_match() {
	let filterer = filt(&[], &["prunes"], &[], &[], &[]).await;

	filterer.file_does_pass("apples");
	filterer.file_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("apples/oranges/bananas");
	filterer.dir_does_pass("apples");
	filterer.dir_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("apples/carrots/cauliflowers/artichokes/oranges");

	filterer.file_does_pass("raw-prunes");
	filterer.dir_does_pass("raw-prunes");
	filterer.file_does_pass("raw-prunes/carrots/cauliflowers/oranges");
	filterer.file_does_pass("raw-prunes/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("raw-prunes/oranges/bananas");
	filterer.dir_does_pass("raw-prunes/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("raw-prunes/carrots/cauliflowers/artichokes/oranges");

	filterer.file_doesnt_pass("prunes");
	filterer.dir_doesnt_pass("prunes");

	filterer.file_doesnt_pass("prunes/carrots/cauliflowers/oranges");
	filterer.file_doesnt_pass("prunes/carrots/cauliflowers/artichokes/oranges");
	filterer.file_doesnt_pass("prunes/oranges/bananas");
	filterer.dir_doesnt_pass("prunes/carrots/cauliflowers/oranges");
	filterer.dir_doesnt_pass("prunes/carrots/cauliflowers/artichokes/oranges");
}

#[tokio::test]
async fn ignore_folder_with_bare_and_leading_slash() {
	let filterer = filt(&[], &["/prunes"], &[], &[], &[]).await;

	filterer.file_does_pass("apples");
	filterer.file_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("apples/oranges/bananas");
	filterer.dir_does_pass("apples");
	filterer.dir_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("apples/carrots/cauliflowers/artichokes/oranges");

	filterer.file_does_pass("raw-prunes");
	filterer.dir_does_pass("raw-prunes");
	filterer.file_does_pass("raw-prunes/carrots/cauliflowers/oranges");
	filterer.file_does_pass("raw-prunes/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("raw-prunes/oranges/bananas");
	filterer.dir_does_pass("raw-prunes/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("raw-prunes/carrots/cauliflowers/artichokes/oranges");

	filterer.file_doesnt_pass("prunes");
	filterer.dir_doesnt_pass("prunes");

	filterer.file_doesnt_pass("prunes/carrots/cauliflowers/oranges");
	filterer.file_doesnt_pass("prunes/carrots/cauliflowers/artichokes/oranges");
	filterer.file_doesnt_pass("prunes/oranges/bananas");
	filterer.dir_doesnt_pass("prunes/carrots/cauliflowers/oranges");
	filterer.dir_doesnt_pass("prunes/carrots/cauliflowers/artichokes/oranges");
}

#[tokio::test]
async fn ignore_folder_with_bare_and_trailing_slash() {
	let filterer = filt(&[], &["prunes/"], &[], &[], &[]).await;

	filterer.file_does_pass("apples");
	filterer.file_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("apples/oranges/bananas");
	filterer.dir_does_pass("apples");
	filterer.dir_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("apples/carrots/cauliflowers/artichokes/oranges");

	filterer.file_does_pass("raw-prunes");
	filterer.dir_does_pass("raw-prunes");
	filterer.file_does_pass("raw-prunes/carrots/cauliflowers/oranges");
	filterer.file_does_pass("raw-prunes/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("raw-prunes/oranges/bananas");
	filterer.dir_does_pass("raw-prunes/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("raw-prunes/carrots/cauliflowers/artichokes/oranges");

	filterer.dir_doesnt_pass("prunes");

	// The directory-only glob does not ignore a file at the boundary.
	filterer.file_does_pass("prunes");
	filterer.file_doesnt_pass("prunes/carrots/cauliflowers/oranges");
	filterer.file_doesnt_pass("prunes/carrots/cauliflowers/artichokes/oranges");
	filterer.file_doesnt_pass("prunes/oranges/bananas");
	filterer.dir_doesnt_pass("prunes/carrots/cauliflowers/oranges");
	filterer.dir_doesnt_pass("prunes/carrots/cauliflowers/artichokes/oranges");
}

#[tokio::test]
async fn ignore_folder_with_only_double_double_glob() {
	let filterer = filt(&[], &["**/prunes/**"], &[], &[], &[]).await;

	filterer.file_does_pass("apples");
	filterer.file_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("apples/oranges/bananas");
	filterer.dir_does_pass("apples");
	filterer.dir_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("apples/carrots/cauliflowers/artichokes/oranges");

	filterer.file_does_pass("raw-prunes");
	filterer.dir_does_pass("raw-prunes");
	filterer.file_does_pass("raw-prunes/carrots/cauliflowers/oranges");
	filterer.file_does_pass("raw-prunes/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("raw-prunes/oranges/bananas");
	filterer.dir_does_pass("raw-prunes/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("raw-prunes/carrots/cauliflowers/artichokes/oranges");

	filterer.file_doesnt_pass("prunes/carrots/cauliflowers/oranges");
	filterer.file_doesnt_pass("prunes/carrots/cauliflowers/artichokes/oranges");
	filterer.file_doesnt_pass("prunes/oranges/bananas");
	filterer.dir_doesnt_pass("prunes/carrots/cauliflowers/oranges");
	filterer.dir_doesnt_pass("prunes/carrots/cauliflowers/artichokes/oranges");

	// A trailing `/**` does not match the directory boundary, so traversal remains possible.
	filterer.file_does_pass("prunes");
	filterer.dir_does_pass("prunes");
}

#[tokio::test]
async fn ignore_folder_correctly_with_double_and_double_double_globs() {
	let filterer = filt(&[], &["**/prunes", "**/prunes/**"], &[], &[], &[]).await;

	filterer.file_does_pass("apples");
	filterer.file_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("apples/oranges/bananas");
	filterer.dir_does_pass("apples");
	filterer.dir_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("apples/carrots/cauliflowers/artichokes/oranges");

	filterer.file_does_pass("raw-prunes");
	filterer.dir_does_pass("raw-prunes");
	filterer.file_does_pass("raw-prunes/carrots/cauliflowers/oranges");
	filterer.file_does_pass("raw-prunes/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("raw-prunes/oranges/bananas");
	filterer.dir_does_pass("raw-prunes/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("raw-prunes/carrots/cauliflowers/artichokes/oranges");

	filterer.file_doesnt_pass("prunes");
	filterer.file_doesnt_pass("prunes/carrots/cauliflowers/oranges");
	filterer.file_doesnt_pass("prunes/carrots/cauliflowers/artichokes/oranges");
	filterer.file_doesnt_pass("prunes/oranges/bananas");
	filterer.dir_doesnt_pass("prunes");
	filterer.dir_doesnt_pass("prunes/carrots/cauliflowers/oranges");
	filterer.dir_doesnt_pass("prunes/carrots/cauliflowers/artichokes/oranges");
}

#[tokio::test]
async fn descendant_negation_does_not_reopen_ignored_parent() {
	let filterer = filt(&[], &["parent/", "!parent/child"], &[], &[], &[]).await;

	filterer.dir_doesnt_pass("parent");
	filterer.file_doesnt_pass("parent/child");
	assert_source_dir(&filterer, "parent", false);
	assert_source_dir(&filterer, "parent/child", false);
}

#[tokio::test]
async fn explicitly_unignored_parent_allows_descendant_negation() {
	let filterer = filt(
		&[],
		&["parent/", "!parent/", "parent/*", "!parent/child"],
		&[],
		&[],
		&[],
	)
	.await;

	filterer.dir_does_pass("parent");
	filterer.file_does_pass("parent/child");
	filterer.file_doesnt_pass("parent/sibling");
	assert_source_dir(&filterer, "parent", true);
	assert_source_dir(&filterer, "parent/child", true);
	assert_source_dir(&filterer, "parent/sibling", false);
}

#[tokio::test]
async fn whitelist_overrides_ignore() {
	let filterer = filt(&[], &["**/prunes"], &["/prunes"], &[], &[]).await;

	filterer.file_does_pass("apples");
	filterer.file_does_pass("/prunes");
	filterer.dir_does_pass("apples");
	filterer.dir_does_pass("/prunes");

	filterer.file_does_pass("raw-prunes");
	filterer.dir_does_pass("raw-prunes");

	filterer.file_doesnt_pass("apples/prunes");
	filterer.file_doesnt_pass("raw/prunes");
	filterer.dir_doesnt_pass("apples/prunes");
	filterer.dir_doesnt_pass("raw/prunes");
}

#[tokio::test]
async fn whitelist_overrides_ignore_files() {
	let mut ignore_file = tempfile::NamedTempFile::new().unwrap();
	let _ = ignore_file.write(b"prunes");

	let origin = std::fs::canonicalize(".").unwrap();
	let whitelist = origin.join("prunes").display().to_string();

	let filterer = filt(
		&[],
		&[],
		&[&whitelist],
		&[],
		&[ignore_file.path().to_path_buf()],
	)
	.await;

	filterer.file_does_pass("apples");
	filterer.file_does_pass("prunes");
	filterer.dir_does_pass("apples");
	filterer.dir_does_pass("prunes");

	filterer.file_does_pass("raw-prunes");
	filterer.dir_does_pass("raw-prunes");

	filterer.file_doesnt_pass("apples/prunes");
	filterer.file_doesnt_pass("raw/prunes");
	filterer.dir_doesnt_pass("apples/prunes");
	filterer.dir_doesnt_pass("raw/prunes");
}

#[tokio::test]
async fn whitelist_overrides_ignore_files_nested() {
	let mut ignore_file = tempfile::NamedTempFile::new().unwrap();
	let _ = ignore_file.write(b"prunes\n");

	let origin = std::fs::canonicalize(".").unwrap();
	let whitelist = origin.join("prunes").join("target").display().to_string();

	let filterer = filt(
		&[],
		&[],
		&[&whitelist],
		&[],
		&[ignore_file.path().to_path_buf()],
	)
	.await;

	filterer.file_does_pass("apples");
	filterer.file_doesnt_pass("prunes");
	filterer.dir_does_pass("apples");
	filterer.dir_doesnt_pass("prunes");

	filterer.file_does_pass("raw-prunes");
	filterer.dir_does_pass("raw-prunes");

	filterer.file_doesnt_pass("prunes/apples");
	filterer.file_doesnt_pass("prunes/raw");
	filterer.dir_doesnt_pass("prunes/apples");
	filterer.dir_doesnt_pass("prunes/raw");

	filterer.file_doesnt_pass("apples/prunes");
	filterer.file_doesnt_pass("raw/prunes");
	filterer.dir_doesnt_pass("apples/prunes");
	filterer.dir_doesnt_pass("raw/prunes");

	filterer.file_does_pass("prunes/target");
	filterer.dir_does_pass("prunes/target");

	filterer.file_doesnt_pass("prunes/nested/target");
	filterer.dir_doesnt_pass("prunes/nested/target");
}
