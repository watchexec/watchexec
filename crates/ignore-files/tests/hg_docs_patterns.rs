use chumsky::prelude::*;
use ignore_files::parse::hg::glob::*;

#[test]
fn glob_star_dot_c() {
	// glob:*.c - any name ending in ".c" in the current directory
	let result = glob().parse("*.c").into_result().unwrap();
	assert_eq!(
		result,
		Glob(vec![Token::AnyInSegment, Token::Literal(".c".into())])
	);
}

#[test]
fn glob_star_dot_c_implicit() {
	// *.c - any name ending in ".c" in the current directory
	let result = glob().parse("*.c").into_result().unwrap();
	assert_eq!(
		result,
		Glob(vec![Token::AnyInSegment, Token::Literal(".c".into())])
	);
}

#[test]
fn glob_doublestar_dot_c() {
	// **.c - any name ending in ".c" in any subdirectory of the current directory including itself
	let result = glob().parse("**.c").into_result().unwrap();
	assert_eq!(
		result,
		Glob(vec![Token::AnyInPath, Token::Literal(".c".into())])
	);
}

#[test]
fn glob_foo_star() {
	// foo/* - any file in directory foo
	let result = glob().parse("foo/*").into_result().unwrap();
	assert_eq!(
		result,
		Glob(vec![
			Token::Literal("foo".into()),
			Token::Separator,
			Token::AnyInSegment
		])
	);
}

#[test]
fn glob_foo_doublestar() {
	// foo/** - any file in directory foo plus all its subdirectories, recursively
	let result = glob().parse("foo/**").into_result().unwrap();
	assert_eq!(
		result,
		Glob(vec![
			Token::Literal("foo".into()),
			Token::Separator,
			Token::AnyInPath
		])
	);
}

#[test]
fn glob_foo_star_dot_c() {
	// foo/*.c - any name ending in ".c" in the directory foo
	let result = glob().parse("foo/*.c").into_result().unwrap();
	assert_eq!(
		result,
		Glob(vec![
			Token::Literal("foo".into()),
			Token::Separator,
			Token::AnyInSegment,
			Token::Literal(".c".into())
		])
	);
}

#[test]
fn glob_foo_doublestar_dot_c() {
	// foo/**.c - any name ending in ".c" in any subdirectory of foo including itself
	let result = glob().parse("foo/**.c").into_result().unwrap();
	assert_eq!(
		result,
		Glob(vec![
			Token::Literal("foo".into()),
			Token::Separator,
			Token::AnyInPath,
			Token::Literal(".c".into())
		])
	);
}

#[test]
fn glob_rootglob_star_dot_c() {
	// rootglob:*.c - any name ending in ".c" in the root of the repository
	// Note: The parser doesn't handle prefixes like "rootglob:" but the pattern itself is valid
	let result = glob().parse("*.c").into_result().unwrap();
	assert_eq!(
		result,
		Glob(vec![Token::AnyInSegment, Token::Literal(".c".into())])
	);
}

#[test]
fn glob_alternation_extensions() {
	// Test glob alternation with different file extensions
	let result = glob().parse("*.{c,h}").into_result().unwrap();
	assert_eq!(
		result,
		Glob(vec![
			Token::AnyInSegment,
			Token::Literal(".".into()),
			Token::Alt(vec![
				Glob(vec![Token::Literal("c".into())]),
				Glob(vec![Token::Literal("h".into())])
			])
		])
	);
}

#[test]
fn glob_alternation_with_paths() {
	// Test glob alternation with paths
	let result = glob().parse("{src,tests}/**/*.rs").into_result().unwrap();
	assert_eq!(
		result,
		Glob(vec![
			Token::Alt(vec![
				Glob(vec![Token::Literal("src".into())]),
				Glob(vec![Token::Literal("tests".into())])
			]),
			Token::Separator,
			Token::AnyInPath,
			Token::Separator,
			Token::AnyInSegment,
			Token::Literal(".rs".into())
		])
	);
}

#[test]
fn glob_nested_directories() {
	// Test deeply nested directory patterns
	let result = glob().parse("a/b/c/d/**/*.txt").into_result().unwrap();
	assert_eq!(
		result,
		Glob(vec![
			Token::Literal("a".into()),
			Token::Separator,
			Token::Literal("b".into()),
			Token::Separator,
			Token::Literal("c".into()),
			Token::Separator,
			Token::Literal("d".into()),
			Token::Separator,
			Token::AnyInPath,
			Token::Separator,
			Token::AnyInSegment,
			Token::Literal(".txt".into())
		])
	);
}

#[test]
fn glob_question_mark_wildcard() {
	// Test single character wildcard
	let result = glob().parse("file?.txt").into_result().unwrap();
	assert_eq!(
		result,
		Glob(vec![
			Token::Literal("file".into()),
			Token::One,
			Token::Literal(".txt".into())
		])
	);
}

#[test]
fn glob_character_class_basic() {
	// Test basic character class
	let result = glob().parse("file[0-9].txt").into_result().unwrap();
	assert_eq!(
		result,
		Glob(vec![
			Token::Literal("file".into()),
			Token::Class(ignore_files::parse::charclass::Class {
				negated: false,
				classes: vec![ignore_files::parse::charclass::CharClass::Range('0', '9')]
			}),
			Token::Literal(".txt".into())
		])
	);
}

#[test]
fn glob_character_class_negated() {
	// Test negated character class
	let result = glob().parse("file[!0-9].txt").into_result().unwrap();
	assert_eq!(
		result,
		Glob(vec![
			Token::Literal("file".into()),
			Token::Class(ignore_files::parse::charclass::Class {
				negated: true,
				classes: vec![ignore_files::parse::charclass::CharClass::Range('0', '9')]
			}),
			Token::Literal(".txt".into())
		])
	);
}

#[test]
fn glob_character_class_mixed() {
	// Test character class with mixed singles and ranges
	let result = glob().parse("file[a-z0-9_.].txt").into_result().unwrap();
	assert_eq!(
		result,
		Glob(vec![
			Token::Literal("file".into()),
			Token::Class(ignore_files::parse::charclass::Class {
				negated: false,
				classes: vec![
					ignore_files::parse::charclass::CharClass::Range('a', 'z'),
					ignore_files::parse::charclass::CharClass::Range('0', '9'),
					ignore_files::parse::charclass::CharClass::Single('_'),
					ignore_files::parse::charclass::CharClass::Single('.')
				]
			}),
			Token::Literal(".txt".into())
		])
	);
}

#[test]
fn glob_complex_real_world_patterns() {
	// Test some real-world-like patterns
	let patterns = vec![
		"**/*.{js,ts,jsx,tsx}",
		"src/**/*.rs",
		"tests/**/test_*.py",
		"docs/**/*.{md,rst}",
		"*.{yml,yaml}",
		"**/.{git,hg}ignore",
		"target/**",
		"build/**/*.o",
	];

	for pattern in patterns {
		let result = glob().parse(pattern).into_result();
		assert!(result.is_ok(), "Failed to parse pattern: {}", pattern);
	}
}

#[test]
fn glob_empty_alternation() {
	// Test empty alternation
	let result = glob().parse("file{}").into_result().unwrap();
	assert_eq!(
		result,
		Glob(vec![Token::Literal("file".into()), Token::Alt(vec![])])
	);
}

#[test]
fn glob_single_alternation() {
	// Test alternation with single option
	let result = glob().parse("file{txt}").into_result().unwrap();
	assert_eq!(
		result,
		Glob(vec![
			Token::Literal("file".into()),
			Token::Alt(vec![Glob(vec![Token::Literal("txt".into())])])
		])
	);
}

#[test]
fn glob_alternation_with_wildcards() {
	// Test alternation containing wildcards
	let result = glob().parse("{*.c,*.h,src/**/*.rs}").into_result().unwrap();
	assert_eq!(
		result,
		Glob(vec![Token::Alt(vec![
			Glob(vec![Token::AnyInSegment, Token::Literal(".c".into())]),
			Glob(vec![Token::AnyInSegment, Token::Literal(".h".into())]),
			Glob(vec![
				Token::Literal("src".into()),
				Token::Separator,
				Token::AnyInPath,
				Token::Separator,
				Token::AnyInSegment,
				Token::Literal(".rs".into())
			])
		])])
	);
}

#[test]
fn glob_alternation_with_paths_and_separators() {
	// Test alternation with paths containing separators
	let result = glob()
		.parse("{src/main,tests/unit}/**.rs")
		.into_result()
		.unwrap();
	assert_eq!(
		result,
		Glob(vec![
			Token::Alt(vec![
				Glob(vec![
					Token::Literal("src".into()),
					Token::Separator,
					Token::Literal("main".into())
				]),
				Glob(vec![
					Token::Literal("tests".into()),
					Token::Separator,
					Token::Literal("unit".into())
				])
			]),
			Token::Separator,
			Token::AnyInPath,
			Token::Literal(".rs".into())
		])
	);
}

#[test]
fn glob_escaped_special_characters() {
	// Test escaped special characters in various contexts
	let result = glob()
		.parse(r"file\*name\?test\[bracket\]")
		.into_result()
		.unwrap();
	assert_eq!(
		result,
		Glob(vec![Token::Literal("file*name?test[bracket]".into())])
	);
}

#[test]
fn glob_backslash_escaping() {
	// Test backslash escaping
	let result = glob().parse(r"path\\to\\file").into_result().unwrap();
	assert_eq!(result, Glob(vec![Token::Literal(r"path\to\file".into())]));
}

#[test]
fn glob_mixed_escaping_and_wildcards() {
	// Test mix of escaped characters and wildcards
	let result = glob().parse(r"test\*file*.txt").into_result().unwrap();
	assert_eq!(
		result,
		Glob(vec![
			Token::Literal("test*file".into()),
			Token::AnyInSegment,
			Token::Literal(".txt".into())
		])
	);
}

#[test]
fn glob_character_class_with_slash() {
	// Test character class containing forward slash (our fix)
	let result = glob().parse("file[a-z/].txt").into_result().unwrap();
	assert_eq!(
		result,
		Glob(vec![
			Token::Literal("file".into()),
			Token::Class(ignore_files::parse::charclass::Class {
				negated: false,
				classes: vec![
					ignore_files::parse::charclass::CharClass::Range('a', 'z'),
					ignore_files::parse::charclass::CharClass::Single('/')
				]
			}),
			Token::Literal(".txt".into())
		])
	);
}

#[test]
fn glob_character_class_posix_named() {
	// Test POSIX named character classes
	let result = glob().parse("file[[:alnum:]].txt").into_result().unwrap();
	assert_eq!(
		result,
		Glob(vec![
			Token::Literal("file".into()),
			Token::Class(ignore_files::parse::charclass::Class {
				negated: false,
				classes: vec![ignore_files::parse::charclass::CharClass::Named(
					"alnum".into()
				)]
			}),
			Token::Literal(".txt".into())
		])
	);
}

#[test]
fn glob_character_class_equivalence() {
	// Test equivalence classes
	let result = glob().parse("file[[=a=]].txt").into_result().unwrap();
	assert_eq!(
		result,
		Glob(vec![
			Token::Literal("file".into()),
			Token::Class(ignore_files::parse::charclass::Class {
				negated: false,
				classes: vec![ignore_files::parse::charclass::CharClass::Equivalence('a')]
			}),
			Token::Literal(".txt".into())
		])
	);
}

#[test]
fn glob_character_class_collating() {
	// Test collating elements
	let result = glob().parse("file[[.ch.]].txt").into_result().unwrap();
	assert_eq!(
		result,
		Glob(vec![
			Token::Literal("file".into()),
			Token::Class(ignore_files::parse::charclass::Class {
				negated: false,
				classes: vec![ignore_files::parse::charclass::CharClass::Collating(
					"ch".into()
				)]
			}),
			Token::Literal(".txt".into())
		])
	);
}

#[test]
fn glob_very_complex_pattern() {
	// Test a very complex pattern combining multiple features
	let result = glob()
		.parse("**/{src,tests}/**/[!.]*.{rs,toml}")
		.into_result()
		.unwrap();
	assert_eq!(
		result,
		Glob(vec![
			Token::AnyInPath,
			Token::Separator,
			Token::Alt(vec![
				Glob(vec![Token::Literal("src".into())]),
				Glob(vec![Token::Literal("tests".into())])
			]),
			Token::Separator,
			Token::AnyInPath,
			Token::Separator,
			Token::Class(ignore_files::parse::charclass::Class {
				negated: true,
				classes: vec![ignore_files::parse::charclass::CharClass::Single('.')]
			}),
			Token::AnyInSegment,
			Token::Literal(".".into()),
			Token::Alt(vec![
				Glob(vec![Token::Literal("rs".into())]),
				Glob(vec![Token::Literal("toml".into())])
			])
		])
	);
}
