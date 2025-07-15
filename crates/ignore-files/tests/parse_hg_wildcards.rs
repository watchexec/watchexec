use chumsky::prelude::*;
use ignore_files::parse::{
	charclass::{CharClass, Class as Klass},
	hg::glob::*,
};

#[test]
fn exercise() {
	use CharClass::*;
	use Token::*;
	assert_eq!(
		glob()
			.parse(
				r"lit,[][!][]-]/lit*?lit[*!?][0a-cf][A-Z][!a-z][[=a=]x[:alnum:]/[.ch.]][[?*\][!]a-][--0]\\\*\?\{\[{a,b*c,d/e}lit"
			)
			.into_result(),
		Ok(Glob(vec![
			Literal("lit,".into()),
			Class(Klass {
				negated: false,
				classes: vec![Single(']'), Single('['), Single('!'),],
			}),
			Class(Klass {
				negated: false,
				classes: vec![Single(']'), Single('-'),],
			}),
			Separator,
			Literal("lit".into()),
			AnyInSegment,
			One,
			Literal("lit".into()),
			Class(Klass {
				negated: false,
				classes: vec![Single('*'), Single('!'), Single('?'),],
			}),
			Class(Klass {
				negated: false,
				classes: vec![Single('0'), Range('a', 'c'), Single('f'),],
			}),
			Class(Klass {
				negated: false,
				classes: vec![Range('A', 'Z')],
			}),
			Class(Klass {
				negated: true,
				classes: vec![Range('a', 'z')],
			}),
			Class(Klass {
				negated: false,
				classes: vec![
					Equivalence('a'),
					Single('x'),
					Named("alnum".into()),
					Single('/'),
					Collating("ch".into()),
				],
			}),
			Class(Klass {
				negated: false,
				classes: vec![Single('['), Single('?'), Single('*'), Single('\\'),],
			}),
			Class(Klass {
				negated: true,
				classes: vec![Single(']'), Single('a'), Single('-'),],
			}),
			Class(Klass {
				negated: false,
				classes: vec![Range('-', '0')],
			}),
			Literal(r"\*?{[".into()),
			Alt(vec![
				Glob(vec![Literal("a".into())]),
				Glob(vec![Literal("b".into()), AnyInSegment, Literal("c".into())]),
				Glob(vec![Literal("d".into()), Separator, Literal("e".into())]),
			]),
			Literal("lit".into()),
		]))
	);
}

#[test]
fn exercise_debug_parts() {
	use CharClass::*;
	use Token::*;
	// Each part of the original exercise pattern, tested individually
	assert_eq!(
		glob().parse("lit,").into_result(),
		Ok(Glob(vec![Literal("lit,".into())]))
	);
	assert_eq!(
		glob().parse("[][!]").into_result(),
		Ok(Glob(vec![Class(Klass {
			negated: false,
			classes: vec![Single(']'), Single('['), Single('!')],
		})]))
	);
	assert_eq!(
		glob().parse("[]-]").into_result(),
		Ok(Glob(vec![Class(Klass {
			negated: false,
			classes: vec![Single(']'), Single('-')],
		})]))
	);
	assert_eq!(
		glob().parse("/lit").into_result(),
		Ok(Glob(vec![Separator, Literal("lit".into())]))
	);
	assert_eq!(
		glob().parse("*?lit").into_result(),
		Ok(Glob(vec![AnyInSegment, One, Literal("lit".into())]))
	);
	assert_eq!(
		glob().parse("[*!?]").into_result(),
		Ok(Glob(vec![Class(Klass {
			negated: false,
			classes: vec![Single('*'), Single('!'), Single('?')],
		})]))
	);
	assert_eq!(
		glob().parse("[0a-cf]").into_result(),
		Ok(Glob(vec![Class(Klass {
			negated: false,
			classes: vec![Single('0'), Range('a', 'c'), Single('f')],
		})]))
	);
	assert_eq!(
		glob().parse("[A-Z]").into_result(),
		Ok(Glob(vec![Class(Klass {
			negated: false,
			classes: vec![Range('A', 'Z')],
		})]))
	);
	assert_eq!(
		glob().parse("[!a-z]").into_result(),
		Ok(Glob(vec![Class(Klass {
			negated: true,
			classes: vec![Range('a', 'z')],
		})]))
	);
	assert_eq!(
		glob().parse("[[=a=]x[:alnum:]/[.ch.]]").into_result(),
		Ok(Glob(vec![Class(Klass {
			negated: false,
			classes: vec![
				Equivalence('a'),
				Single('x'),
				Named("alnum".into()),
				Single('/'),
				Collating("ch".into()),
			],
		})]))
	);
	assert_eq!(
		glob().parse("[[?*\\]").into_result(),
		Ok(Glob(vec![Class(Klass {
			negated: false,
			classes: vec![Single('['), Single('?'), Single('*'), Single('\\')],
		})]))
	);
	assert_eq!(
		glob().parse("[!]a-]").into_result(),
		Ok(Glob(vec![Class(Klass {
			negated: true,
			classes: vec![Single(']'), Single('a'), Single('-')],
		})]))
	);
	assert_eq!(
		glob().parse("[--0]").into_result(),
		Ok(Glob(vec![Class(Klass {
			negated: false,
			classes: vec![Range('-', '0')],
		})]))
	);
	assert_eq!(
		glob().parse(r"\\\*\?\{\[").into_result(),
		Ok(Glob(vec![Literal(r"\*?{[".into())]))
	);
	assert_eq!(
		glob().parse("{a,b*c,d/e}lit").into_result(),
		Ok(Glob(vec![
			Alt(vec![
				Glob(vec![Literal("a".into())]),
				Glob(vec![Literal("b".into()), AnyInSegment, Literal("c".into())]),
				Glob(vec![Literal("d".into()), Separator, Literal("e".into())]),
			]),
			Literal("lit".into()),
		]))
	);
}

#[test]
fn empty() {
	assert_eq!(glob().parse(r"").into_result(), Ok(Glob(vec![])));
}

#[test]
fn anys() {
	use Token::*;
	assert_eq!(
		glob().parse(r"*").into_result(),
		Ok(Glob(vec![AnyInSegment]))
	);
	assert_eq!(glob().parse(r"**").into_result(), Ok(Glob(vec![AnyInPath])));
}

#[test]
fn one() {
	use Token::*;
	assert_eq!(glob().parse(r"?").into_result(), Ok(Glob(vec![One])));
}

#[test]
fn literal() {
	use Token::*;
	assert_eq!(
		glob().parse(r"lit").into_result(),
		Ok(Glob(vec![Literal("lit".into())]))
	);
}

#[test]
fn segmented() {
	use Token::*;
	assert_eq!(
		glob().parse(r"a/b/c").into_result(),
		Ok(Glob(vec![
			Literal("a".into()),
			Separator,
			Literal("b".into()),
			Separator,
			Literal("c".into())
		]))
	);
}

#[test]
fn class_range() {
	use CharClass::*;
	use Token::*;
	assert_eq!(
		glob().parse(r"[A-Z]").into_result(),
		Ok(Glob(vec![Class(Klass {
			negated: false,
			classes: vec![Range('A', 'Z')],
		})]))
	);
}

#[test]
fn class_negated_range() {
	use CharClass::*;
	use Token::*;
	assert_eq!(
		glob().parse(r"[!a-z]").into_result(),
		Ok(Glob(vec![Class(Klass {
			negated: true,
			classes: vec![Range('a', 'z')],
		})]))
	);
}

#[test]
fn class_special_chars() {
	use CharClass::*;
	use Token::*;
	assert_eq!(
		glob().parse(r"[*!?]").into_result(),
		Ok(Glob(vec![Class(Klass {
			negated: false,
			classes: vec![Single('*'), Single('!'), Single('?'),],
		})]))
	);
}

#[test]
fn class_mixed() {
	use CharClass::*;
	use Token::*;
	assert_eq!(
		glob().parse(r"[0a-cf]").into_result(),
		Ok(Glob(vec![Class(Klass {
			negated: false,
			classes: vec![Single('0'), Range('a', 'c'), Single('f'),],
		})]))
	);
}

#[test]
fn class_unicode() {
	use CharClass::*;
	use Token::*;
	assert_eq!(
		glob().parse(r"[[=a=]x[:alnum:]y[.ch.]]").into_result(),
		Ok(Glob(vec![Class(Klass {
			negated: false,
			classes: vec![
				Equivalence('a'),
				Single('x'),
				Named("alnum".into()),
				Single('y'),
				Collating("ch".into()),
			],
		})]))
	);
}

#[test]
fn class_opening_inner_open_bracket() {
	use CharClass::*;
	use Token::*;
	assert_eq!(
		glob().parse(r"[[?*\]").into_result(),
		Ok(Glob(vec![Class(Klass {
			negated: false,
			classes: vec![Single('['), Single('?'), Single('*'), Single('\\'),],
		})]))
	);
}

#[test]
fn class_negated_inner_close_bracket() {
	use CharClass::*;
	use Token::*;
	assert_eq!(
		glob().parse(r"[!]a-]").into_result(),
		Ok(Glob(vec![Class(Klass {
			negated: true,
			classes: vec![Single(']'), Single('a'), Single('-'),],
		})]))
	);
}

#[test]
fn class_inner_close_bracket_and_bang() {
	use CharClass::*;
	use Token::*;
	assert_eq!(
		glob().parse(r"[][!]").into_output_errors(),
		(
			Some(Glob(vec![Class(Klass {
				negated: false,
				classes: vec![Single(']'), Single('['), Single('!'),],
			})])),
			Vec::new()
		),
		r"parsing [][!]"
	);
}

#[test]
fn class_inner_close_bracket_and_dash() {
	use CharClass::*;
	use Token::*;
	assert_eq!(
		glob().parse(r"[]-]").into_output_errors(),
		(
			Some(Glob(vec![Class(Klass {
				negated: false,
				classes: vec![Single(']'), Single('-'),],
			})])),
			Vec::new()
		),
		r"parsing []-]"
	);
}

#[test]
fn classes_inner_close_bracket() {
	use CharClass::*;
	use Token::*;
	assert_eq!(
		glob().parse(r"[][!][]-]").into_result(),
		Ok(Glob(vec![
			Class(Klass {
				negated: false,
				classes: vec![Single(']'), Single('['), Single('!'),],
			}),
			Class(Klass {
				negated: false,
				classes: vec![Single(']'), Single('-'),],
			})
		]))
	);
}

#[test]
fn class_hyphen_start_range() {
	use CharClass::*;
	use Token::*;
	assert_eq!(
		glob().parse(r"[--0]").into_result(),
		Ok(Glob(vec![Class(Klass {
			negated: false,
			classes: vec![Range('-', '0')],
		})]))
	);
}

#[test]
fn escaped_literals() {
	use Token::*;
	assert_eq!(
		glob().parse(r"\\\*\?lit").into_result(),
		Ok(Glob(vec![Literal(r"\*?lit".into())]))
	);
}

#[test]
fn alt_simple() {
	use Token::*;
	assert_eq!(
		glob().parse(r"{a,b}").into_result(),
		Ok(Glob(vec![Alt(vec![
			Glob(vec![Literal("a".into())]),
			Glob(vec![Literal("b".into())]),
		])]))
	);
}

#[test]
fn alt_single() {
	use Token::*;
	assert_eq!(
		glob().parse(r"{a}").into_result(),
		Ok(Glob(vec![Alt(vec![Glob(vec![Literal("a".into())]),])]))
	);
}

#[test]
fn alt_with_glob() {
	use Token::*;
	assert_eq!(
		glob().parse(r"{a,b,c*d}").into_result(),
		Ok(Glob(vec![Alt(vec![
			Glob(vec![Literal("a".into())]),
			Glob(vec![Literal("b".into())]),
			Glob(vec![Literal("c".into()), AnyInSegment, Literal("d".into())]),
		])]))
	);
}

#[test]
fn alt_empty() {
	use Token::*;
	assert_eq!(
		glob().parse(r"{}").into_result(),
		Ok(Glob(vec![Alt(vec![])]))
	);
}
