use chumsky::prelude::*;

use super::common::{none_of_nonl, ParserDebugExt as _, ParserErr};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Class {
	// [afg] and [!afg]
	pub negated: bool,
	pub classes: Vec<CharClass>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum CharClass {
	Single(char),      // e
	Range(char, char), // A-Z
	Named(String),     // [:alnum:]
	Collating(String), // [.ch.]
	Equivalence(char), // [=a=]
}

pub fn charclass<'src>() -> impl Parser<'src, &'src str, Class, ParserErr<'src>> + Clone {
	use CharClass::*;

	let single = none_of_nonl("]").map(Single).debug("single");
	let range = none_of_nonl("]")
		.then_ignore(just('-'))
		.then(none_of_nonl("]"))
		.map(|(a, b)| Range(a, b))
		.debug("range");
	let named = none_of_nonl(":")
		.repeated()
		.at_least(1)
		.collect::<String>()
		.map(Named)
		.delimited_by(just("[:"), just(":]"))
		.debug("named");
	let collating = none_of_nonl(".")
		.repeated()
		.at_least(1)
		.collect::<String>()
		.map(Collating)
		.delimited_by(just("[."), just(".]"))
		.debug("collating");
	let equivalence = none_of_nonl("")
		.map(Equivalence)
		.delimited_by(just("[="), just("=]"))
		.debug("equivalence");
	let bracketed = choice((named.clone(), collating.clone(), equivalence.clone()));
	let alts = choice((bracketed, range, single)).debug("alts").boxed();

	let inner0 = alts
		.clone()
		.repeated()
		.collect::<Vec<_>>()
		.debug("inner0")
		.boxed();
	let inner1 = alts
		.repeated()
		.at_least(1)
		.collect::<Vec<_>>()
		.debug("inner1")
		.boxed();

	choice((
		inner1
			.clone()
			.delimited_by(just("[!]-"), just(']'))
			.map(|mut classes| Class {
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
			})
			.debug("negbraran"),
		inner0
			.clone()
			.delimited_by(just("[!]"), just(']'))
			.map(|mut classes| Class {
				negated: true,
				classes: {
					classes.insert(0, Single(']'));
					classes
				},
			})
			.debug("negbra"),
		inner0
			.delimited_by(just("[]"), just(']'))
			.map(|mut classes| Class {
				negated: false,
				classes: {
					classes.insert(0, Single(']'));
					classes
				},
			})
			.debug("posbra"),
		inner1
			.clone()
			.delimited_by(just("[!"), just(']'))
			.map(|classes| Class {
				negated: true,
				classes,
			})
			.debug("negother"),
		inner1
			.delimited_by(just('['), just(']'))
			.map(|classes| Class {
				negated: false,
				classes,
			})
			.debug("posother"),
	))
}
