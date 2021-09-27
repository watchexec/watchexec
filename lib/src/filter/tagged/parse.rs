use std::str::FromStr;

use globset::Glob;
use nom::{Finish, IResult, branch::alt, bytes::complete::{is_not, tag, tag_no_case, take_while1}, character::complete::char, combinator::{map_res, opt}, sequence::{delimited, tuple}};
use regex::Regex;

use super::*;
use crate::error::RuntimeError;

impl FromStr for Filter {
	type Err = RuntimeError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		fn matcher(i: &str) -> IResult<&str, Matcher> {
			map_res(
				alt((
					tag_no_case("tag"),
					tag_no_case("path"),
					tag_no_case("kind"),
					tag_no_case("source"),
					tag_no_case("src"),
					tag_no_case("process"),
					tag_no_case("signal"),
					tag_no_case("exit"),
				)),
				|m: &str| match m.to_ascii_lowercase().as_str() {
					"tag" => Ok(Matcher::Tag),
					"path" => Ok(Matcher::Path),
					"kind" => Ok(Matcher::FileEventKind),
					"source" => Ok(Matcher::Source),
					"src" => Ok(Matcher::Source),
					"process" => Ok(Matcher::Process),
					"signal" => Ok(Matcher::Signal),
					"exit" => Ok(Matcher::ProcessCompletion),
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
					tag("*="),
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
							(Op::Auto | Op::Glob, Matcher::Path) => {
								Pattern::Glob(Glob::new(p).map_err(drop)?.compile_matcher())
							}
							(Op::Equal | Op::NotEqual, _) => Pattern::Exact(p.to_string()),
							(Op::Glob | Op::NotGlob, _) => Pattern::Glob(Glob::new(p).map_err(drop)?.compile_matcher()),
							(Op::Regex | Op::NotRegex, _) => Pattern::Regex(Regex::new(p).map_err(drop)?),
							(Op::Auto | Op::InSet | Op::NotInSet, _) => {
								Pattern::Set(p.split(',').map(|s| s.trim().to_string()).collect())
							}
						},
						negate: n.is_some(),
					})
				},
			)(i)
		}

		filter(s)
			.finish()
			.map(|(_, f)| f)
			.map_err(|e| RuntimeError::FilterParse {
				src: s.to_string(),
				err: e.code,
			})
	}
}
