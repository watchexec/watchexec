use std::str::FromStr;

use nom::{
	branch::alt,
	bytes::complete::{is_not, tag, tag_no_case, take_while1},
	character::complete::char,
	combinator::{map_res, opt},
	sequence::{delimited, tuple},
	Finish, IResult,
};
use regex::Regex;
use tracing::trace;

use super::*;

impl FromStr for Filter {
	type Err = error::TaggedFiltererError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		fn matcher(i: &str) -> IResult<&str, Matcher> {
			map_res(
				alt((
					tag_no_case("tag"),
					tag_no_case("path"),
					tag_no_case("type"),
					tag_no_case("kind"),
					tag_no_case("fek"),
					tag_no_case("source"),
					tag_no_case("src"),
					tag_no_case("process"),
					tag_no_case("pid"),
					tag_no_case("signal"),
					tag_no_case("complete"),
					tag_no_case("exit"),
				)),
				|m: &str| match m.to_ascii_lowercase().as_str() {
					"tag" => Ok(Matcher::Tag),
					"path" => Ok(Matcher::Path),
					"type" => Ok(Matcher::FileType),
					"kind" | "fek" => Ok(Matcher::FileEventKind),
					"source" | "src" => Ok(Matcher::Source),
					"process" | "pid" => Ok(Matcher::Process),
					"signal" => Ok(Matcher::Signal),
					"complete" | "exit" => Ok(Matcher::ProcessCompletion),
					m => Err(format!("unknown matcher: {}", m)),
				},
			)(i)
		}

		fn op(i: &str) -> IResult<&str, Op> {
			map_res(
				alt((
					tag("=="),
					tag("!="),
					tag("~="),
					tag("~!"),
					tag("*="),
					tag("*!"),
					tag(":="),
					tag(":!"),
					tag("="),
				)),
				|o: &str| match o {
					"==" => Ok(Op::Equal),
					"!=" => Ok(Op::NotEqual),
					"~=" => Ok(Op::Regex),
					"~!" => Ok(Op::NotRegex),
					"*=" => Ok(Op::Glob),
					"*!" => Ok(Op::NotGlob),
					":=" => Ok(Op::InSet),
					":!" => Ok(Op::NotInSet),
					"=" => Ok(Op::Auto),
					o => Err(format!("unknown op: `{}`", o)),
				},
			)(i)
		}

		fn pattern(i: &str) -> IResult<&str, &str> {
			alt((
				// TODO: escapes
				delimited(char('"'), is_not("\""), char('"')),
				delimited(char('\''), is_not("'"), char('\'')),
				take_while1(|_| true),
			))(i)
		}

		fn filter(i: &str) -> IResult<&str, Filter> {
			map_res(
				tuple((opt(tag("!")), matcher, op, pattern)),
				|(n, m, o, p)| -> Result<_, ()> {
					Ok(Filter {
						in_path: None,
						on: m,
						op: match o {
							Op::Auto => match m {
								Matcher::Path => Op::Glob,
								_ => Op::InSet,
							},
							o => o,
						},
						pat: match (o, m) {
							// TODO: carry regex/glob errors through
							(Op::Auto | Op::Glob, Matcher::Path) | (Op::Glob | Op::NotGlob, _) => {
								Pattern::Glob(p.to_string())
							}
							(Op::Auto | Op::InSet | Op::NotInSet, _) => {
								Pattern::Set(p.split(',').map(|s| s.trim().to_string()).collect())
							}
							(Op::Regex | Op::NotRegex, _) => {
								Pattern::Regex(Regex::new(p).map_err(drop)?)
							}
							(Op::Equal | Op::NotEqual, _) => Pattern::Exact(p.to_string()),
						},
						negate: n.is_some(),
					})
				},
			)(i)
		}

		trace!(src=?s, "parsing tagged filter");
		filter(s)
			.finish()
			.map(|(_, f)| {
				trace!(src=?s, filter=?f, "parsed tagged filter");
				f
			})
			.map_err(|e| error::TaggedFiltererError::Parse {
				src: s.to_string(),
				err: e.code,
			})
	}
}
