mod helpers;
use helpers::globset::*;

#[tokio::test]
async fn empty_filter_passes_everything() {
	let filterer = filt(&[], &[], &[]).await;

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
	let filterer = filt(&["Cargo.toml"], &[], &[]).await;

	filterer.file_does_pass("Cargo.toml");
	filterer.file_does_pass("/test/foo/bar/Cargo.toml");
	filterer.file_doesnt_pass("Cargo.json");
	filterer.file_doesnt_pass("Gemfile.toml");
	filterer.file_doesnt_pass("FINAL-FINAL.docx");
	filterer.dir_doesnt_pass("/a/folder");
	filterer.dir_does_pass("/test/Cargo.toml");
}

#[tokio::test]
async fn exact_filenames_multiple() {
	let filterer = filt(&["Cargo.toml", "package.json"], &[], &[]).await;

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
	let filterer = filt(&["Cargo.*"], &[], &[]).await;

	filterer.file_does_pass("Cargo.toml");
	filterer.file_does_pass("Cargo.json");
	filterer.file_doesnt_pass("Gemfile.toml");
	filterer.file_doesnt_pass("FINAL-FINAL.docx");
	filterer.dir_doesnt_pass("/a/folder");
	filterer.dir_does_pass("Cargo.toml");
}

#[tokio::test]
async fn glob_star_trailing_slash() {
	let filterer = filt(&["Cargo.*/"], &[], &[]).await;

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
	let filterer = filt(&["/Cargo.*"], &[], &[]).await;

	filterer.file_does_pass("Cargo.toml");
	filterer.file_does_pass("Cargo.json");
	filterer.dir_does_pass("Cargo.toml");
	filterer.unk_does_pass("Cargo.toml");
	filterer.file_doesnt_pass("foo/Cargo.toml");
	filterer.dir_doesnt_pass("foo/Cargo.toml");
}

#[tokio::test]
async fn glob_leading_double_star() {
	let filterer = filt(&["**/possum"], &[], &[]).await;

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
	let filterer = filt(&["possum/**"], &[], &[]).await;

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
	let filterer = filt(&["apples/**/oranges"], &[], &[]).await;

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
	let filterer = filt(&["apples/**/oranges/"], &[], &[]).await;

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
	let filterer = filt(&[], &["Cargo.toml"], &[]).await;

	filterer.file_doesnt_pass("Cargo.toml");
	filterer.file_doesnt_pass("/test/foo/bar/Cargo.toml");
	filterer.file_does_pass("Cargo.json");
	filterer.file_does_pass("Gemfile.toml");
	filterer.file_does_pass("FINAL-FINAL.docx");
	filterer.dir_does_pass("/a/folder");
	filterer.dir_doesnt_pass("/test/Cargo.toml");
}

#[tokio::test]
async fn ignore_exact_filenames_multiple() {
	let filterer = filt(&[], &["Cargo.toml", "package.json"], &[]).await;

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
	let filterer = filt(&[], &["Cargo.*"], &[]).await;

	filterer.file_doesnt_pass("Cargo.toml");
	filterer.file_doesnt_pass("Cargo.json");
	filterer.file_does_pass("Gemfile.toml");
	filterer.file_does_pass("FINAL-FINAL.docx");
	filterer.dir_does_pass("/a/folder");
	filterer.dir_doesnt_pass("Cargo.toml");
}

#[tokio::test]
async fn ignore_glob_star_trailing_slash() {
	let filterer = filt(&[], &["Cargo.*/"], &[]).await;

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
	let filterer = filt(&[], &["/Cargo.*"], &[]).await;

	filterer.file_doesnt_pass("Cargo.toml");
	filterer.file_doesnt_pass("Cargo.json");
	filterer.dir_doesnt_pass("Cargo.toml");
	filterer.unk_doesnt_pass("Cargo.toml");
	filterer.file_does_pass("foo/Cargo.toml");
	filterer.dir_does_pass("foo/Cargo.toml");
}

#[tokio::test]
async fn ignore_glob_leading_double_star() {
	let filterer = filt(&[], &["**/possum"], &[]).await;

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
	let filterer = filt(&[], &["possum/**"], &[]).await;

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
	let filterer = filt(&[], &["apples/**/oranges"], &[]).await;

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
	let filterer = filt(&[], &["apples/**/oranges/"], &[]).await;

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
	let filterer = filt(&["*.docx", "*.toml", "*.json"], &["*.toml", "*.json"], &[]).await;

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
	let filterer = filt(&[], &[], &["py"]).await;

	filterer.file_does_pass("Cargo.py");
	filterer.file_doesnt_pass("Cargo.toml");
	filterer.dir_doesnt_pass("Cargo");
	filterer.dir_doesnt_pass("Cargo.toml");
	filterer.dir_doesnt_pass("Cargo.py");
}

#[tokio::test]
async fn extensions_fail_extensionless() {
	let filterer = filt(&[], &[], &["py"]).await;

	filterer.file_does_pass("Cargo.py");
	filterer.file_doesnt_pass("Cargo");
}

// The following tests replicate the "buggy"/"confusing" watchexec v1 behaviour.

#[tokio::test]
async fn ignore_folder_incorrectly_with_bare_match() {
	let filterer = filt(&[], &["prunes"], &[]).await;

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
	let filterer = filt(&[], &["/prunes"], &[]).await;

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
	let filterer = filt(&[], &["prunes/"], &[]).await;

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
	let filterer = filt(&[], &["**/prunes/**"], &[]).await;

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
	let filterer = filt(&[], &["**/prunes", "**/prunes/**"], &[]).await;

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
