use chumsky::prelude::*;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Line {
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
	Wildcard(String),
	All,
}

fn line<'src>() -> impl Parser<'src, &'src str, Line> {
	let comment = just('#').ignore_then(any().repeated().collect::<String>());

	let negator = just('!').or_not().map(|exists| exists.is_some());

	let segments = none_of("/")
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
		.collect::<Vec<_>>();

	comment
		.map(|content| Line::Comment(content))
		.or(negator.then(segments).map(|(negated, mut segments)| {
			if let Some(Segment::Wildcard(ref mut last) | Segment::Fixed(ref mut last)) =
				segments.last_mut()
			{
				let final_length = {
					let without_trailing_whitespace = last.trim_end();
					if without_trailing_whitespace.ends_with('\\') {
						without_trailing_whitespace.len() + 1
					} else {
						without_trailing_whitespace.len()
					}
				};
				let _ = last.split_off(final_length);
			}

			Line::Pattern { negated, segments }
		}))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn pattern_simple() {
		assert_eq!(
			line().parse("test").into_result(),
			Ok(Line::Pattern {
				negated: false,
				segments: vec![Segment::Fixed("test".into())],
			})
		);
	}

	#[test]
	fn pattern_trailing_whitespace() {
		assert_eq!(
			line().parse("test    ").into_result(),
			Ok(Line::Pattern {
				negated: false,
				segments: vec![Segment::Fixed("test".into())],
			})
		);
	}

	#[test]
	fn pattern_escaped_trailing_whitespace() {
		assert_eq!(
			line().parse(r"test\    ").into_result(),
			Ok(Line::Pattern {
				negated: false,
				segments: vec![Segment::Fixed(r"test\ ".into())],
			})
		);
	}

	#[test]
	fn pattern_leading_slash() {
		assert_eq!(
			line().parse("/test").into_result(),
			Ok(Line::Pattern {
				negated: false,
				segments: vec![Segment::Terminal, Segment::Fixed("test".into())],
			})
		);
	}

	#[test]
	fn pattern_trailing_slash() {
		assert_eq!(
			line().parse("test/").into_result(),
			Ok(Line::Pattern {
				negated: false,
				segments: vec![Segment::Fixed("test".into()), Segment::Terminal],
			})
		);
	}

	#[test]
	fn pattern_surrounded_by_slashes() {
		assert_eq!(
			line().parse("/test/").into_result(),
			Ok(Line::Pattern {
				negated: false,
				segments: vec![
					Segment::Terminal,
					Segment::Fixed("test".into()),
					Segment::Terminal
				],
			})
		);
	}

	#[test]
	fn pattern_complex_with_wildcards() {
		assert_eq!(
			line().parse("/foo/**/b*z").into_result(),
			Ok(Line::Pattern {
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

	#[test]
	fn pattern_negated() {
		assert_eq!(
			line().parse("!/foo/**/b*z").into_result(),
			Ok(Line::Pattern {
				negated: true,
				segments: vec![
					Segment::Terminal,
					Segment::Fixed("foo".into()),
					Segment::All,
					Segment::Wildcard("b*z".into())
				],
			})
		);
	}

	#[test]
	fn pattern_escaped_exclamation() {
		assert_eq!(
			line().parse(r"\!/foo/**/b*z").into_result(),
			Ok(Line::Pattern {
				negated: false,
				segments: vec![
					Segment::Fixed(r"\!".into()),
					Segment::Fixed("foo".into()),
					Segment::All,
					Segment::Wildcard("b*z".into())
				],
			})
		);
	}

	#[test]
	fn comment_empty() {
		assert_eq!(
			line().parse(r"#").into_result(),
			Ok(Line::Comment("".into()))
		);
	}

	#[test]
	fn comment_no_space() {
		assert_eq!(
			line().parse(r"#foo").into_result(),
			Ok(Line::Comment("foo".into()))
		);
	}

	#[test]
	fn comment_with_space() {
		assert_eq!(
			line().parse(r"# foo").into_result(),
			Ok(Line::Comment(" foo".into()))
		);
	}
}
