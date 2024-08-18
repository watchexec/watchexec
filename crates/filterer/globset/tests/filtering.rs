mod helpers;
use helpers::globset::*;
use std::io::Write;

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
	filterer.file_does_pass("apples/oranges/bananas");
	filterer.dir_doesnt_pass("apples/carrots/oranges");
	filterer.dir_doesnt_pass("apples/carrots/cauliflowers/oranges");
	filterer.dir_doesnt_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.dir_does_pass("apples/oranges/bananas");
}

#[tokio::test]
async fn ignore_glob_double_star_trailing_slash() {
	let filterer = filt(&[], &["apples/**/oranges/"], &[], &[], &[]).await;

	filterer.dir_does_pass("/a/folder");
	filterer.file_does_pass("apples/carrots/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("apples/oranges/bananas");
	filterer.dir_doesnt_pass("apples/carrots/oranges");
	filterer.dir_doesnt_pass("apples/carrots/cauliflowers/oranges");
	filterer.dir_doesnt_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.dir_does_pass("apples/oranges/bananas");
	filterer.unk_does_pass("apples/carrots/oranges");
	filterer.unk_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.unk_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
}

#[tokio::test]
async fn ignores_take_precedence() {
	let filterer = filt(&["*.docx", "*.toml", "*.json"], &["*.toml", "*.json"], &[], &[], &[]).await;

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

// The following tests replicate the "buggy"/"confusing" watchexec v1 behaviour.

#[tokio::test]
async fn ignore_folder_incorrectly_with_bare_match() {
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

	// buggy behaviour (should be doesnt):
	filterer.file_does_pass("prunes/carrots/cauliflowers/oranges");
	filterer.file_does_pass("prunes/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("prunes/oranges/bananas");
	filterer.dir_does_pass("prunes/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("prunes/carrots/cauliflowers/artichokes/oranges");
}

#[tokio::test]
async fn ignore_folder_incorrectly_with_bare_and_leading_slash() {
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

	// buggy behaviour (should be doesnt):
	filterer.file_does_pass("prunes/carrots/cauliflowers/oranges");
	filterer.file_does_pass("prunes/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("prunes/oranges/bananas");
	filterer.dir_does_pass("prunes/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("prunes/carrots/cauliflowers/artichokes/oranges");
}

#[tokio::test]
async fn ignore_folder_incorrectly_with_bare_and_trailing_slash() {
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

	// buggy behaviour (should be doesnt):
	filterer.file_does_pass("prunes");
	filterer.file_does_pass("prunes/carrots/cauliflowers/oranges");
	filterer.file_does_pass("prunes/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("prunes/oranges/bananas");
	filterer.dir_does_pass("prunes/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("prunes/carrots/cauliflowers/artichokes/oranges");
}

#[tokio::test]
async fn ignore_folder_incorrectly_with_only_double_double_glob() {
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

	// buggy behaviour (should be doesnt):
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

	let filterer = filt(&[], &[], &[&whitelist], &[], &[ignore_file.path().to_path_buf()]).await;

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

	let filterer = filt(&[], &[], &[&whitelist], &[], &[ignore_file.path().to_path_buf()]).await;

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
