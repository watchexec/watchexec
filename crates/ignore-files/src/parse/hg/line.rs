use chumsky::prelude::*;

use crate::parse::common::ParserErr;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Line {
	prefix: Option<Prefix>,
	pattern: Option<String>,
	comment: Option<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Prefix {
	Re,
	Path,
	FilePath,
	RelPath,
	RootFilesIn,
	RelGlob,
	RelRe,
	Glob,
	RootGlob,
	ListFile,
	ListFileNulls,
	Include,
	SubInclude,
	Syntax,
}

/*
 * The difference between listfile and include is that listfile cannot include comments.
 * There's one other difference in the handling when cwd is a concern, but that's not relevant for us.
 * We also can't simply say that listfile == include, because # is a valid character in a pattern
 * and therefore listfile needs different handling where # is not interpreted specially. Because
 * of course. Mercurial is a miserable pile of techdebt where nothing is fucking simple.
 */

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PatternSyntax {
	Glob,
	RootGlob,
	Regexp,
}

pub fn prefix<'src>() -> impl Parser<'src, &'src str, Prefix, ParserErr<'src>> {
	choice((
		just("re").to(Prefix::Re),
		just("path").to(Prefix::Path),
		just("filepath").to(Prefix::FilePath),
		just("relpath").to(Prefix::RelPath),
		just("rootfilesin").to(Prefix::RootFilesIn),
		just("relglob").to(Prefix::RelGlob),
		just("relre").to(Prefix::RelRe),
		just("glob").to(Prefix::Glob),
		just("rootglob").to(Prefix::RootGlob),
		just("include").to(Prefix::Include),
		just("subinclude").to(Prefix::SubInclude),
		just("syntax").to(Prefix::Syntax),
	))
	.then_ignore(just(':'))
}

pub fn pattern<'src>() -> impl Parser<'src, &'src str, String, ParserErr<'src>> {
	let bulk = none_of(r"\#").repeated().at_least(1).collect::<String>();
	choice((just(r"\#").to(String::from("#")), bulk))
		.repeated()
		.collect::<Vec<String>>()
		.map(|strs| strs.join(""))
}

pub fn line<'src>() -> impl Parser<'src, &'src str, Line, ParserErr<'src>> {
	prefix()
		.or_not()
		.then(
			pattern()
				.or_not()
				.map(|opt| opt.and_then(|pat| if pat.is_empty() { None } else { Some(pat) })),
		)
		.then(
			just('#')
				.ignore_then(none_of('\n').repeated().at_least(1).collect::<String>())
				.or_not(),
		)
		.map(|((prefix, pattern), comment)| Line {
			prefix,
			pattern,
			comment,
		})
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn all_components() {
		let input = "re:pattern # comment";
		assert_eq!(
			line().parse(input).into_output_errors(),
			(
				Some(Line {
					prefix: Some(Prefix::Re),
					pattern: Some("pattern ".into()),
					comment: Some(" comment".into()),
				}),
				Vec::new()
			)
		);
	}

	#[test]
	fn just_comment() {
		let input = "# comment";
		assert_eq!(
			line().parse(input).into_output_errors(),
			(
				Some(Line {
					prefix: None,
					pattern: None,
					comment: Some(" comment".into()),
				}),
				Vec::new()
			)
		);
	}

	#[test]
	fn double_comment() {
		let input = "# comment # comment";
		assert_eq!(
			line().parse(input).into_output_errors(),
			(
				Some(Line {
					prefix: None,
					pattern: None,
					comment: Some(" comment # comment".into()),
				}),
				Vec::new()
			)
		);
	}

	#[test]
	fn just_pattern() {
		let input = "pattern";
		assert_eq!(
			line().parse(input).into_output_errors(),
			(
				Some(Line {
					prefix: None,
					pattern: Some("pattern".into()),
					comment: None,
				}),
				Vec::new()
			)
		);
	}

	#[test]
	fn prefixed_pattern() {
		let input = "glob:pattern";
		assert_eq!(
			line().parse(input).into_output_errors(),
			(
				Some(Line {
					prefix: Some(Prefix::Glob),
					pattern: Some("pattern".into()),
					comment: None,
				}),
				Vec::new()
			)
		);
	}

	#[test]
	fn pattern_with_escaped_hash() {
		let input = r"\#*\#";
		assert_eq!(
			line().parse(input).into_output_errors(),
			(
				Some(Line {
					prefix: None,
					pattern: Some("#*#".into()),
					comment: None,
				}),
				Vec::new()
			)
		);
	}

	#[test]
	fn pattern_with_escaped_hash_and_comment() {
		let input = r"bar\#foo # comment";
		assert_eq!(
			line().parse(input).into_output_errors(),
			(
				Some(Line {
					prefix: None,
					pattern: Some("bar#foo ".into()),
					comment: Some(" comment".into()),
				}),
				Vec::new()
			)
		);
	}

	#[test]
	fn prefix_three_times() {
		let input = r"re:path:glob:pattern";
		assert_eq!(
			line().parse(input).into_output_errors(),
			(
				Some(Line {
					prefix: Some(Prefix::Re),
					pattern: Some("path:glob:pattern".into()),
					comment: None,
				}),
				Vec::new()
			)
		);
	}
}
