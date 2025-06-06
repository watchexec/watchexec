use chumsky::prelude::*;

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

fn pattern<'src>() -> impl Parser<'src, &'src str, Pattern> {
	none_of("/")
		.repeated()
		.collect::<String>()
		.map(|seg| {
			if seg.is_empty() {
				Segment::Terminal
			} else if seg == "**" {
				Segment::All
			} else if seg.contains(['*', '?', '[']) {
				Segment::Wildcard(seg)
			} else {
				Segment::Fixed(seg)
			}
		})
		.separated_by(just('/'))
		.collect::<Vec<_>>()
		.map(|segments| Pattern {
			negated: false,
			segments,
		})
}

#[test]
fn test_patterns() {
	assert_eq!(
		pattern().parse("test").into_result(),
		Ok(Pattern {
			negated: false,
			segments: vec![Segment::Fixed("test".into())],
		})
	);
	assert_eq!(
		pattern().parse("/test").into_result(),
		Ok(Pattern {
			negated: false,
			segments: vec![Segment::Terminal, Segment::Fixed("test".into())],
		})
	);
	assert_eq!(
		pattern().parse("test/").into_result(),
		Ok(Pattern {
			negated: false,
			segments: vec![Segment::Fixed("test".into()), Segment::Terminal],
		})
	);
	assert_eq!(
		pattern().parse("/test/").into_result(),
		Ok(Pattern {
			negated: false,
			segments: vec![
				Segment::Terminal,
				Segment::Fixed("test".into()),
				Segment::Terminal
			],
		})
	);
	assert_eq!(
		pattern().parse("/foo/**/b*z").into_result(),
		Ok(Pattern {
			negated: false,
			segments: vec![
				Segment::Terminal,
				Segment::Fixed("foo".into()),
				Segment::All,
				Segment::Wildcard("b*z".into())
			],
		})
	);
}
