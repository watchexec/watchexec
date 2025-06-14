use chumsky::prelude::*;
use ignore_files::parse::git::*;

#[test]
fn simple() {
	assert_eq!(
		line().parse("test").into_result(),
		Ok(Line::Pattern {
			negated: false,
			segments: vec![Segment::Fixed("test".into())],
		})
	);
}

#[test]
fn trailing_whitespace() {
	assert_eq!(
		line().parse("test    ").into_result(),
		Ok(Line::Pattern {
			negated: false,
			segments: vec![Segment::Fixed("test".into())],
		})
	);
}

#[test]
fn escaped_trailing_whitespace() {
	assert_eq!(
		line().parse(r"test\    ").into_result(),
		Ok(Line::Pattern {
			negated: false,
			segments: vec![Segment::Fixed(r"test ".into())],
		})
	);
}

#[test]
fn faux_escaped_trailing_whitespace() {
	assert_eq!(
		line().parse(r"foo/te\ st/bar").into_result(),
		Ok(Line::Pattern {
			negated: false,
			segments: vec![
				Segment::Fixed("foo".into()),
				Segment::Fixed(r"te\ st".into()),
				Segment::Fixed("bar".into())
			],
		})
	);
}

#[test]
fn leading_slash() {
	assert_eq!(
		line().parse("/test").into_result(),
		Ok(Line::Pattern {
			negated: false,
			segments: vec![Segment::Terminal, Segment::Fixed("test".into())],
		})
	);
}

#[test]
fn trailing_slash() {
	assert_eq!(
		line().parse("test/").into_result(),
		Ok(Line::Pattern {
			negated: false,
			segments: vec![Segment::Fixed("test".into()), Segment::Terminal],
		})
	);
}

#[test]
fn surrounded_by_slashes() {
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
fn complex_with_wildcards() {
	assert_eq!(
		line().parse("/foo/**/b*z").into_result(),
		Ok(Line::Pattern {
			negated: false,
			segments: vec![
				Segment::Terminal,
				Segment::Fixed("foo".into()),
				Segment::All,
				Segment::Wildcard(vec![
					WildcardToken::Literal("b".into()),
					WildcardToken::Any,
					WildcardToken::Literal("z".into()),
				])
			],
		})
	);
}

#[test]
fn negated() {
	assert_eq!(
		line().parse("!/foo/**/b*z").into_result(),
		Ok(Line::Pattern {
			negated: true,
			segments: vec![
				Segment::Terminal,
				Segment::Fixed("foo".into()),
				Segment::All,
				Segment::Wildcard(vec![
					WildcardToken::Literal("b".into()),
					WildcardToken::Any,
					WildcardToken::Literal("z".into()),
				])
			],
		})
	);
}

#[test]
fn escaped_exclamation() {
	assert_eq!(
		line().parse(r"\!/foo/**/b*z").into_result(),
		Ok(Line::Pattern {
			negated: false,
			segments: vec![
				Segment::Fixed(r"!".into()),
				Segment::Fixed("foo".into()),
				Segment::All,
				Segment::Wildcard(vec![
					WildcardToken::Literal("b".into()),
					WildcardToken::Any,
					WildcardToken::Literal("z".into()),
				])
			],
		})
	);
}

#[test]
fn faux_escaped_exclamation() {
	assert_eq!(
		line().parse(r"/fo\!o/**/b*z").into_result(),
		Ok(Line::Pattern {
			negated: false,
			segments: vec![
				Segment::Terminal,
				Segment::Fixed(r"fo\!o".into()),
				Segment::All,
				Segment::Wildcard(vec![
					WildcardToken::Literal("b".into()),
					WildcardToken::Any,
					WildcardToken::Literal("z".into()),
				])
			],
		})
	);
}

#[test]
fn escaped_hash() {
	assert_eq!(
		line().parse(r"\#/foo/**/b*z").into_result(),
		Ok(Line::Pattern {
			negated: false,
			segments: vec![
				Segment::Fixed(r"#".into()),
				Segment::Fixed("foo".into()),
				Segment::All,
				Segment::Wildcard(vec![
					WildcardToken::Literal("b".into()),
					WildcardToken::Any,
					WildcardToken::Literal("z".into()),
				])
			],
		})
	);
}

#[test]
fn faux_escaped_hash() {
	assert_eq!(
		line().parse(r"/fo\#o/**/b*z").into_result(),
		Ok(Line::Pattern {
			negated: false,
			segments: vec![
				Segment::Terminal,
				Segment::Fixed(r"fo\#o".into()),
				Segment::All,
				Segment::Wildcard(vec![
					WildcardToken::Literal("b".into()),
					WildcardToken::Any,
					WildcardToken::Literal("z".into()),
				])
			],
		})
	);
}

#[test]
fn escaped_periods() {
	assert_eq!(
		line().parse(r"\.foo/\.bar*").into_result(),
		Ok(Line::Pattern {
			negated: false,
			segments: vec![
				Segment::Fixed(".foo".into()),
				Segment::Wildcard(vec![
					WildcardToken::Literal(".bar".into()),
					WildcardToken::Any,
				])
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
