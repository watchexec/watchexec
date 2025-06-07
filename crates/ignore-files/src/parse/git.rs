use std::iter::once;

use chumsky::prelude::*;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Line {
	Comment(String),
	Pattern {
		negated: bool,
		segments: Vec<Segment>,
	},
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Segment {
	Terminal,
	Fixed(String),
	Wildcard(String),
	All,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum WildcardToken {
	Any, // *
	One, // ?
	Class {
		// [afg] and [!afg]
		negated: bool,
		classes: Vec<CharClass>,
	},
	Literal(String),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum CharClass {
	Single(char),      // e
	Range(char, char), // A-Z
	Named(String),     // [:alnum:]
	Collating(String), // [.ch.]
	Equivalence(char), // [=a=]
}

fn wildcard<'src>() -> impl Parser<'src, &'src str, Vec<WildcardToken>> {
	use WildcardToken::*;

	let class = just(']')
		.or_not()
		.then(none_of(']').repeated().collect::<Vec<char>>())
		.map(|(initial, mut rest)| Class {
			negated: false,
			classes: {
				if let Some(c) = initial {
					rest.insert(0, c);
				}
				rest.into_iter().map(CharClass::Single).collect()
			},
		});

	choice((
		just('*').to(Any),
		just('?').to(One),
		just('\\').ignore_then(choice((
			just('\\').to(Literal(r"\".into())),
			just('?').to(Literal(r"?".into())),
			just('[').to(Literal(r"[".into())),
			just('*').to(Literal(r"*".into())),
		))),
		just('[').ignore_then(class.then_ignore(just(']'))),
	))
	.or(any()
		.repeated()
		.at_least(1)
		.collect::<String>()
		.map(Literal))
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

fn line<'src>() -> impl Parser<'src, &'src str, Line> {
	let comment = just('#').ignore_then(any().repeated().collect::<String>());

	let negator = just('!').or_not().map(|exists| exists.is_some());

	let segments = none_of('/')
		.repeated()
		.collect::<String>()
		.map(|seg| {
			if seg.is_empty() {
				Segment::Terminal
			} else if seg == "**" {
				Segment::All
			} else if seg.contains(['*', '?', '[']) {
				Segment::Wildcard(seg)
			} else {
				Segment::Fixed(seg)
			}
		})
		.separated_by(just('/'))
		.collect::<Vec<_>>();

	comment
		.map(|content| Line::Comment(content))
		.or(negator.then(segments).map(|(negated, mut segments)| {
			if let Some(Segment::Wildcard(ref mut last) | Segment::Fixed(ref mut last)) =
				segments.last_mut()
			{
				let final_length = {
					let without_trailing_whitespace = last.trim_end();
					if without_trailing_whitespace.ends_with('\\') {
						without_trailing_whitespace.len() + 1
					} else {
						without_trailing_whitespace.len()
					}
				};
				let _ = last.split_off(final_length);
			}

			Line::Pattern { negated, segments }
		}))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn wildcard_exercise() {
		use CharClass::*;
		use WildcardToken::*;
		assert_eq!(
			wildcard()
				.parse(
					r"lit[A-Z][!a-z]*?[*!?][0a-cf][[=a=]x[:alnum:]y[.ch.]][[?*\][!]a-][][!][]-][--0]\\\*\?lit"
				)
				.into_result(),
			Ok(vec![
				Literal("lit".into()),
				Class {
					negated: false,
					classes: vec![Range('A', 'Z')],
				},
				Class {
					negated: true,
					classes: vec![Range('a', 'z')],
				},
				Any,
				One,
				Class {
					negated: false,
					classes: vec![Single('*'), Single('!'), Single('?'),],
				},
				Class {
					negated: false,
					classes: vec![Single('0'), Range('a', 'c'), Single('f'),],
				},
				Class {
					negated: false,
					classes: vec![
						Equivalence('a'),
						Single('x'),
						Named("alnum".into()),
						Single('y'),
						Collating("ch".into()),
					],
				},
				Class {
					negated: false,
					classes: vec![Single('['), Single('?'), Single('*'), Single('\\'),],
				},
				Class {
					negated: true,
					classes: vec![Single(']'), Single('a'), Single('-'),],
				},
				Class {
					negated: false,
					classes: vec![Single(']'), Single('['), Single('!'),],
				},
				Class {
					negated: false,
					classes: vec![Single(']'), Single('-'),],
				},
				Class {
					negated: false,
					classes: vec![Range('-', '0')],
				},
				Literal(r"\*?lit".into()),
			])
		);
	}

	#[test]
	fn wildcard_empty() {
		assert_eq!(wildcard().parse(r"").into_result(), Ok(vec![]));
	}

	#[test]
	fn wildcard_any() {
		use WildcardToken::*;
		assert_eq!(wildcard().parse(r"*").into_result(), Ok(vec![Any]));
	}

	#[test]
	fn wildcard_one() {
		use WildcardToken::*;
		assert_eq!(wildcard().parse(r"?").into_result(), Ok(vec![One]));
	}

	#[test]
	fn wildcard_literal() {
		use WildcardToken::*;
		assert_eq!(
			wildcard().parse(r"lit").into_result(),
			Ok(vec![Literal("lit".into())])
		);
	}

	#[test]
	fn wildcard_class_range() {
		use CharClass::*;
		use WildcardToken::*;
		assert_eq!(
			wildcard().parse(r"[A-Z]").into_result(),
			Ok(vec![Class {
				negated: false,
				classes: vec![Range('A', 'Z')],
			}])
		);
	}

	#[test]
	fn wildcard_class_negated_range() {
		use CharClass::*;
		use WildcardToken::*;
		assert_eq!(
			wildcard().parse(r"[!a-z]").into_result(),
			Ok(vec![Class {
				negated: true,
				classes: vec![Range('a', 'z')],
			}])
		);
	}

	#[test]
	fn wildcard_class_special_chars() {
		use CharClass::*;
		use WildcardToken::*;
		assert_eq!(
			wildcard().parse(r"[*!?]").into_result(),
			Ok(vec![Class {
				negated: false,
				classes: vec![Single('*'), Single('!'), Single('?'),],
			}])
		);
	}

	#[test]
	fn wildcard_class_mixed() {
		use CharClass::*;
		use WildcardToken::*;
		assert_eq!(
			wildcard().parse(r"[0a-cf]").into_result(),
			Ok(vec![Class {
				negated: false,
				classes: vec![Single('0'), Range('a', 'c'), Single('f'),],
			}])
		);
	}

	#[test]
	fn wildcard_class_unicode() {
		use CharClass::*;
		use WildcardToken::*;
		assert_eq!(
			wildcard().parse(r"[[=a=]x[:alnum:]y[.ch.]]").into_result(),
			Ok(vec![Class {
				negated: false,
				classes: vec![
					Equivalence('a'),
					Single('x'),
					Named("alnum".into()),
					Single('y'),
					Collating("ch".into()),
				],
			}])
		);
	}

	#[test]
	fn wildcard_class_opening_inner_open_bracket() {
		use CharClass::*;
		use WildcardToken::*;
		assert_eq!(
			wildcard().parse(r"[[?*\]").into_result(),
			Ok(vec![Class {
				negated: false,
				classes: vec![Single('['), Single('?'), Single('*'), Single('\\'),],
			}])
		);
	}

	#[test]
	fn wildcard_class_negated_inner_close_bracket() {
		use CharClass::*;
		use WildcardToken::*;
		assert_eq!(
			wildcard().parse(r"[!]a-]").into_result(),
			Ok(vec![Class {
				negated: true,
				classes: vec![Single(']'), Single('a'), Single('-'),],
			}])
		);
	}

	#[test]
	fn wildcard_classes_inner_close_bracket() {
		use CharClass::*;
		use WildcardToken::*;
		assert_eq!(
			wildcard().parse(r"[][!][]-]").into_result(),
			Ok(vec![
				Class {
					negated: false,
					classes: vec![Single(']'), Single('['), Single('!'),],
				},
				Class {
					negated: false,
					classes: vec![Single(']'), Single('-'),],
				}
			])
		);
	}

	#[test]
	fn wildcard_class_hyphen_start_range() {
		use CharClass::*;
		use WildcardToken::*;
		assert_eq!(
			wildcard().parse(r"[--0]").into_result(),
			Ok(vec![Class {
				negated: false,
				classes: vec![Range('-', '0')],
			}])
		);
	}

	#[test]
	fn wildcard_escaped_literals() {
		use WildcardToken::*;
		assert_eq!(
			wildcard().parse(r"\\\*\?lit").into_result(),
			Ok(vec![Literal(r"\*?lit".into())])
		);
	}

	#[test]
	fn pattern_simple() {
		assert_eq!(
			line().parse("test").into_result(),
			Ok(Line::Pattern {
				negated: false,
				segments: vec![Segment::Fixed("test".into())],
			})
		);
	}

	#[test]
	fn pattern_trailing_whitespace() {
		assert_eq!(
			line().parse("test    ").into_result(),
			Ok(Line::Pattern {
				negated: false,
				segments: vec![Segment::Fixed("test".into())],
			})
		);
	}

	#[test]
	fn pattern_escaped_trailing_whitespace() {
		assert_eq!(
			line().parse(r"test\    ").into_result(),
			Ok(Line::Pattern {
				negated: false,
				segments: vec![Segment::Fixed(r"test\ ".into())],
			})
		);
	}

	#[test]
	fn pattern_leading_slash() {
		assert_eq!(
			line().parse("/test").into_result(),
			Ok(Line::Pattern {
				negated: false,
				segments: vec![Segment::Terminal, Segment::Fixed("test".into())],
			})
		);
	}

	#[test]
	fn pattern_trailing_slash() {
		assert_eq!(
			line().parse("test/").into_result(),
			Ok(Line::Pattern {
				negated: false,
				segments: vec![Segment::Fixed("test".into()), Segment::Terminal],
			})
		);
	}

	#[test]
	fn pattern_surrounded_by_slashes() {
		assert_eq!(
			line().parse("/test/").into_result(),
			Ok(Line::Pattern {
				negated: false,
				segments: vec![
					Segment::Terminal,
					Segment::Fixed("test".into()),
					Segment::Terminal
				],
			})
		);
	}

	#[test]
	fn pattern_complex_with_wildcards() {
		assert_eq!(
			line().parse("/foo/**/b*z").into_result(),
			Ok(Line::Pattern {
				negated: false,
				segments: vec![
					Segment::Terminal,
					Segment::Fixed("foo".into()),
					Segment::All,
					Segment::Wildcard("b*z".into())
				],
			})
		);
	}

	#[test]
	fn pattern_negated() {
		assert_eq!(
			line().parse("!/foo/**/b*z").into_result(),
			Ok(Line::Pattern {
				negated: true,
				segments: vec![
					Segment::Terminal,
					Segment::Fixed("foo".into()),
					Segment::All,
					Segment::Wildcard("b*z".into())
				],
			})
		);
	}

	#[test]
	fn pattern_escaped_exclamation() {
		assert_eq!(
			line().parse(r"\!/foo/**/b*z").into_result(),
			Ok(Line::Pattern {
				negated: false,
				segments: vec![
					Segment::Fixed(r"\!".into()),
					Segment::Fixed("foo".into()),
					Segment::All,
					Segment::Wildcard("b*z".into())
				],
			})
		);
	}

	#[test]
	fn comment_empty() {
		assert_eq!(
			line().parse(r"#").into_result(),
			Ok(Line::Comment("".into()))
		);
	}

	#[test]
	fn comment_no_space() {
		assert_eq!(
			line().parse(r"#foo").into_result(),
			Ok(Line::Comment("foo".into()))
		);
	}

	#[test]
	fn comment_with_space() {
		assert_eq!(
			line().parse(r"# foo").into_result(),
			Ok(Line::Comment(" foo".into()))
		);
	}
}
