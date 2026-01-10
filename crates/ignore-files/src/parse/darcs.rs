use chumsky::{prelude::*, text::newline};

use super::common::{any_nonl, ParserErr};

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Line {
	Empty,
	Comment(String),
	Pattern(String),
}

/// Parse a darcs boring line
#[must_use]
pub fn line<'src>() -> impl Parser<'src, &'src str, Line, ParserErr<'src>> {
	let comment = just('#').ignore_then(any_nonl().repeated().collect::<String>());

	let pattern = any_nonl().repeated().collect::<String>();

	comment.map(Line::Comment).or(pattern.map(|content| {
		let trimmed = content.trim();

		if trimmed.is_empty() {
			Line::Empty
		} else {
			Line::Pattern(trimmed.to_string())
		}
	}))
}

/// Parse a complete darcs boring file
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
	fn simple_regex_pattern() {
		assert_eq!(
			line().parse(r"\.o$").into_result(),
			Ok(Line::Pattern(r"\.o$".into()))
		);
	}

	#[test]
	fn complex_regex_pattern() {
		assert_eq!(
			line().parse(r"(^|/)\.git($|/)").into_result(),
			Ok(Line::Pattern(r"(^|/)\.git($|/)".into()))
		);
	}

	#[test]
	fn comment_line() {
		assert_eq!(
			line().parse("# haskell (ghc) interfaces").into_result(),
			Ok(Line::Comment(" haskell (ghc) interfaces".into()))
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
	fn pattern_with_leading_whitespace() {
		assert_eq!(
			line().parse("  \\.tmp$").into_result(),
			Ok(Line::Pattern(r"\.tmp$".into()))
		);
	}

	#[test]
	fn pattern_with_trailing_whitespace() {
		assert_eq!(
			line().parse(r"\.tmp$  ").into_result(),
			Ok(Line::Pattern(r"\.tmp$".into()))
		);
	}

	#[test]
	fn pattern_with_surrounding_whitespace() {
		assert_eq!(
			line().parse(r"  \.tmp$  ").into_result(),
			Ok(Line::Pattern(r"\.tmp$".into()))
		);
	}

	#[test]
	fn comment_with_hash_in_content() {
		assert_eq!(
			line()
				.parse("# this # has # multiple # hashes")
				.into_result(),
			Ok(Line::Comment(" this # has # multiple # hashes".into()))
		);
	}

	#[test]
	fn regex_with_special_chars() {
		assert_eq!(
			line().parse(r"-darcs-backup[[:digit:]]+$").into_result(),
			Ok(Line::Pattern(r"-darcs-backup[[:digit:]]+$".into()))
		);
	}

	#[test]
	fn regex_with_alternation() {
		assert_eq!(
			line().parse(r"\.(fas|fasl|sparcf|x86f)$").into_result(),
			Ok(Line::Pattern(r"\.(fas|fasl|sparcf|x86f)$".into()))
		);
	}

	#[test]
	fn regex_with_brackets() {
		assert_eq!(
			line()
				.parse(r"(^|/)\.waf-[[:digit:].]+-[[:digit:]]+($|/)")
				.into_result(),
			Ok(Line::Pattern(
				r"(^|/)\.waf-[[:digit:].]+-[[:digit:]]+($|/)".into()
			))
		);
	}

	#[test]
	fn file_parsing() {
		let input = "# Boring file regexps:\n\n# compiler intermediate files\n\\.hi$\n\\.o$\n\n# python byte code\n\\.py[co]$";
		let result = file().parse(input).into_result().unwrap();

		assert_eq!(result.len(), 8);
		assert_eq!(result[0], Line::Comment(" Boring file regexps:".into()));
		assert_eq!(result[1], Line::Empty);
		assert_eq!(
			result[2],
			Line::Comment(" compiler intermediate files".into())
		);
		assert_eq!(result[3], Line::Pattern(r"\.hi$".into()));
		assert_eq!(result[4], Line::Pattern(r"\.o$".into()));
		assert_eq!(result[5], Line::Empty);
		assert_eq!(result[6], Line::Comment(" python byte code".into()));
		assert_eq!(result[7], Line::Pattern(r"\.py[co]$".into()));
	}

	#[test]
	fn tabs_and_spaces() {
		assert_eq!(line().parse("\t  \t").into_result(), Ok(Line::Empty));
	}

	#[test]
	fn mixed_whitespace_pattern() {
		assert_eq!(
			line().parse("\t  \\.tmp$  \t").into_result(),
			Ok(Line::Pattern(r"\.tmp$".into()))
		);
	}
}
