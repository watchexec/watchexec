mod helpers;

use std::path::Path;

use helpers::ignore_tests::*;
use ignore::Match;
use ignore_files::IgnoreFilter;

fn path_is_ignored(filter: &IgnoreFilter, path: &Path, is_dir: bool) -> bool {
	matches!(
		filter.match_path_or_ancestors(path, is_dir),
		Match::Ignore(glob) if glob.from().map_or(true, |from| path.starts_with(from))
	)
}

#[tokio::test]
async fn globals() {
	let filter = filt(
		"tree",
		&[
			file("global/first").applies_globally(),
			file("global/second").applies_globally(),
		],
	)
	.await;

	// Both ignores should be loaded as global
	filter.agnostic_fail("/apples");
	filter.agnostic_fail("/oranges");

	// Sanity check
	filter.agnostic_pass("/kiwi");
}

#[tokio::test]
async fn tree() {
	let filter = filt("tree", &[file("tree/base"), file("tree/branch/inner")]).await;

	// "oranges" is not ignored at any level
	filter.agnostic_pass("tree/oranges");
	filter.agnostic_pass("tree/branch/oranges");
	filter.agnostic_pass("tree/branch/inner/oranges");
	filter.agnostic_pass("tree/other/oranges");

	// "apples" should only be ignored at the root
	filter.agnostic_fail("tree/apples");
	filter.agnostic_pass("tree/branch/apples");
	filter.agnostic_pass("tree/branch/inner/apples");
	filter.agnostic_pass("tree/other/apples");

	// "carrots" should be ignored at any level
	filter.agnostic_fail("tree/carrots");
	filter.agnostic_fail("tree/branch/carrots");
	filter.agnostic_fail("tree/branch/inner/carrots");
	filter.agnostic_fail("tree/other/carrots");

	// "pineapples/grapes" should only be ignored at the root
	filter.agnostic_fail("tree/pineapples/grapes");
	filter.agnostic_pass("tree/branch/pineapples/grapes");
	filter.agnostic_pass("tree/branch/inner/pineapples/grapes");
	filter.agnostic_pass("tree/other/pineapples/grapes");

	// "cauliflowers" should only be ignored at the root of "branch/"
	filter.agnostic_pass("tree/cauliflowers");
	filter.agnostic_fail("tree/branch/cauliflowers");
	filter.agnostic_pass("tree/branch/inner/cauliflowers");
	filter.agnostic_pass("tree/other/cauliflowers");

	// "artichokes" should be ignored anywhere inside of "branch/"
	filter.agnostic_pass("tree/artichokes");
	filter.agnostic_fail("tree/branch/artichokes");
	filter.agnostic_fail("tree/branch/inner/artichokes");
	filter.agnostic_pass("tree/other/artichokes");

	// "bananas/pears" should only be ignored at the root of "branch/"
	filter.agnostic_pass("tree/bananas/pears");
	filter.agnostic_fail("tree/branch/bananas/pears");
	filter.agnostic_pass("tree/branch/inner/bananas/pears");
	filter.agnostic_pass("tree/other/bananas/pears");
}

#[tokio::test]
async fn nested_negation_does_not_reopen_ignored_parent() {
	let origin = std::fs::canonicalize("tests/tree").unwrap();
	let branch = origin.join("branch");
	let mut filter = IgnoreFilter::new(&origin, &[]).await.unwrap();
	filter
		.add_globs(&["branch/parent/"], Some(&origin))
		.unwrap();
	filter.add_globs(&["!parent/child"], Some(&branch)).unwrap();

	let parent = branch.join("parent");
	let child = parent.join("child");
	assert!(!filter.check_dir(&parent));
	assert!(path_is_ignored(&filter, &child, false));
	assert!(!filter.check_dir(&child));
}

#[tokio::test]
async fn nested_negation_works_when_parent_is_explicitly_unignored() {
	let origin = std::fs::canonicalize("tests/tree").unwrap();
	let branch = origin.join("branch");
	let mut filter = IgnoreFilter::new(&origin, &[]).await.unwrap();
	filter
		.add_globs(&["branch/parent/"], Some(&origin))
		.unwrap();
	filter
		.add_globs(&["!parent/", "parent/*", "!parent/child"], Some(&branch))
		.unwrap();

	let parent = branch.join("parent");
	let child = parent.join("child");
	let sibling = parent.join("sibling");
	assert!(filter.check_dir(&parent));
	assert!(!path_is_ignored(&filter, &child, false));
	assert!(filter.check_dir(&child));
	assert!(path_is_ignored(&filter, &sibling, false));
	assert!(!filter.check_dir(&sibling));
}

#[tokio::test]
async fn scoped_ignores_respect_path_component_boundaries() {
	let origin = std::fs::canonicalize("tests/tree").unwrap();
	let scoped = origin.join("tests");
	let mut filter = IgnoreFilter::new(&origin, &[]).await.unwrap();
	filter
		.add_globs(&["item", "!allowed"], Some(&origin))
		.unwrap();
	filter
		.add_globs(&["!item", "allowed"], Some(&scoped))
		.unwrap();

	assert!(!path_is_ignored(&filter, &scoped.join("item"), false));
	assert!(path_is_ignored(&filter, &scoped.join("allowed"), false));
	assert!(path_is_ignored(
		&filter,
		&origin.join("tests2").join("item"),
		false
	));
	assert!(!path_is_ignored(
		&filter,
		&origin.join("tests2").join("allowed"),
		false
	));
}

#[tokio::test]
async fn out_of_origin_paths_do_not_match_external_ancestors() {
	let origin = std::fs::canonicalize("tests/tree").unwrap();
	let mut filter = IgnoreFilter::new(&origin, &[]).await.unwrap();
	filter.add_globs(&["rust"], None).unwrap();

	let external = origin
		.parent()
		.unwrap()
		.join("rust")
		.join("watched")
		.join("child");
	let internal = origin.join("rust").join("child");
	assert!(!path_is_ignored(&filter, &external, false));
	assert!(filter.check_dir(&external));
	assert!(path_is_ignored(&filter, &internal, false));
	assert!(!filter.check_dir(&internal));
}
