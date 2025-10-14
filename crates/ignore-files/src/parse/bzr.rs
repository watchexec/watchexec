use chumsky::{prelude::*, text::newline};

use super::common::{any_nonl, ParserErr};

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Line {
	Empty,
	Comment(String),
	Pattern {
		negated: bool,
		kind: PatternKind,
		pattern: String,
	},
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum PatternKind {
	Glob,
	Regex { case_insensitive: bool },
}

/// Parse a regex pattern with optional case-insensitive flag
#[must_use]
pub fn regex_pattern<'src>() -> impl Parser<'src, &'src str, (String, bool), ParserErr<'src>> {
	just("RE:")
		.ignore_then(
			just("(?i)")
				.or_not()
				.map(|flag| flag.is_some())
				.then(any_nonl().repeated().collect::<String>()),
		)
		.map(|(case_insensitive, pattern)| (pattern, case_insensitive))
}

/// Parse a bazaar ignore line
#[must_use]
pub fn line<'src>() -> impl Parser<'src, &'src str, Line, ParserErr<'src>> {
	let comment = just('#').ignore_then(any_nonl().repeated().collect::<String>());

	let pattern_content = any_nonl().repeated().collect::<String>();

	comment
		.map(Line::Comment)
		.or(pattern_content.map(|content| {
			let trimmed = content.trim();

			if trimmed.is_empty() {
				return Line::Empty;
			}

			// Check for negation after trimming
			let (negated, pattern_part) = trimmed
				.strip_prefix('!')
				.map_or((false, trimmed), |rest| (true, rest));

			// Check if it's a regex pattern
			if let Some(regex_content) = pattern_part.strip_prefix("RE:") {
				let (case_insensitive, pattern) = regex_content.strip_prefix("(?i)").map_or_else(
					|| (false, regex_content.to_string()),
					|case_insensitive_content| (true, case_insensitive_content.to_string()),
				);

				return Line::Pattern {
					negated,
					kind: PatternKind::Regex { case_insensitive },
					pattern,
				};
			}

			// Handle escaped characters at the start
			let mut pattern = pattern_part.to_string();
			if pattern.starts_with(r"\!") {
				pattern = pattern[1..].to_string();
			}

			// Remove trailing slashes (as per bazaar documentation)
			while pattern.ends_with('/') {
				pattern.pop();
			}

			Line::Pattern {
				negated,
				kind: PatternKind::Glob,
				pattern,
			}
		}))
}

/// Parse a complete bazaar ignore file
#[must_use]
pub fn file<'src>() -> impl Parser<'src, &'src str, Vec<Line>, ParserErr<'src>> {
	line()
		.separated_by(newline())
		.allow_trailing()
		.collect::<Vec<_>>()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn simple_glob() {
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
	fn negated_pattern() {
		assert_eq!(
			line().parse("!important.tmp").into_result(),
			Ok(Line::Pattern {
				negated: true,
				kind: PatternKind::Glob,
				pattern: "important.tmp".into(),
			})
		);
	}

	#[test]
	fn regex_pattern() {
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
	fn negated_regex() {
		assert_eq!(
			line().parse("!RE:(?i)foo").into_result(),
			Ok(Line::Pattern {
				negated: true,
				kind: PatternKind::Regex {
					case_insensitive: true
				},
				pattern: "foo".into(),
			})
		);
	}

	#[test]
	fn comment() {
		assert_eq!(
			line().parse("# this is a comment").into_result(),
			Ok(Line::Comment(" this is a comment".into()))
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
	fn whitespace_only() {
		assert_eq!(line().parse("   ").into_result(), Ok(Line::Empty));
	}

	#[test]
	fn escaped_exclamation() {
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
	fn trailing_slash_removed() {
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
	fn multiple_trailing_slashes() {
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
	fn root_directory_pattern() {
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
	fn complex_glob() {
		assert_eq!(
			line().parse("src/**/*.rs").into_result(),
			Ok(Line::Pattern {
				negated: false,
				kind: PatternKind::Glob,
				pattern: "src/**/*.rs".into(),
			})
		);
	}

	#[test]
	fn character_class() {
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
	fn file_parsing() {
		let input = "# Build files\n*.o\n*.so\n!important.o\n\nRE:(?i)temp.*";
		let result = file().parse(input).into_result().unwrap();

		assert_eq!(result.len(), 6);
		assert_eq!(result[0], Line::Comment(" Build files".into()));
		assert_eq!(
			result[1],
			Line::Pattern {
				negated: false,
				kind: PatternKind::Glob,
				pattern: "*.o".into(),
			}
		);
		assert_eq!(
			result[2],
			Line::Pattern {
				negated: false,
				kind: PatternKind::Glob,
				pattern: "*.so".into(),
			}
		);
		assert_eq!(
			result[3],
			Line::Pattern {
				negated: true,
				kind: PatternKind::Glob,
				pattern: "important.o".into(),
			}
		);
		assert_eq!(result[4], Line::Empty);
		assert_eq!(
			result[5],
			Line::Pattern {
				negated: false,
				kind: PatternKind::Regex {
					case_insensitive: true
				},
				pattern: "temp.*".into(),
			}
		);
	}
}
