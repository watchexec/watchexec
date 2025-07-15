use chumsky::{prelude::*, text::newline};

use super::{
	charclass::{CharClass, Class},
	common::{any_nonl, none_of_nonl, ParserDebugExt as _, ParserErr},
};

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Line {
	Empty,
	Comment(String),
	Patterns(Vec<Pattern>),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Pattern {
	pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Segment {
	Fixed(String),
	Wildcard(Vec<WildcardToken>),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum WildcardToken {
	Any,          // *
	One,          // ?
	Class(Class), // [abc] or [^abc]
	Literal(String),
}

/// Parse a quoted pattern (single or double quotes)
fn quoted_pattern<'src>() -> impl Parser<'src, &'src str, String, ParserErr<'src>> {
	let single_quoted = just('\'')
		.ignore_then(none_of("'").repeated().collect::<String>())
		.then_ignore(just('\''));

	let double_quoted = just('"')
		.ignore_then(none_of("\"").repeated().collect::<String>())
		.then_ignore(just('"'));

	single_quoted.or(double_quoted)
}

/// Parse an unquoted pattern (until whitespace or comma)
fn unquoted_pattern<'src>() -> impl Parser<'src, &'src str, String, ParserErr<'src>> {
	none_of(" \t\n\r,\"'")
		.repeated()
		.at_least(1)
		.collect::<String>()
}

/// Parse a fossil-specific character class [abc] or [^abc]
fn fossil_charclass<'src>() -> impl Parser<'src, &'src str, Class, ParserErr<'src>> {
	let single = none_of_nonl("]").map(CharClass::Single);
	let range = none_of_nonl("]")
		.then_ignore(just('-'))
		.then(none_of_nonl("]"))
		.map(|(a, b)| CharClass::Range(a, b));

	let class_item = choice((range, single));

	let negated_class = just("[^")
		.ignore_then(class_item.clone().repeated().at_least(1).collect())
		.then_ignore(just(']'))
		.map(|class_items| Class {
			negated: true,
			classes: class_items,
		});

	let positive_class = just('[')
		.ignore_then(class_item.repeated().at_least(1).collect())
		.then_ignore(just(']'))
		.map(|class_items| Class {
			negated: false,
			classes: class_items,
		});

	choice((negated_class, positive_class))
}

/// Parse wildcard tokens for fossil patterns
pub fn wildcard<'src>() -> impl Parser<'src, &'src str, Vec<WildcardToken>, ParserErr<'src>> {
	use WildcardToken::*;

	let literal = none_of_nonl("*?[")
		.repeated()
		.at_least(1)
		.collect::<String>()
		.map(Literal)
		.debug("literal");

	choice((
		just('*').to(Any),
		just('?').to(One),
		fossil_charclass().map(Class),
		literal,
	))
	.repeated()
	.collect::<Vec<_>>()
	.map(|toks| {
		toks.into_iter().fold(Vec::new(), |mut acc, tok| {
			match (tok, acc.last_mut()) {
				(Literal(tok), Some(&mut Literal(ref mut last))) => {
					last.push_str(&tok);
				}
				(tok, _) => acc.push(tok),
			}
			acc
		})
	})
}

/// Parse a single pattern string into segments
fn parse_pattern_segments<'src>() -> impl Parser<'src, &'src str, Segment, ParserErr<'src>> {
	wildcard().map(|tokens| {
		if tokens.is_empty() {
			Segment::Fixed(String::new())
		} else if tokens
			.iter()
			.all(|t| matches!(t, WildcardToken::Literal(_)))
		{
			Segment::Fixed(
				tokens
					.into_iter()
					.map(|t| match t {
						WildcardToken::Literal(s) => s,
						_ => unreachable!(),
					})
					.collect(),
			)
		} else {
			Segment::Wildcard(tokens)
		}
	})
}

/// Parse multiple patterns separated by whitespace or commas
fn pattern_list<'src>() -> impl Parser<'src, &'src str, Vec<Pattern>, ParserErr<'src>> {
	let separator = choice((
		just(' ').repeated().at_least(1).to(()),
		just('\t').repeated().at_least(1).to(()),
		just(',').then(just(' ').or(just('\t')).repeated()).to(()),
	));

	let pattern_text = quoted_pattern().or(unquoted_pattern());

	pattern_text
		.map(|text| {
			let segment = parse_pattern_segments()
				.parse(&text)
				.into_result()
				.unwrap_or_else(|_| Segment::Fixed(text.clone()));
			Pattern {
				segments: vec![segment],
			}
		})
		.separated_by(separator)
		.collect()
}

/// Parse a fossil ignore line
#[must_use]
pub fn line<'src>() -> impl Parser<'src, &'src str, Line, ParserErr<'src>> {
	let comment = just('#').ignore_then(any_nonl().repeated().collect::<String>());

	let content = any_nonl().repeated().collect::<String>();

	comment.map(Line::Comment).or(content.map(|text| {
		let trimmed = text.trim();
		if trimmed.is_empty() {
			Line::Empty
		} else {
			// Parse the line as a pattern list
			match pattern_list().parse(trimmed).into_result() {
				Ok(patterns) if !patterns.is_empty() => Line::Patterns(patterns),
				_ => Line::Empty,
			}
		}
	}))
}

/// Parse a complete fossil ignore file
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
	fn simple_glob_pattern() {
		assert_eq!(
			line().parse("*.txt").into_result(),
			Ok(Line::Patterns(vec![Pattern {
				segments: vec![Segment::Wildcard(vec![
					WildcardToken::Any,
					WildcardToken::Literal(".txt".into())
				])]
			}]))
		);
	}

	#[test]
	fn multiple_patterns_space_separated() {
		assert_eq!(
			line().parse("*.txt *.log").into_result(),
			Ok(Line::Patterns(vec![
				Pattern {
					segments: vec![Segment::Wildcard(vec![
						WildcardToken::Any,
						WildcardToken::Literal(".txt".into())
					])]
				},
				Pattern {
					segments: vec![Segment::Wildcard(vec![
						WildcardToken::Any,
						WildcardToken::Literal(".log".into())
					])]
				}
			]))
		);
	}

	#[test]
	fn multiple_patterns_comma_separated() {
		assert_eq!(
			line().parse("*.txt,*.log").into_result(),
			Ok(Line::Patterns(vec![
				Pattern {
					segments: vec![Segment::Wildcard(vec![
						WildcardToken::Any,
						WildcardToken::Literal(".txt".into())
					])]
				},
				Pattern {
					segments: vec![Segment::Wildcard(vec![
						WildcardToken::Any,
						WildcardToken::Literal(".log".into())
					])]
				}
			]))
		);
	}

	#[test]
	fn quoted_pattern_with_spaces() {
		assert_eq!(
			line().parse("\"foo bar.txt\"").into_result(),
			Ok(Line::Patterns(vec![Pattern {
				segments: vec![Segment::Fixed("foo bar.txt".into())]
			}]))
		);
	}

	#[test]
	fn single_quoted_pattern() {
		assert_eq!(
			line().parse("'foo bar.txt'").into_result(),
			Ok(Line::Patterns(vec![Pattern {
				segments: vec![Segment::Fixed("foo bar.txt".into())]
			}]))
		);
	}

	#[test]
	fn question_mark_wildcard() {
		assert_eq!(
			line().parse("test?.log").into_result(),
			Ok(Line::Patterns(vec![Pattern {
				segments: vec![Segment::Wildcard(vec![
					WildcardToken::Literal("test".into()),
					WildcardToken::One,
					WildcardToken::Literal(".log".into())
				])]
			}]))
		);
	}

	#[test]
	fn character_class_pattern() {
		assert_eq!(
			line().parse("*.[ch]").into_result(),
			Ok(Line::Patterns(vec![Pattern {
				segments: vec![Segment::Wildcard(vec![
					WildcardToken::Any,
					WildcardToken::Literal(".".into()),
					WildcardToken::Class(Class {
						negated: false,
						classes: vec![CharClass::Single('c'), CharClass::Single('h')]
					})
				])]
			}]))
		);
	}

	#[test]
	fn negated_character_class() {
		assert_eq!(
			line().parse("*.[^ch]").into_result(),
			Ok(Line::Patterns(vec![Pattern {
				segments: vec![Segment::Wildcard(vec![
					WildcardToken::Any,
					WildcardToken::Literal(".".into()),
					WildcardToken::Class(Class {
						negated: true,
						classes: vec![CharClass::Single('c'), CharClass::Single('h')]
					})
				])]
			}]))
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
	fn empty_line() {
		assert_eq!(line().parse("").into_result(), Ok(Line::Empty));
	}

	#[test]
	fn whitespace_only() {
		assert_eq!(line().parse("   ").into_result(), Ok(Line::Empty));
	}

	#[test]
	fn mixed_separators() {
		assert_eq!(
			line().parse("*.txt, *.log\t*.bak").into_result(),
			Ok(Line::Patterns(vec![
				Pattern {
					segments: vec![Segment::Wildcard(vec![
						WildcardToken::Any,
						WildcardToken::Literal(".txt".into())
					])]
				},
				Pattern {
					segments: vec![Segment::Wildcard(vec![
						WildcardToken::Any,
						WildcardToken::Literal(".log".into())
					])]
				},
				Pattern {
					segments: vec![Segment::Wildcard(vec![
						WildcardToken::Any,
						WildcardToken::Literal(".bak".into())
					])]
				}
			]))
		);
	}

	#[test]
	fn complex_pattern() {
		assert_eq!(
			line().parse("src/*.[ch]").into_result(),
			Ok(Line::Patterns(vec![Pattern {
				segments: vec![Segment::Wildcard(vec![
					WildcardToken::Literal("src/".into()),
					WildcardToken::Any,
					WildcardToken::Literal(".".into()),
					WildcardToken::Class(Class {
						negated: false,
						classes: vec![CharClass::Single('c'), CharClass::Single('h')]
					})
				])]
			}]))
		);
	}

	#[test]
	fn file_parsing() {
		let input =
			"# Fossil ignore patterns\n*.o\n*.tmp, *.bak\n\n\"foo bar.txt\"\n# Another comment";
		let result = file().parse(input).into_result().unwrap();

		assert_eq!(result.len(), 6);
		assert_eq!(result[0], Line::Comment(" Fossil ignore patterns".into()));
		assert_eq!(
			result[1],
			Line::Patterns(vec![Pattern {
				segments: vec![Segment::Wildcard(vec![
					WildcardToken::Any,
					WildcardToken::Literal(".o".into())
				])]
			}])
		);
		assert_eq!(
			result[2],
			Line::Patterns(vec![
				Pattern {
					segments: vec![Segment::Wildcard(vec![
						WildcardToken::Any,
						WildcardToken::Literal(".tmp".into())
					])]
				},
				Pattern {
					segments: vec![Segment::Wildcard(vec![
						WildcardToken::Any,
						WildcardToken::Literal(".bak".into())
					])]
				}
			])
		);
		assert_eq!(result[3], Line::Empty);
		assert_eq!(
			result[4],
			Line::Patterns(vec![Pattern {
				segments: vec![Segment::Fixed("foo bar.txt".into())]
			}])
		);
		assert_eq!(result[5], Line::Comment(" Another comment".into()));
	}

	#[test]
	fn fixed_pattern() {
		assert_eq!(
			line().parse("README.txt").into_result(),
			Ok(Line::Patterns(vec![Pattern {
				segments: vec![Segment::Fixed("README.txt".into())]
			}]))
		);
	}

	#[test]
	fn tabs_and_spaces() {
		assert_eq!(line().parse("\t  \t").into_result(), Ok(Line::Empty));
	}

	#[test]
	fn character_range() {
		assert_eq!(
			line().parse("*[a-z].txt").into_result(),
			Ok(Line::Patterns(vec![Pattern {
				segments: vec![Segment::Wildcard(vec![
					WildcardToken::Any,
					WildcardToken::Class(Class {
						negated: false,
						classes: vec![CharClass::Range('a', 'z')]
					}),
					WildcardToken::Literal(".txt".into())
				])]
			}]))
		);
	}
}
