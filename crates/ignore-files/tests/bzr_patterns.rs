use chumsky::prelude::*;
use ignore_files::parse::bzr::*;

#[test]
fn simple_glob_pattern() {
	assert_eq!(
		line().parse("*.tmp").into_result(),
		Ok(Line::Pattern {
			negated: false,
			kind: PatternKind::Glob,
			pattern: "*.tmp".into(),
		})
	);
}

#[test]
fn question_mark_wildcard() {
	assert_eq!(
		line().parse("test?.log").into_result(),
		Ok(Line::Pattern {
			negated: false,
			kind: PatternKind::Glob,
			pattern: "test?.log".into(),
		})
	);
}

#[test]
fn character_class_pattern() {
	assert_eq!(
		line().parse("*.[ch]").into_result(),
		Ok(Line::Pattern {
			negated: false,
			kind: PatternKind::Glob,
			pattern: "*.[ch]".into(),
		})
	);
}

#[test]
fn character_class_range() {
	assert_eq!(
		line().parse("test[0-9].log").into_result(),
		Ok(Line::Pattern {
			negated: false,
			kind: PatternKind::Glob,
			pattern: "test[0-9].log".into(),
		})
	);
}

#[test]
fn recursive_directory_pattern() {
	assert_eq!(
		line().parse("src/**/build").into_result(),
		Ok(Line::Pattern {
			negated: false,
			kind: PatternKind::Glob,
			pattern: "src/**/build".into(),
		})
	);
}

#[test]
fn root_directory_with_dot_slash() {
	assert_eq!(
		line().parse("./temp").into_result(),
		Ok(Line::Pattern {
			negated: false,
			kind: PatternKind::Glob,
			pattern: "./temp".into(),
		})
	);
}

#[test]
fn negated_pattern() {
	assert_eq!(
		line().parse("!important.log").into_result(),
		Ok(Line::Pattern {
			negated: true,
			kind: PatternKind::Glob,
			pattern: "important.log".into(),
		})
	);
}

#[test]
fn negated_glob_pattern() {
	assert_eq!(
		line().parse("!*.keep").into_result(),
		Ok(Line::Pattern {
			negated: true,
			kind: PatternKind::Glob,
			pattern: "*.keep".into(),
		})
	);
}

#[test]
fn escaped_exclamation_mark() {
	assert_eq!(
		line().parse(r"\!important").into_result(),
		Ok(Line::Pattern {
			negated: false,
			kind: PatternKind::Glob,
			pattern: "!important".into(),
		})
	);
}

#[test]
fn basic_regex_pattern() {
	assert_eq!(
		line().parse("RE:.*\\.tmp$").into_result(),
		Ok(Line::Pattern {
			negated: false,
			kind: PatternKind::Regex {
				case_insensitive: false
			},
			pattern: ".*\\.tmp$".into(),
		})
	);
}

#[test]
fn case_insensitive_regex() {
	assert_eq!(
		line().parse("RE:(?i)foo").into_result(),
		Ok(Line::Pattern {
			negated: false,
			kind: PatternKind::Regex {
				case_insensitive: true
			},
			pattern: "foo".into(),
		})
	);
}

#[test]
fn case_insensitive_regex_complex() {
	assert_eq!(
		line().parse("RE:(?i).*\\.(jpg|png|gif)$").into_result(),
		Ok(Line::Pattern {
			negated: false,
			kind: PatternKind::Regex {
				case_insensitive: true
			},
			pattern: ".*\\.(jpg|png|gif)$".into(),
		})
	);
}

#[test]
fn negated_regex_pattern() {
	assert_eq!(
		line().parse("!RE:temp.*").into_result(),
		Ok(Line::Pattern {
			negated: true,
			kind: PatternKind::Regex {
				case_insensitive: false
			},
			pattern: "temp.*".into(),
		})
	);
}

#[test]
fn negated_case_insensitive_regex() {
	assert_eq!(
		line().parse("!RE:(?i)temp.*").into_result(),
		Ok(Line::Pattern {
			negated: true,
			kind: PatternKind::Regex {
				case_insensitive: true
			},
			pattern: "temp.*".into(),
		})
	);
}

#[test]
fn trailing_slash_ignored() {
	assert_eq!(
		line().parse("build/").into_result(),
		Ok(Line::Pattern {
			negated: false,
			kind: PatternKind::Glob,
			pattern: "build".into(),
		})
	);
}

#[test]
fn multiple_trailing_slashes_ignored() {
	assert_eq!(
		line().parse("build///").into_result(),
		Ok(Line::Pattern {
			negated: false,
			kind: PatternKind::Glob,
			pattern: "build".into(),
		})
	);
}

#[test]
fn path_with_slashes() {
	assert_eq!(
		line().parse("src/build/temp").into_result(),
		Ok(Line::Pattern {
			negated: false,
			kind: PatternKind::Glob,
			pattern: "src/build/temp".into(),
		})
	);
}

#[test]
fn regex_with_slashes() {
	assert_eq!(
		line().parse("RE:src/.*\\.o$").into_result(),
		Ok(Line::Pattern {
			negated: false,
			kind: PatternKind::Regex {
				case_insensitive: false
			},
			pattern: "src/.*\\.o$".into(),
		})
	);
}

#[test]
fn comment_line() {
	assert_eq!(
		line().parse("# This is a comment").into_result(),
		Ok(Line::Comment(" This is a comment".into()))
	);
}

#[test]
fn comment_with_pattern_like_text() {
	assert_eq!(
		line().parse("# *.tmp files are ignored").into_result(),
		Ok(Line::Comment(" *.tmp files are ignored".into()))
	);
}

#[test]
fn empty_comment() {
	assert_eq!(
		line().parse("#").into_result(),
		Ok(Line::Comment("".into()))
	);
}

#[test]
fn empty_line() {
	assert_eq!(line().parse("").into_result(), Ok(Line::Empty));
}

#[test]
fn whitespace_only_line() {
	assert_eq!(line().parse("   ").into_result(), Ok(Line::Empty));
}

#[test]
fn tabs_and_spaces() {
	assert_eq!(line().parse("\t  \t").into_result(), Ok(Line::Empty));
}

#[test]
fn pattern_with_leading_whitespace() {
	assert_eq!(
		line().parse("  *.tmp").into_result(),
		Ok(Line::Pattern {
			negated: false,
			kind: PatternKind::Glob,
			pattern: "*.tmp".into(),
		})
	);
}

#[test]
fn pattern_with_trailing_whitespace() {
	assert_eq!(
		line().parse("*.tmp  ").into_result(),
		Ok(Line::Pattern {
			negated: false,
			kind: PatternKind::Glob,
			pattern: "*.tmp".into(),
		})
	);
}

#[test]
fn negated_pattern_with_whitespace() {
	assert_eq!(
		line().parse("  !important.log  ").into_result(),
		Ok(Line::Pattern {
			negated: true,
			kind: PatternKind::Glob,
			pattern: "important.log".into(),
		})
	);
}

#[test]
fn regex_pattern_with_whitespace() {
	assert_eq!(
		line().parse("  RE:.*\\.tmp$  ").into_result(),
		Ok(Line::Pattern {
			negated: false,
			kind: PatternKind::Regex {
				case_insensitive: false
			},
			pattern: ".*\\.tmp$".into(),
		})
	);
}

#[test]
fn complex_glob_with_multiple_wildcards() {
	assert_eq!(
		line().parse("src/**/*.[ch]pp").into_result(),
		Ok(Line::Pattern {
			negated: false,
			kind: PatternKind::Glob,
			pattern: "src/**/*.[ch]pp".into(),
		})
	);
}

#[test]
fn pattern_starting_with_dot() {
	assert_eq!(
		line().parse(".hidden").into_result(),
		Ok(Line::Pattern {
			negated: false,
			kind: PatternKind::Glob,
			pattern: ".hidden".into(),
		})
	);
}

#[test]
fn pattern_with_question_marks() {
	assert_eq!(
		line().parse("test???.log").into_result(),
		Ok(Line::Pattern {
			negated: false,
			kind: PatternKind::Glob,
			pattern: "test???.log".into(),
		})
	);
}

#[test]
fn regex_with_groups() {
	assert_eq!(
		line().parse("RE:(test|spec)_.*\\.rb$").into_result(),
		Ok(Line::Pattern {
			negated: false,
			kind: PatternKind::Regex {
				case_insensitive: false
			},
			pattern: "(test|spec)_.*\\.rb$".into(),
		})
	);
}

#[test]
fn complete_file_parsing() {
	let input = r#"# Bazaar ignore file
# Build artifacts
*.o
*.so
*.a

# Temporary files
*.tmp
*.bak
!important.bak

# Directories
build/
dist/

# Case insensitive regex for images
RE:(?i).*\.(jpg|png|gif)$

# Negated regex
!RE:keep_.*\.tmp$

# Root directory only
./config

# Complex glob patterns
src/**/*.class
test??.log
*.[ch]pp
"#;

	let result = file().parse(input).into_result().unwrap();

	assert_eq!(result.len(), 29);

	// Check a few key patterns
	assert_eq!(result[0], Line::Comment(" Bazaar ignore file".into()));
	assert_eq!(result[1], Line::Comment(" Build artifacts".into()));
	assert_eq!(
		result[2],
		Line::Pattern {
			negated: false,
			kind: PatternKind::Glob,
			pattern: "*.o".into(),
		}
	);
	assert_eq!(
		result[9],
		Line::Pattern {
			negated: true,
			kind: PatternKind::Glob,
			pattern: "important.bak".into(),
		}
	);
	assert_eq!(
		result[16],
		Line::Pattern {
			negated: false,
			kind: PatternKind::Regex {
				case_insensitive: true
			},
			pattern: ".*\\.(jpg|png|gif)$".into(),
		}
	);
	assert_eq!(
		result[22],
		Line::Pattern {
			negated: false,
			kind: PatternKind::Glob,
			pattern: "./config".into(),
		}
	);
}

#[test]
fn edge_case_regex_prefix_in_glob() {
	// This should be treated as a glob pattern, not regex
	assert_eq!(
		line().parse("RE_test.log").into_result(),
		Ok(Line::Pattern {
			negated: false,
			kind: PatternKind::Glob,
			pattern: "RE_test.log".into(),
		})
	);
}

#[test]
fn regex_pattern_empty() {
	assert_eq!(
		line().parse("RE:").into_result(),
		Ok(Line::Pattern {
			negated: false,
			kind: PatternKind::Regex {
				case_insensitive: false
			},
			pattern: "".into(),
		})
	);
}

#[test]
fn case_insensitive_regex_empty() {
	assert_eq!(
		line().parse("RE:(?i)").into_result(),
		Ok(Line::Pattern {
			negated: false,
			kind: PatternKind::Regex {
				case_insensitive: true
			},
			pattern: "".into(),
		})
	);
}
