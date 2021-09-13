use std::{collections::HashSet, path::PathBuf};

use regex::Regex;

use crate::event::Tag;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
	pub in_path: Option<PathBuf>,
	pub on: Tag,
	pub op: Op,
	pub pat: Pattern,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
	Equal,
	NotEqual,
	Regex,
	Glob,
	Includes,
	Excludes,
	InSet,
	OutSet,
}

#[derive(Debug, Clone)]
pub enum Pattern {
	Exact(String),
	Regex(Regex),
	Set(HashSet<String>),
}

impl PartialEq<Self> for Pattern {
	fn eq(&self, other: &Self) -> bool {
		match (self, other) {
			(Self::Exact(l), Self::Exact(r)) => l == r,
			(Self::Regex(l), Self::Regex(r)) => l.as_str() == r.as_str(),
			(Self::Set(l), Self::Set(r)) => l == r,
			_ => false,
		}
	}
}

impl Eq for Pattern {}
