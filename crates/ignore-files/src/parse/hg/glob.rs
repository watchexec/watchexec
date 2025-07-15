use chumsky::prelude::*;

use crate::parse::{
	charclass::{charclass, Class},
	common::{none_of_nonl, ParserDebugExt as _, ParserErr},
};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Glob(pub Vec<Token>);

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Token {
	Separator,      // /
	AnyInSegment,   // *
	AnyInPath,      // **
	One,            // ?
	Class(Class),   // []
	Alt(Vec<Glob>), // {}
	Literal(String),
}

pub fn glob<'src>() -> impl Parser<'src, &'src str, Glob, ParserErr<'src>> {
	recursive(|glob| {
		use Token::*;

		// Parser for a literal sequence, including escaped characters, outside alternation (commas allowed)
		let literal_outside_alt = choice((
			just('\\').then(none_of_nonl("")).map(|(_, c)| c),
			none_of_nonl("[]*?\\{}/"),
		))
		.repeated()
		.at_least(1)
		.collect::<String>()
		.map(Literal)
		.debug("literal_outside_alt");

		// Parser for a literal sequence, including escaped characters, inside alternation (commas NOT allowed)
		let literal_inside_alt = choice((
			just('\\').then(none_of_nonl("")).map(|(_, c)| c),
			none_of_nonl("[]*?\\{}/,"),
		))
		.repeated()
		.at_least(1)
		.collect::<String>()
		.map(Literal)
		.debug("literal_inside_alt");

		// Alternation parser uses the inside-alt literal parser
		let alt = recursive(|alt_glob| {
			let alt_item = choice((
				just('/').to(Separator),
				just("**").to(AnyInPath),
				just('*').to(AnyInSegment),
				just('?').to(One),
				charclass().map(Class),
				alt_glob,
				literal_inside_alt,
			))
			.repeated()
			.collect::<Vec<_>>()
			.map(|toks| {
				let mut acc = Vec::new();
				for tok in toks {
					match tok {
						Literal(s) => {
							let mut buf = String::new();
							for c in s.chars() {
								if c == '/' {
									if !buf.is_empty() {
										acc.push(Token::Literal(buf.clone()));
										buf.clear();
									}
									acc.push(Token::Separator);
								} else {
									buf.push(c);
								}
							}
							if !buf.is_empty() {
								acc.push(Token::Literal(buf));
							}
						}
						_ => acc.push(tok),
					}
				}
				Glob(acc)
			});
			alt_item
				.separated_by(just(','))
				.collect::<Vec<_>>()
				.map(|alts| {
					Alt(if alts == vec![Glob(vec![])] {
						Vec::new()
					} else {
						alts
					})
				})
				.delimited_by(just('{'), just('}'))
				.debug("alt")
		});

		// Main glob parser uses the outside-alt literal parser
		choice((
			just('/').to(Separator),
			just("**").to(AnyInPath),
			just('*').to(AnyInSegment),
			just('?').to(One),
			charclass().map(Class),
			alt,
			literal_outside_alt,
		))
		.repeated()
		.collect::<Vec<_>>()
		.map(|toks| {
			// Split any Literal containing '/' into segments
			let mut acc = Vec::new();
			for tok in toks {
				match tok {
					Literal(s) => {
						let mut buf = String::new();
						for c in s.chars() {
							if c == '/' {
								if !buf.is_empty() {
									acc.push(Token::Literal(buf.clone()));
									buf.clear();
								}
								acc.push(Token::Separator);
							} else {
								buf.push(c);
							}
						}
						if !buf.is_empty() {
							acc.push(Token::Literal(buf));
						}
					}
					_ => acc.push(tok),
				}
			}
			Glob(acc)
		})
	})
}
