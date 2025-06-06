use winnow::combinator::{opt, separated, seq};
use winnow::prelude::*;
use winnow::token::{literal, rest, take_until};
use winnow::Result;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Pattern {
	pub negated: bool,
	pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Segment {
	Terminal,
	Fixed(String),
	Wildcard(String),
	All,
}

impl std::str::FromStr for Pattern {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		pattern.parse(s).map_err(|e| e.to_string())
	}
}

fn pattern(input: &mut &str) -> Result<Pattern> {
	seq!(Pattern {
		negated: opt(literal('!')).map(|l| l.is_some()),
		segments: (
			opt(literal('/')),
			separated(0.., rest, '/'),
			opt(literal('/'))
		)
			.map(
				|(begin, segments, end): (Option<&str>, Vec<&str>, Option<&str>)| {
					let mut segments: Vec<Segment> = segments
						.into_iter()
						.map(|segment| {
							if segment == "**" {
								Segment::All
							} else if segment.contains(|c| c == '*' || c == '?' || c == '[') {
								// "a string is a wildcard pattern if it contains one of the characters '?', '*', or '['"
								Segment::Wildcard(segment.to_string())
							} else {
								Segment::Fixed(segment.to_string())
							}
						})
						.collect();
					if begin.is_some() {
						segments.insert(0, Segment::Terminal);
					}
					if end.is_some() {
						segments.push(Segment::Terminal);
					}
					segments
				}
			),
	})
	.parse_next(input)
}

#[test]
fn test_patterns() {
	assert_eq!(
		pattern.parse_peek("test"),
		Ok((
			"",
			Pattern {
				negated: false,
				segments: vec![Segment::Fixed("test".into())],
			}
		))
	);
	assert_eq!(
		pattern.parse_peek("/test"),
		Ok((
			"",
			Pattern {
				negated: false,
				segments: vec![Segment::Terminal, Segment::Fixed("test".into())],
			}
		))
	);
	assert_eq!(
		pattern.parse_peek("test/"),
		Ok((
			"",
			Pattern {
				negated: false,
				segments: vec![Segment::Fixed("test".into()), Segment::Terminal],
			}
		))
	);
	assert_eq!(
		pattern.parse_peek("/test/"),
		Ok((
			"",
			Pattern {
				negated: false,
				segments: vec![
					Segment::Terminal,
					Segment::Fixed("test".into()),
					Segment::Terminal
				],
			}
		))
	);
	assert_eq!(
		pattern.parse_peek("/foo/**/b*z"),
		Ok((
			"",
			Pattern {
				negated: false,
				segments: vec![
					Segment::Terminal,
					Segment::Fixed("foo".into()),
					Segment::All,
					Segment::Wildcard("b*z".into())
				],
			}
		))
	);
}
