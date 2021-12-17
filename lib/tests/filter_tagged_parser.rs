use std::{collections::HashSet, str::FromStr};

use regex::Regex;
use watchexec::filter::tagged::{error::TaggedFiltererError, Filter, Matcher, Op, Pattern};

mod helpers;
use helpers::tagged::*;

#[test]
fn empty_filter() {
	assert!(matches!(
		Filter::from_str(""),
		Err(TaggedFiltererError::Parse { .. })
	));
}

#[test]
fn only_bang() {
	assert!(matches!(
		Filter::from_str("!"),
		Err(TaggedFiltererError::Parse { .. })
	));
}

#[test]
fn no_op() {
	assert!(matches!(
		Filter::from_str("foobar"),
		Err(TaggedFiltererError::Parse { .. })
	));
}

#[test]
fn path_auto_op() {
	assert_eq!(
		filter("path=foo"),
		Filter {
			in_path: None,
			on: Matcher::Path,
			op: Op::Glob,
			pat: Pattern::Glob("foo".to_string()),
			negate: false,
		}
	);
}

#[test]
fn other_auto_op() {
	assert_eq!(
		filter("kind=foo"),
		Filter {
			in_path: None,
			on: Matcher::FileEventKind,
			op: Op::InSet,
			pat: Pattern::Set(HashSet::from(["foo".to_string()])),
			negate: false,
		}
	);
}

#[test]
fn op_equal() {
	assert_eq!(
		filter("path==foo"),
		Filter {
			in_path: None,
			on: Matcher::Path,
			op: Op::Equal,
			pat: Pattern::Exact("foo".to_string()),
			negate: false,
		}
	);
}

#[test]
fn op_not_equal() {
	assert_eq!(
		filter("path!=foo"),
		Filter {
			in_path: None,
			on: Matcher::Path,
			op: Op::NotEqual,
			pat: Pattern::Exact("foo".to_string()),
			negate: false,
		}
	);
}

#[test]
fn op_regex() {
	assert_eq!(
		filter("path~=^fo+$"),
		Filter {
			in_path: None,
			on: Matcher::Path,
			op: Op::Regex,
			pat: Pattern::Regex(Regex::new("^fo+$").unwrap()),
			negate: false,
		}
	);
}

#[test]
fn op_not_regex() {
	assert_eq!(
		filter("path~!f(o|al)+"),
		Filter {
			in_path: None,
			on: Matcher::Path,
			op: Op::NotRegex,
			pat: Pattern::Regex(Regex::new("f(o|al)+").unwrap()),
			negate: false,
		}
	);
}

#[test]
fn op_glob() {
	assert_eq!(
		filter("path*=**/foo"),
		Filter {
			in_path: None,
			on: Matcher::Path,
			op: Op::Glob,
			pat: Pattern::Glob("**/foo".to_string()),
			negate: false,
		}
	);
}

#[test]
fn op_not_glob() {
	assert_eq!(
		filter("path*!foo.*"),
		Filter {
			in_path: None,
			on: Matcher::Path,
			op: Op::NotGlob,
			pat: Pattern::Glob("foo.*".to_string()),
			negate: false,
		}
	);
}

#[test]
fn op_in_set() {
	assert_eq!(
		filter("path:=foo,bar"),
		Filter {
			in_path: None,
			on: Matcher::Path,
			op: Op::InSet,
			pat: Pattern::Set(HashSet::from(["foo".to_string(), "bar".to_string()])),
			negate: false,
		}
	);
}

#[test]
fn op_not_in_set() {
	assert_eq!(
		filter("path:!baz,qux"),
		Filter {
			in_path: None,
			on: Matcher::Path,
			op: Op::NotInSet,
			pat: Pattern::Set(HashSet::from(["baz".to_string(), "qux".to_string()])),
			negate: false,
		}
	);
}

#[test]
fn quoted_single() {
	assert_eq!(
		filter("path='blanche neige'"),
		Filter {
			in_path: None,
			on: Matcher::Path,
			op: Op::Glob,
			pat: Pattern::Glob("blanche neige".to_string()),
			negate: false,
		}
	);
}

#[test]
fn quoted_double() {
	assert_eq!(
		filter("path=\"et les sept nains\""),
		Filter {
			in_path: None,
			on: Matcher::Path,
			op: Op::Glob,
			pat: Pattern::Glob("et les sept nains".to_string()),
			negate: false,
		}
	);
}

#[test]
fn negate() {
	assert_eq!(
		filter("!path~=^f[om]+$"),
		Filter {
			in_path: None,
			on: Matcher::Path,
			op: Op::Regex,
			pat: Pattern::Regex(Regex::new("^f[om]+$").unwrap()),
			negate: true,
		}
	);
}
