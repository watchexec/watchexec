use chumsky::prelude::*;

use crate::parse::{
	charclass::{charclass, Class},
	common::{none_of_nonl, ParserDebugExt as _, ParserErr},
};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Glob(pub Vec<Token>);

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Token {
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

		let literal = none_of_nonl("/[]*?\\{},")
			.repeated()
			.at_least(1)
			.collect::<String>()
			.map(Literal)
			.debug("literal");

		let alt = glob
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
			.debug("alt");

		choice((
			just("**").to(AnyInPath),
			just('*').to(AnyInSegment),
			just('?').to(One),
			just(r"\").ignore_then(
				// as defined in the mercurial repo at rust/hg-core/src/filepatterns.rs
				choice((
					just('('),
					just(')'),
					just('['),
					just(']'),
					just('{'),
					just('}'),
					just('?'),
					just('*'),
					just('+'),
					just('-'),
					just('|'),
					just('^'),
					just('$'),
					just('\\'),
					just('.'),
					just('&'),
					just('~'),
					just('#'),
					just('\t'),
					just('\n'),
					just('\r'),
					just('\x0b'),
					just('\x0c'),
				))
				.map(|lit| Literal(lit.into())),
			),
			charclass().map(Class),
			alt,
			literal,
			one_of("[],").map(|c: char| Literal(c.into())),
		))
		.repeated()
		.collect::<Vec<_>>()
		.map(|toks| {
			Glob(toks.into_iter().fold(Vec::new(), |mut acc, tok| {
				match (tok, acc.last_mut()) {
					(Literal(tok), Some(&mut Literal(ref mut last))) => {
						last.push_str(&tok);
					}
					(tok, _) => acc.push(tok),
				}
				acc
			}))
		})
	})
}
