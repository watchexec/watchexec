use chumsky::{prelude::*, text::newline};

use super::{
	charclass::{charclass, Class},
	common::{any_nonl, none_of_nonl, ParserDebugExt as _, ParserErr},
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
	Class(Class),
	Literal(String),
}

pub fn wildcard<'src>() -> impl Parser<'src, &'src str, Vec<WildcardToken>, ParserErr<'src>> {
	use WildcardToken::*;

	let literal = none_of_nonl("/[]*?\\")
		.repeated()
		.at_least(1)
		.collect::<String>()
		.map(Literal)
		.debug("literal");

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
		charclass().map(Class),
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

pub fn line<'src>() -> impl Parser<'src, &'src str, Line, ParserErr<'src>> {
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

pub fn file<'src>() -> impl Parser<'src, &'src str, Vec<Line>, ParserErr<'src>> {
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
