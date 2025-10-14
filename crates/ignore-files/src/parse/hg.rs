use std::path::PathBuf;

pub mod glob;
pub mod line;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Directive {
	Glob {
		pattern: String,
		anchor: Option<Anchor>,
	},
	Regex {
		pattern: String,
		anchor: Option<Anchor>,
	},
	Path {
		path: PathBuf,
		anchor: Anchor,
		file: bool,
		non_recursive: bool,
	},
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Anchor {
	Relative,
	Root,
}
