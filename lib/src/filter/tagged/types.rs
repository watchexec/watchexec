use std::{collections::HashSet, path::PathBuf};

use globset::Glob;
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
	pub in_path: Option<PathBuf>,
	pub on: Matcher,
	pub op: Op,
	pub pat: Pattern,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Matcher {
	Tag,
	Path,
	FileEventKind,
	Source,
	Process,
	Signal,
	ProcessCompletion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Op {
	Auto,     // =
	Equal,    // ==
	NotEqual, // !=
	Regex,    // ~=
	Glob,     // *=
	InSet,    // :=
	NotInSet, // :!
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Pattern {
	Exact(String),
	Regex(Regex),
	Glob(Glob),
	Set(HashSet<String>),
}

impl PartialEq<Self> for Pattern {
	fn eq(&self, other: &Self) -> bool {
		match (self, other) {
			(Self::Exact(l), Self::Exact(r)) => l == r,
			(Self::Regex(l), Self::Regex(r)) => l.as_str() == r.as_str(),
			(Self::Glob(l), Self::Glob(r)) => l == r,
			(Self::Set(l), Self::Set(r)) => l == r,
			_ => false,
		}
	}
}

impl Eq for Pattern {}
