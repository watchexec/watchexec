//! Example usage of the Bazaar ignore file parser.
//!
//! This example demonstrates how to use the bazaar parser to parse .bzrignore files
//! and understand the different types of patterns supported.

use chumsky::prelude::*;
use ignore_files::parse::bzr::{file, line, Line, PatternKind};

fn main() {
	// Example .bzrignore file content
	let bzrignore_content = r#"# Bazaar ignore file for a Rust project
# Build artifacts
*.o
*.so
*.a
target/

# Temporary files
*.tmp
*.bak
!important.bak

# Case insensitive regex for common image files
RE:(?i).*\.(jpg|jpeg|png|gif|bmp)$

# Regex for log files with timestamps
RE:.*\.log\.\d{4}-\d{2}-\d{2}$

# Negated regex to keep certain temp files
!RE:keep_.*\.tmp$

# Root directory only
./config

# Complex glob patterns
src/**/*.class
test??.log
*.[ch]pp
"#;

	println!("Parsing .bzrignore file:");
	println!("======================");

	match file().parse(bzrignore_content).into_result() {
		Ok(lines) => {
			for (i, line) in lines.iter().enumerate() {
				println!("{:2}: {}", i + 1, format_line(line));
			}

			println!("\nSummary:");
			println!("--------");
			let mut glob_patterns = 0;
			let mut regex_patterns = 0;
			let mut negated_patterns = 0;
			let mut comments = 0;
			let mut empty_lines = 0;

			for line in &lines {
				match line {
					Line::Empty => empty_lines += 1,
					Line::Comment(_) => comments += 1,
					Line::Pattern { negated, kind, .. } => {
						if *negated {
							negated_patterns += 1;
						}
						match kind {
							PatternKind::Glob => glob_patterns += 1,
							PatternKind::Regex { .. } => regex_patterns += 1,
						}
					}
				}
			}

			println!("  Total lines: {}", lines.len());
			println!("  Empty lines: {}", empty_lines);
			println!("  Comments: {}", comments);
			println!("  Glob patterns: {}", glob_patterns);
			println!("  Regex patterns: {}", regex_patterns);
			println!("  Negated patterns: {}", negated_patterns);
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
		"*.tmp",
		"!important.log",
		"RE:.*\\.log$",
		"RE:(?i)foo",
		"# This is a comment",
		"src/**/*.rs",
		"./config",
		"build/",
		r"\!special",
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
}

fn format_line(line: &Line) -> String {
	match line {
		Line::Empty => "Empty line".to_string(),
		Line::Comment(content) => format!("Comment: '{}'", content),
		Line::Pattern {
			negated,
			kind,
			pattern,
		} => {
			let negation = if *negated { "!" } else { "" };
			match kind {
				PatternKind::Glob => format!("{}Glob: '{}'", negation, pattern),
				PatternKind::Regex { case_insensitive } => {
					let case_flag = if *case_insensitive {
						" (case-insensitive)"
					} else {
						""
					};
					format!("{}Regex{}: '{}'", negation, case_flag, pattern)
				}
			}
		}
	}
}
