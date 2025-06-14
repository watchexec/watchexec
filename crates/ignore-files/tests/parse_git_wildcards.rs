use chumsky::prelude::*;
use ignore_files::parse::git::*;

#[test]
fn exercise() {
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
fn empty() {
	assert_eq!(wildcard().parse(r"").into_result(), Ok(vec![]));
}

#[test]
fn any() {
	use WildcardToken::*;
	assert_eq!(wildcard().parse(r"*").into_result(), Ok(vec![Any]));
}

#[test]
fn one() {
	use WildcardToken::*;
	assert_eq!(wildcard().parse(r"?").into_result(), Ok(vec![One]));
}

#[test]
fn literal() {
	use WildcardToken::*;
	assert_eq!(
		wildcard().parse(r"lit").into_result(),
		Ok(vec![Literal("lit".into())])
	);
}

#[test]
fn class_range() {
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
fn class_negated_range() {
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
fn class_special_chars() {
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
fn class_mixed() {
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
fn class_unicode() {
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
fn class_opening_inner_open_bracket() {
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
fn class_negated_inner_close_bracket() {
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
fn class_inner_close_bracket_and_bang() {
	use CharClass::*;
	use WildcardToken::*;
	assert_eq!(
		wildcard().parse(r"[][!]").into_output_errors(),
		(
			Some(vec![Class {
				negated: false,
				classes: vec![Single(']'), Single('['), Single('!'),],
			}]),
			Vec::new()
		),
		r"parsing [][!]"
	);
}

#[test]
fn class_inner_close_bracket_and_dash() {
	use CharClass::*;
	use WildcardToken::*;
	assert_eq!(
		wildcard().parse(r"[]-]").into_output_errors(),
		(
			Some(vec![Class {
				negated: false,
				classes: vec![Single(']'), Single('-'),],
			}]),
			Vec::new()
		),
		r"parsing []-]"
	);
}

#[test]
fn classes_inner_close_bracket() {
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
fn class_hyphen_start_range() {
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
fn escaped_literals() {
	use WildcardToken::*;
	assert_eq!(
		wildcard().parse(r"\\\*\?lit").into_result(),
		Ok(vec![Literal(r"\*?lit".into())])
	);
}
