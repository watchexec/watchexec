use chumsky::prelude::*;
use ignore_files::parse::git::*;

#[test]
fn simple() {
	assert_eq!(
		file()
			.parse(
				r"
target
/watchexec-*

# log files
watchexec.*.log
"
			)
			.into_output_errors(),
		(
			Some(vec![
				Line::Empty,
				Line::Pattern {
					negated: false,
					segments: vec![Segment::Fixed("target".into())],
				},
				Line::Pattern {
					negated: false,
					segments: vec![
						Segment::Terminal,
						Segment::Wildcard(vec![
							WildcardToken::Literal("watchexec-".into()),
							WildcardToken::Any,
						]),
					],
				},
				Line::Empty,
				Line::Comment(" log files".into()),
				Line::Pattern {
					negated: false,
					segments: vec![Segment::Wildcard(vec![
						WildcardToken::Literal("watchexec.".into()),
						WildcardToken::Any,
						WildcardToken::Literal(".log".into())
					])]
				},
				Line::Empty,
			]),
			Vec::new()
		)
	);
}

#[test]
fn github_corpus_root() {
	use gitignores::GitIgnore;
	for variant in gitignores::Root::list() {
		let ignore = gitignores::Root::get(variant).unwrap();
		assert_eq!(
			file().parse(ignore.contents()).into_output_errors().1,
			Vec::new(),
			"{variant} ({})",
			ignore.file_path(),
		);
	}
}

#[test]
fn github_corpus_global() {
	use gitignores::GitIgnore;
	for variant in gitignores::Global::list() {
		let ignore = gitignores::Global::get(variant).unwrap();
		assert_eq!(
			file().parse(ignore.contents()).into_output_errors().1,
			Vec::new(),
			"{variant} ({})",
			ignore.file_path(),
		);
	}
}

#[test]
fn github_corpus_community() {
	use gitignores::GitIgnore;
	for variant in gitignores::Community::list() {
		let ignore = gitignores::Community::get(variant).unwrap();
		assert_eq!(
			file().parse(ignore.contents()).into_output_errors().1,
			Vec::new(),
			"{variant} ({})",
			ignore.file_path(),
		);
	}
}
