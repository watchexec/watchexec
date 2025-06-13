use chumsky::{
	input::{Checkpoint, Cursor},
	inspector::Inspector,
	prelude::*,
	text::newline,
};

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Line {
	Empty,
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
	Wildcard(Vec<WildcardToken>),
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

#[derive(Clone, Copy, Debug, Default)]
struct LogInspector<T>(pub T);

impl<'src, T, I: Input<'src>> Inspector<'src, I> for LogInspector<T>
where
	<I as Input<'src>>::Token: std::fmt::Debug,
	<I as Input<'src>>::Cursor: std::fmt::Debug,
{
	type Checkpoint = ();

	#[inline(always)]
	fn on_token(&mut self, token: &<I as Input<'src>>::Token) {
		eprint!("{token:?} ");
	}

	#[inline(always)]
	fn on_save<'parse>(&self, _: &Cursor<'src, 'parse, I>) -> Self::Checkpoint {}

	#[inline(always)]
	fn on_rewind<'parse>(&mut self, checkpoint: &Checkpoint<'src, 'parse, I, Self::Checkpoint>) {
		eprint!(":{:?} ", checkpoint.cursor().inner());
	}
}

fn debug<'src, P, I, O, E>(name: &'static str, parser: P) -> DebugParser<P, O>
where
	I: Input<'src>,
	E: extra::ParserExtra<'src, I>,
	P: Parser<'src, I, O, E>,
{
	DebugParser {
		parser,
		name,
		phantom: std::marker::PhantomData,
	}
}

#[derive(Copy)]
pub struct DebugParser<A, OA> {
	parser: A,
	name: &'static str,
	#[allow(dead_code)]
	phantom: std::marker::PhantomData<OA>,
}

impl<A: Clone, OA: Clone> Clone for DebugParser<A, OA> {
	fn clone(&self) -> Self {
		Self {
			parser: self.parser.clone(),
			name: self.name.clone(),
			phantom: std::marker::PhantomData,
		}
	}
}

impl<'src, I, O, E, A> Parser<'src, I, O, E> for DebugParser<A, O>
where
	I: Input<'src>,
	E: extra::ParserExtra<'src, I>,
	A: Parser<'src, I, O, E>,
{
	#[inline(always)]
	fn go<M: chumsky::private::Mode>(
		&self,
		inp: &mut chumsky::input::InputRef<'src, '_, I, E>,
	) -> Result<<M as chumsky::private::Mode>::Output<O>, ()> {
		eprint!("[{}] ", self.name);
		self.parser.go::<M>(inp)
	}

	chumsky::go_extra!(O);
}

type ParserErr<'src> =
	chumsky::extra::Full<chumsky::error::Rich<'src, char>, LogInspector<char>, ()>;

fn any_nonl<'src>() -> impl Parser<'src, &'src str, char, ParserErr<'src>> + Clone {
	debug("any", any().and_is(newline().not()))
}

fn none_of_nonl<'src>(
	none: &'src str,
) -> impl Parser<'src, &'src str, char, ParserErr<'src>> + Clone {
	debug(
		"none_of",
		any().and_is(newline().or(one_of(none).to(())).not()),
	)
}

fn class<'src>() -> impl Parser<'src, &'src str, WildcardToken, ParserErr<'src>> {
	use CharClass::*;

	let single = debug("single", none_of_nonl("/]").map(Single));
	let range = debug(
		"range",
		none_of_nonl("/]")
			.then_ignore(just('-'))
			.then(none_of_nonl("/]"))
			.map(|(a, b)| Range(a, b)),
	);
	let named = debug(
		"named",
		none_of_nonl(":/")
			.repeated()
			.at_least(1)
			.collect::<String>()
			.map(Named)
			.delimited_by(just("[:"), just(":]")),
	);
	let collating = debug(
		"collating",
		none_of_nonl("./")
			.repeated()
			.at_least(1)
			.collect::<String>()
			.map(Collating)
			.delimited_by(just("[."), just(".]")),
	);
	let equivalence = debug(
		"equivalence",
		none_of_nonl("/")
			.map(Equivalence)
			.delimited_by(just("[="), just("=]")),
	);
	let alts = debug(
		"alts",
		choice((named, collating, equivalence, range, single.clone())).or(single),
	)
	.boxed();

	let inner0 = debug("inner0", alts.clone().repeated().collect::<Vec<_>>()).boxed();
	let inner1 = debug("inner1", alts.repeated().at_least(1).collect::<Vec<_>>()).boxed();

	choice((
		debug(
			"negbraran",
			inner1
				.clone()
				.delimited_by(just("[!]-"), just(']'))
				.map(|mut classes| WildcardToken::Class {
					negated: true,
					classes: {
						if let Single(c) = *classes.first().unwrap() {
							classes[0] = Range(']', c);
							classes
						} else {
							classes.insert(0, Single(']'));
							classes.insert(1, Single('-'));
							classes
						}
					},
				}),
		),
		debug(
			"negbra",
			inner0
				.clone()
				.delimited_by(just("[!]"), just(']'))
				.map(|mut classes| WildcardToken::Class {
					negated: true,
					classes: {
						classes.insert(0, Single(']'));
						classes
					},
				}),
		),
		debug(
			"posbra",
			inner0
				.delimited_by(just("[]"), just(']'))
				.map(|mut classes| WildcardToken::Class {
					negated: false,
					classes: {
						classes.insert(0, Single(']'));
						classes
					},
				}),
		),
		debug(
			"negother",
			inner1
				.clone()
				.delimited_by(just("[!"), just(']'))
				.map(|classes| WildcardToken::Class {
					negated: true,
					classes,
				}),
		),
		debug(
			"posother",
			inner1
				.delimited_by(just('['), just(']'))
				.map(|classes| WildcardToken::Class {
					negated: false,
					classes,
				}),
		),
	))
}

fn wildcard<'src>() -> impl Parser<'src, &'src str, Vec<WildcardToken>, ParserErr<'src>> {
	use WildcardToken::*;

	let literal = debug(
		"literal",
		none_of_nonl("/[]*?\\")
			.repeated()
			.at_least(1)
			.collect::<String>()
			.map(Literal),
	);

	choice((
		just('*').to(Any),
		just('?').to(One),
		just(r"\\").to(Literal(r"\".into())),
		just(r"\.").to(Literal(r".".into())), // undocumented
		just(r"\?").to(Literal(r"?".into())),
		just(r"\[").to(Literal(r"[".into())),
		just(r"\*").to(Literal(r"*".into())),
		just(r"\!").to(Literal(r"\!".into())), // bangs don't need escaping except at the very start, but we still need to parse that here
		just(r"\#").to(Literal(r"\#".into())), // hashes don't need escaping except at the very start, but we still need to parse that here
		just(r"\ ").to(Literal(r"\ ".into())), // spaces don't need escaping except at the end, where we have special handling in line()
		class(),
		literal,
		one_of("[]").map(|c: char| Literal(c.into())),
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

fn line<'src>() -> impl Parser<'src, &'src str, Line, ParserErr<'src>> {
	let comment = just('#').ignore_then(any_nonl().repeated().collect::<String>());

	let negator = just('!').or_not().map(|exists| exists.is_some());

	let segments = wildcard()
		.map(|seg| {
			if seg.is_empty() {
				Segment::Terminal
			} else if &seg == &[WildcardToken::Any, WildcardToken::Any] {
				Segment::All
			} else if seg.iter().all(|w| matches!(w, WildcardToken::Literal(_))) {
				Segment::Fixed(
					seg.into_iter()
						.map(|w| {
							if let WildcardToken::Literal(l) = w {
								l
							} else {
								unreachable!()
							}
						})
						.collect(),
				)
			} else {
				Segment::Wildcard(seg)
			}
		})
		.separated_by(just('/'))
		.collect::<Vec<_>>();

	comment
		.map(|content| Line::Comment(content))
		.or(negator.then(segments).map(|(negated, mut segments)| {
			if segments == [Segment::Terminal] {
				return Line::Empty;
			}

			match segments.first_mut() {
				Some(Segment::Fixed(first)) => {
					handle_escaped_starts(first);
				}
				Some(Segment::Wildcard(first)) => {
					if let Some(WildcardToken::Literal(ref mut first)) = first.first_mut() {
						handle_escaped_starts(first);
					}
				}
				_ => {}
			}

			match segments.last_mut() {
				Some(Segment::Fixed(ref mut last)) => {
					trim_and_handle_whitespace_escape(last);
				}
				Some(Segment::Wildcard(ref mut last)) => {
					if let Some(WildcardToken::Literal(ref mut last)) = last.last_mut() {
						trim_and_handle_whitespace_escape(last);
					}
				}
				_ => {}
			}

			Line::Pattern { negated, segments }
		}))
}

fn file<'src>() -> impl Parser<'src, &'src str, Vec<Line>, ParserErr<'src>> {
	line().separated_by(newline()).collect::<Vec<_>>()
}

fn handle_escaped_starts(s: &mut String) {
	if s.starts_with(r"\!") || s.starts_with(r"\#") {
		*s = s[1..].into();
	}
}

fn trim_and_handle_whitespace_escape(s: &mut String) {
	let without_trailing_whitespace = s.trim_end();
	if let Some(without_backslash) = without_trailing_whitespace.strip_suffix(r"\") {
		if s.len() >= without_trailing_whitespace.len() + 2 {
			dbg!(&s, &without_trailing_whitespace, &without_backslash);
			*s = format!(
				"{without_backslash}{}",
				// the next char after the backslash
				s.get(without_trailing_whitespace.len()..)
					.and_then(|it| it.chars().next())
					.unwrap_or(' ')
			);
			return;
		}
	}

	*s = without_trailing_whitespace.into();
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
					r"lit[][!][]-]lit*?lit[*!?][0a-cf][A-Z][!a-z][[=a=]x[:alnum:]y[.ch.]][[?*\][!]a-][--0]\\\*\?lit"
				)
				.into_result(),
			Ok(vec![
				Literal("lit".into()),
				Class {
					negated: false,
					classes: vec![Single(']'), Single('['), Single('!'),],
				},
				Class {
					negated: false,
					classes: vec![Single(']'), Single('-'),],
				},
				Literal("lit".into()),
				Any,
				One,
				Literal("lit".into()),
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
					classes: vec![Range('A', 'Z')],
				},
				Class {
					negated: true,
					classes: vec![Range('a', 'z')],
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
	fn wildcard_class_opening_inner_open_bracket_single() {
		use CharClass::*;
		use WildcardToken::*;
		assert_eq!(
			class().parse(r"[[?*\]").into_output_errors(),
			(
				Some(Class {
					negated: false,
					classes: vec![Single('['), Single('?'), Single('*'), Single('\\'),],
				}),
				Vec::new()
			),
			r"parsing [[?*\]"
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
	fn wildcard_class_negated_inner_close_bracket_single() {
		use CharClass::*;
		use WildcardToken::*;

		assert_eq!(
			class().parse(r"[!]a-]").into_output_errors(),
			(
				Some(Class {
					negated: true,
					classes: vec![Single(']'), Single('a'), Single('-'),],
				}),
				Vec::new()
			),
			r"parsing [!]a-]"
		);
	}

	#[test]
	fn wildcard_class_inner_close_bracket_and_bang() {
		use CharClass::*;
		use WildcardToken::*;
		assert_eq!(
			class().parse(r"[][!]").into_output_errors(),
			(
				Some(Class {
					negated: false,
					classes: vec![Single(']'), Single('['), Single('!'),],
				},),
				Vec::new()
			),
			r"parsing [][!]"
		);
	}

	#[test]
	fn wildcard_class_inner_close_bracket_and_dash() {
		use CharClass::*;
		use WildcardToken::*;
		assert_eq!(
			class().parse(r"[]-]").into_output_errors(),
			(
				Some(Class {
					negated: false,
					classes: vec![Single(']'), Single('-'),],
				},),
				Vec::new()
			),
			r"parsing []-]"
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
				segments: vec![Segment::Fixed(r"test ".into())],
			})
		);
	}

	#[test]
	fn pattern_faux_escaped_trailing_whitespace() {
		assert_eq!(
			line().parse(r"foo/te\ st/bar").into_result(),
			Ok(Line::Pattern {
				negated: false,
				segments: vec![
					Segment::Fixed("foo".into()),
					Segment::Fixed(r"te\ st".into()),
					Segment::Fixed("bar".into())
				],
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
					Segment::Wildcard(vec![
						WildcardToken::Literal("b".into()),
						WildcardToken::Any,
						WildcardToken::Literal("z".into()),
					])
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
					Segment::Wildcard(vec![
						WildcardToken::Literal("b".into()),
						WildcardToken::Any,
						WildcardToken::Literal("z".into()),
					])
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
					Segment::Fixed(r"!".into()),
					Segment::Fixed("foo".into()),
					Segment::All,
					Segment::Wildcard(vec![
						WildcardToken::Literal("b".into()),
						WildcardToken::Any,
						WildcardToken::Literal("z".into()),
					])
				],
			})
		);
	}

	#[test]
	fn pattern_faux_escaped_exclamation() {
		assert_eq!(
			line().parse(r"/fo\!o/**/b*z").into_result(),
			Ok(Line::Pattern {
				negated: false,
				segments: vec![
					Segment::Terminal,
					Segment::Fixed(r"fo\!o".into()),
					Segment::All,
					Segment::Wildcard(vec![
						WildcardToken::Literal("b".into()),
						WildcardToken::Any,
						WildcardToken::Literal("z".into()),
					])
				],
			})
		);
	}

	#[test]
	fn pattern_escaped_hash() {
		assert_eq!(
			line().parse(r"\#/foo/**/b*z").into_result(),
			Ok(Line::Pattern {
				negated: false,
				segments: vec![
					Segment::Fixed(r"#".into()),
					Segment::Fixed("foo".into()),
					Segment::All,
					Segment::Wildcard(vec![
						WildcardToken::Literal("b".into()),
						WildcardToken::Any,
						WildcardToken::Literal("z".into()),
					])
				],
			})
		);
	}

	#[test]
	fn pattern_faux_escaped_hash() {
		assert_eq!(
			line().parse(r"/fo\#o/**/b*z").into_result(),
			Ok(Line::Pattern {
				negated: false,
				segments: vec![
					Segment::Terminal,
					Segment::Fixed(r"fo\#o".into()),
					Segment::All,
					Segment::Wildcard(vec![
						WildcardToken::Literal("b".into()),
						WildcardToken::Any,
						WildcardToken::Literal("z".into()),
					])
				],
			})
		);
	}

	#[test]
	fn pattern_escaped_periods() {
		assert_eq!(
			line().parse(r"\.foo/\.bar*").into_result(),
			Ok(Line::Pattern {
				negated: false,
				segments: vec![
					Segment::Fixed(".foo".into()),
					Segment::Wildcard(vec![
						WildcardToken::Literal(".bar".into()),
						WildcardToken::Any,
					])
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

	#[test]
	fn inline_file() {
		assert_eq!(
			file()
				.parse(
					r"
target
/watchexec-*

# log files
watchexec.*.log
"
				)
				.into_output_errors(),
			(
				Some(vec![
					Line::Empty,
					Line::Pattern {
						negated: false,
						segments: vec![Segment::Fixed("target".into())],
					},
					Line::Pattern {
						negated: false,
						segments: vec![
							Segment::Terminal,
							Segment::Wildcard(vec![
								WildcardToken::Literal("watchexec-".into()),
								WildcardToken::Any,
							]),
						],
					},
					Line::Empty,
					Line::Comment(" log files".into()),
					Line::Pattern {
						negated: false,
						segments: vec![Segment::Wildcard(vec![
							WildcardToken::Literal("watchexec.".into()),
							WildcardToken::Any,
							WildcardToken::Literal(".log".into())
						])]
					},
					Line::Empty,
				]),
				Vec::new()
			)
		);
	}
}
