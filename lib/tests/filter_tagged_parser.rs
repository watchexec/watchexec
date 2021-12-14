use std::str::FromStr;

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
