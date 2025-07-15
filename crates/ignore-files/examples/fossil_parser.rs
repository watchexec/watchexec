//! This is used for debugging the fossil ignore-glob file parser.

use chumsky::prelude::*;
use ignore_files::parse::fossil::{file, line, Line, Pattern, Segment, WildcardToken};

fn main() {
	// Example .fossil-settings/ignore-glob file content
	let fossil_content = r#"# Fossil ignore patterns

# Build artifacts
*.o
*.so
*.a

# Multiple patterns on one line
*.log, *.err, *.out

# Quoted patterns with spaces
"build output.txt"
'temp file.dat'

# Character classes
*.[ch]
*.[0-9]
test[a-z].log

# Negated character classes
*.[^ch]
file[^0-9].txt

# Complex patterns
src/*.[ch]
doc/*.txt
build/*/temp.*

# Wildcard patterns
test?.log
temp???.dat
backup*.tar.gz

# Mixed separators
*.c *.h, *.cpp	*.hpp

# Version control directories
.git/
.hg/
.svn/
"#;

	println!("Parsing Fossil ignore file:");
	println!("===========================");

	match file().parse(fossil_content).into_result() {
		Ok(lines) => {
			for (i, line) in lines.iter().enumerate() {
				println!("{:2}: {}", i + 1, format_line(line));
			}

			println!("\nSummary:");
			println!("--------");
			let mut pattern_lines = 0;
			let mut total_patterns = 0;
			let mut comments = 0;
			let mut empty_lines = 0;

			for line in &lines {
				match line {
					Line::Empty => empty_lines += 1,
					Line::Comment(_) => comments += 1,
					Line::Patterns(patterns) => {
						pattern_lines += 1;
						total_patterns += patterns.len();
					}
				}
			}

			println!("  Total lines: {}", lines.len());
			println!("  Empty lines: {}", empty_lines);
			println!("  Comments: {}", comments);
			println!("  Pattern lines: {}", pattern_lines);
			println!("  Total patterns: {}", total_patterns);
		}
		Err(errors) => {
			eprintln!("Parse errors:");
			for error in errors {
				eprintln!("  {}", error);
			}
		}
	}

	println!("\n\nTesting individual patterns:");
	println!("============================");

	let test_patterns = vec![
		"*.o",
		"*.log, *.err",
		"\"foo bar.txt\"",
		"'temp file.dat'",
		"*.[ch]",
		"*.[^ch]",
		"test?.log",
		"src/*.[ch]",
		"*.c *.h, *.cpp	*.hpp",
		"# This is a comment",
		"",
		"   ",
	];

	for pattern in test_patterns {
		match line().parse(pattern).into_result() {
			Ok(parsed) => {
				println!("'{}' -> {}", pattern, format_line(&parsed));
			}
			Err(errors) => {
				println!("'{}' -> ERROR: {:?}", pattern, errors);
			}
		}
	}

	println!("\n\nPattern breakdown examples:");
	println!("==========================");

	let breakdown_examples = vec![
		"*.txt",
		"test?.log",
		"*.[ch]",
		"src/*.[ch]",
		"backup*.tar.gz",
	];

	for pattern in breakdown_examples {
		match line().parse(pattern).into_result() {
			Ok(Line::Patterns(patterns)) if patterns.len() == 1 => {
				println!("\nPattern: '{}'", pattern);
				for (i, segment) in patterns[0].segments.iter().enumerate() {
					match segment {
						Segment::Fixed(s) => println!("  Segment {}: Fixed(\"{}\")", i, s),
						Segment::Wildcard(tokens) => {
							println!("  Segment {}: Wildcard([", i);
							for (j, token) in tokens.iter().enumerate() {
								println!("    {}: {}", j, format_token(token));
							}
							println!("  ])");
						}
					}
				}
			}
			_ => println!("Pattern '{}' -> Complex or error", pattern),
		}
	}

	println!("\n\nFossil glob pattern features:");
	println!("=============================");

	let feature_examples = vec![
		("Basic wildcards", vec!["*", "?", "*.txt", "test?.log"]),
		(
			"Character classes",
			vec!["*.[ch]", "*.[0-9]", "test[a-z].log"],
		),
		(
			"Negated character classes",
			vec!["*.[^ch]", "file[^0-9].txt"],
		),
		("Character ranges", vec!["*[a-z].txt", "file[A-Z][0-9].dat"]),
		(
			"Path patterns",
			vec!["src/*.c", "doc/*.txt", "build/*/temp.*"],
		),
		(
			"Quoted patterns",
			vec!["\"file with spaces.txt\"", "'another file.dat'"],
		),
		("Multiple patterns", vec!["*.c *.h", "*.log, *.err, *.out"]),
	];

	for (category, patterns) in feature_examples {
		println!("\n{}:", category);
		for pattern in patterns {
			println!("  {}", pattern);
		}
	}
}

fn format_line(line: &Line) -> String {
	match line {
		Line::Empty => "Empty line".to_string(),
		Line::Comment(content) => format!("Comment: '{}'", content),
		Line::Patterns(patterns) => {
			if patterns.len() == 1 {
				format!("Pattern: {}", format_pattern(&patterns[0]))
			} else {
				format!(
					"Patterns: [{}]",
					patterns
						.iter()
						.map(|p| format_pattern(p))
						.collect::<Vec<_>>()
						.join(", ")
				)
			}
		}
	}
}

fn format_pattern(pattern: &Pattern) -> String {
	if pattern.segments.len() == 1 {
		match &pattern.segments[0] {
			Segment::Fixed(s) => format!("'{}'", s),
			Segment::Wildcard(tokens) => {
				format!(
					"glob({})",
					tokens.iter().map(format_token).collect::<Vec<_>>().join("")
				)
			}
		}
	} else {
		format!(
			"[{}]",
			pattern
				.segments
				.iter()
				.map(|s| match s {
					Segment::Fixed(s) => format!("'{}'", s),
					Segment::Wildcard(tokens) => {
						format!(
							"glob({})",
							tokens.iter().map(format_token).collect::<Vec<_>>().join("")
						)
					}
				})
				.collect::<Vec<_>>()
				.join(", ")
		)
	}
}

fn format_token(token: &WildcardToken) -> String {
	match token {
		WildcardToken::Any => "*".to_string(),
		WildcardToken::One => "?".to_string(),
		WildcardToken::Class(class) => {
			if class.negated {
				format!("[^{}]", format_char_classes(&class.classes))
			} else {
				format!("[{}]", format_char_classes(&class.classes))
			}
		}
		WildcardToken::Literal(s) => s.clone(),
	}
}

fn format_char_classes(classes: &[ignore_files::parse::charclass::CharClass]) -> String {
	use ignore_files::parse::charclass::CharClass;
	classes
		.iter()
		.map(|c| match c {
			CharClass::Single(ch) => ch.to_string(),
			CharClass::Range(start, end) => format!("{}-{}", start, end),
			CharClass::Named(name) => format!("[:{}:]", name),
			CharClass::Collating(name) => format!("[.{}.]", name),
			CharClass::Equivalence(ch) => format!("[={}=]", ch),
		})
		.collect::<Vec<_>>()
		.join("")
}
