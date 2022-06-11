use watchexec::ignore::IgnoreFilterer;

mod helpers;
use helpers::ignore::*;

#[tokio::test]
async fn folders() {
	let filterer = filt("", &[file("folders")]).await;

	filterer.file_doesnt_pass("prunes");
	filterer.dir_doesnt_pass("prunes");
	folders_suite(&filterer, "prunes");

	filterer.file_doesnt_pass("apricots");
	filterer.dir_doesnt_pass("apricots");
	folders_suite(&filterer, "apricots");

	filterer.file_does_pass("cherries");
	filterer.dir_doesnt_pass("cherries");
	folders_suite(&filterer, "cherries");

	filterer.file_does_pass("grapes");
	filterer.dir_does_pass("grapes");
	folders_suite(&filterer, "grapes");

	filterer.file_doesnt_pass("feijoa");
	filterer.dir_doesnt_pass("feijoa");
	folders_suite(&filterer, "feijoa");
}

fn folders_suite(filterer: &IgnoreFilterer, name: &str) {
	filterer.file_does_pass("apples");
	filterer.file_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("apples/oranges/bananas");
	filterer.dir_does_pass("apples");
	filterer.dir_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("apples/carrots/cauliflowers/artichokes/oranges");

	filterer.file_does_pass(&format!("raw-{}", name));
	filterer.dir_does_pass(&format!("raw-{}", name));
	filterer.file_does_pass(&format!("raw-{}/carrots/cauliflowers/oranges", name));
	filterer.file_does_pass(&format!("raw-{}/oranges/bananas", name));
	filterer.dir_does_pass(&format!("raw-{}/carrots/cauliflowers/oranges", name));
	filterer.file_does_pass(&format!(
		"raw-{}/carrots/cauliflowers/artichokes/oranges",
		name
	));
	filterer.dir_does_pass(&format!(
		"raw-{}/carrots/cauliflowers/artichokes/oranges",
		name
	));

	filterer.dir_doesnt_pass(&format!("{}/carrots/cauliflowers/oranges", name));
	filterer.dir_doesnt_pass(&format!("{}/carrots/cauliflowers/artichokes/oranges", name));
	filterer.file_doesnt_pass(&format!("{}/carrots/cauliflowers/oranges", name));
	filterer.file_doesnt_pass(&format!("{}/carrots/cauliflowers/artichokes/oranges", name));
	filterer.file_doesnt_pass(&format!("{}/oranges/bananas", name));
}

#[tokio::test]
async fn globs() {
	let filterer = filt("", &[file("globs")]).await;

	// Unmatched
	filterer.file_does_pass("FINAL-FINAL.docx");
	filterer.dir_does_pass("/a/folder");
	filterer.file_does_pass("rat");
	filterer.file_does_pass("foo/bar/rat");
	filterer.file_does_pass("/foo/bar/rat");

	// Cargo.toml
	filterer.file_doesnt_pass("Cargo.toml");
	filterer.dir_doesnt_pass("Cargo.toml");
	filterer.file_does_pass("Cargo.json");

	// package.json
	filterer.file_doesnt_pass("package.json");
	filterer.dir_doesnt_pass("package.json");
	filterer.file_does_pass("package.toml");

	// *.gemspec
	filterer.file_doesnt_pass("pearl.gemspec");
	filterer.dir_doesnt_pass("sapphire.gemspec");
	filterer.file_doesnt_pass(".gemspec");
	filterer.file_does_pass("diamond.gemspecial");

	// test-*
	filterer.file_doesnt_pass("test-unit");
	filterer.dir_doesnt_pass("test-integration");
	filterer.file_does_pass("tester-helper");

	// *.sw*
	filterer.file_doesnt_pass("source.file.swa");
	filterer.file_doesnt_pass(".source.file.swb");
	filterer.dir_doesnt_pass("source.folder.swd");
	filterer.file_does_pass("other.thing.s_w");

	// sources.*/
	filterer.file_does_pass("sources.waters");
	filterer.dir_doesnt_pass("sources.rivers");

	// /output.*
	filterer.file_doesnt_pass("output.toml");
	filterer.file_doesnt_pass("output.json");
	filterer.dir_doesnt_pass("output.toml");
	filterer.unk_doesnt_pass("output.toml");
	filterer.file_does_pass("foo/output.toml");
	filterer.dir_does_pass("foo/output.toml");

	// **/possum
	filterer.file_doesnt_pass("possum");
	filterer.file_doesnt_pass("foo/bar/possum");
	filterer.file_doesnt_pass("/foo/bar/possum");
	filterer.dir_doesnt_pass("possum");
	filterer.dir_doesnt_pass("foo/bar/possum");
	filterer.dir_doesnt_pass("/foo/bar/possum");

	// zebra/**
	filterer.file_does_pass("zebra");
	filterer.file_doesnt_pass("zebra/foo/bar");
	filterer.file_does_pass("/zebra/foo/bar");
	filterer.file_doesnt_pass("/test/zebra/foo/bar");
	filterer.dir_does_pass("zebra");
	filterer.dir_does_pass("foo/bar/zebra");
	filterer.dir_does_pass("/foo/bar/zebra");
	filterer.dir_doesnt_pass("zebra/foo/bar");
	filterer.dir_does_pass("/zebra/foo/bar");
	filterer.dir_doesnt_pass("/test/zebra/foo/bar");

	// elep/**/hant
	filterer.file_doesnt_pass("elep/carrots/hant");
	filterer.file_doesnt_pass("elep/carrots/cauliflowers/hant");
	filterer.file_doesnt_pass("elep/carrots/cauliflowers/artichokes/hant");
	filterer.dir_doesnt_pass("elep/carrots/hant");
	filterer.dir_doesnt_pass("elep/carrots/cauliflowers/hant");
	filterer.dir_doesnt_pass("elep/carrots/cauliflowers/artichokes/hant");
	filterer.file_doesnt_pass("elep/hant/bananas");
	filterer.dir_doesnt_pass("elep/hant/bananas");

	// song/**/bird/
	filterer.file_does_pass("song/carrots/bird");
	filterer.file_does_pass("song/carrots/cauliflowers/bird");
	filterer.file_does_pass("song/carrots/cauliflowers/artichokes/bird");
	filterer.dir_doesnt_pass("song/carrots/bird");
	filterer.dir_doesnt_pass("song/carrots/cauliflowers/bird");
	filterer.dir_doesnt_pass("song/carrots/cauliflowers/artichokes/bird");
	filterer.unk_does_pass("song/carrots/bird");
	filterer.unk_does_pass("song/carrots/cauliflowers/bird");
	filterer.unk_does_pass("song/carrots/cauliflowers/artichokes/bird");
	filterer.file_doesnt_pass("song/bird/bananas");
	filterer.dir_doesnt_pass("song/bird/bananas");
}

#[tokio::test]
async fn negate() {
	let filterer = filt("", &[file("negate")]).await;

	filterer.file_does_pass("yeah");
	filterer.file_doesnt_pass("nah");
	filterer.file_does_pass("nah.yeah");
}

#[tokio::test]
async fn allowlist() {
	let filterer = filt("", &[file("allowlist")]).await;

	filterer.file_does_pass("mod.go");
	filterer.file_does_pass("foo.go");
	filterer.file_does_pass("go.sum");
	filterer.file_does_pass("go.mod");
	filterer.file_does_pass("README.md");
	filterer.file_does_pass("LICENSE");
	filterer.file_does_pass(".gitignore");

	filterer.file_doesnt_pass("evil.sum");
	filterer.file_doesnt_pass("evil.mod");
	filterer.file_doesnt_pass("gofile.gone");
	filterer.file_doesnt_pass("go.js");
	filterer.file_doesnt_pass("README.asciidoc");
	filterer.file_doesnt_pass("LICENSE.txt");
	filterer.file_doesnt_pass("foo/.gitignore");
}

#[tokio::test]
async fn scopes() {
	let filterer = filt(
		"",
		&[
			file("scopes-global"),
			file("scopes-local").applies_in(""),
			file("scopes-sublocal").applies_in("tests"),
		],
	)
	.await;

	filterer.file_doesnt_pass("global.a");
	filterer.file_doesnt_pass("/global.b");
	filterer.file_doesnt_pass("tests/global.c");

	filterer.file_doesnt_pass("local.a");
	filterer.file_does_pass("/local.b");
	filterer.file_doesnt_pass("tests/local.c");

	filterer.file_does_pass("sublocal.a");
	filterer.file_does_pass("/sublocal.b");
	filterer.file_doesnt_pass("tests/sublocal.c");
}

#[tokio::test]
async fn self_ignored() {
	let filterer = filt("", &[file("self.ignore").applies_in("tests/ignores")]).await;

	filterer.file_doesnt_pass("tests/ignores/self.ignore");
	filterer.file_does_pass("self.ignore");
}
