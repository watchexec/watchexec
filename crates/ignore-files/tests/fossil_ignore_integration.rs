use chumsky::prelude::*;
use ignore_files::parse::fossil::*;

#[test]
fn fossil_ignore_integration_test() {
	// Test parsing fossil ignore patterns based on the documentation
	// from https://fossil-scm.org/home/doc/trunk/www/globs.md
	let fossil_content = r#"# Fossil ignore patterns

# Basic glob patterns
*.o
*.tmp
*.bak

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
"file with spaces.txt", normal.txt	another.log

# Empty lines and comments only


# Another section
README
Makefile
"#;

	let result = file().parse(fossil_content).into_result().unwrap();

	// Count different types of lines
	let mut pattern_lines = 0;
	let mut comments = 0;
	let mut empty_lines = 0;
	let mut total_patterns = 0;

	for line in &result {
		match line {
			Line::Empty => empty_lines += 1,
			Line::Comment(_) => comments += 1,
			Line::Patterns(patterns) => {
				pattern_lines += 1;
				total_patterns += patterns.len();
			}
		}
	}

	// Verify we parsed a reasonable number of each type
	assert!(pattern_lines > 10, "Should have many pattern lines");
	assert!(total_patterns > 20, "Should have many total patterns");
	assert!(comments > 5, "Should have some comments");
	assert!(empty_lines > 3, "Should have some empty lines");

	// Check specific patterns are present
	let all_patterns: Vec<String> = result
		.iter()
		.filter_map(|line| match line {
			Line::Patterns(patterns) => Some(
				patterns
					.iter()
					.map(|p| pattern_to_string(p))
					.collect::<Vec<_>>(),
			),
			_ => None,
		})
		.flatten()
		.collect();

	// Test that we can find some expected patterns
	let pattern_strings = all_patterns.join(" ");
	assert!(pattern_strings.contains("*.o"));
	assert!(pattern_strings.contains("*.tmp"));
	assert!(pattern_strings.contains("build output.txt"));
	assert!(pattern_strings.contains("temp file.dat"));

	println!("Successfully parsed {} lines:", result.len());
	println!("  Pattern lines: {}", pattern_lines);
	println!("  Total patterns: {}", total_patterns);
	println!("  Comments: {}", comments);
	println!("  Empty lines: {}", empty_lines);
}

#[test]
fn fossil_ignore_glob_patterns() {
	// Test specific glob patterns from the Fossil documentation
	let test_patterns = vec![
		// Basic patterns
		("*.o", true),
		("*.tmp", true),
		("README", true),
		("Makefile", true),
		// Character classes
		("*.[ch]", true),
		("*.[0-9]", true),
		("test[a-z].log", true),
		("*.[^ch]", true),
		("file[^0-9].txt", true),
		// Wildcards
		("test?.log", true),
		("temp???.dat", true),
		("backup*.tar.gz", true),
		// Path patterns
		("src/*.[ch]", true),
		("doc/*.txt", true),
		("build/*/temp.*", true),
		// Complex patterns
		("*/README", true),
		("*README", true),
		("src/README", true),
	];

	for (pattern, should_parse) in test_patterns {
		let result = line().parse(pattern).into_result();
		if should_parse {
			assert!(result.is_ok(), "Pattern should parse: {}", pattern);
			if let Ok(Line::Patterns(patterns)) = result {
				assert_eq!(patterns.len(), 1);
			}
		} else {
			assert!(result.is_err(), "Pattern should not parse: {}", pattern);
		}
	}
}

#[test]
fn fossil_ignore_quoted_patterns() {
	// Test quoted patterns with spaces and special characters
	let quoted_tests = vec![
		("\"foo bar.txt\"", "foo bar.txt"),
		("'temp file.dat'", "temp file.dat"),
		(
			"\"file with spaces and symbols!@#.log\"",
			"file with spaces and symbols!@#.log",
		),
		(
			"'path/to/file with spaces.txt'",
			"path/to/file with spaces.txt",
		),
	];

	for (input, expected) in quoted_tests {
		let result = line().parse(input).into_result();
		assert!(result.is_ok(), "Quoted pattern should parse: {}", input);

		if let Ok(Line::Patterns(patterns)) = result {
			assert_eq!(patterns.len(), 1);
			match &patterns[0].segments[0] {
				Segment::Fixed(content) => assert_eq!(content, expected),
				_ => panic!("Expected fixed segment for quoted pattern"),
			}
		}
	}
}

#[test]
fn fossil_ignore_multiple_patterns() {
	// Test multiple patterns on one line with different separators
	let multi_pattern_tests = vec![
		("*.c *.h", vec!["*.c", "*.h"]),
		("*.log, *.err, *.out", vec!["*.log", "*.err", "*.out"]),
		("*.c *.h, *.cpp	*.hpp", vec!["*.c", "*.h", "*.cpp", "*.hpp"]),
		("file1.txt file2.txt", vec!["file1.txt", "file2.txt"]),
		(
			"\"file with spaces.txt\", normal.txt	another.log",
			vec!["file with spaces.txt", "normal.txt", "another.log"],
		),
	];

	for (input, expected) in multi_pattern_tests {
		let result = line().parse(input).into_result();
		assert!(result.is_ok(), "Multi-pattern line should parse: {}", input);

		if let Ok(Line::Patterns(patterns)) = result {
			assert_eq!(patterns.len(), expected.len());
			for (i, expected_pattern) in expected.iter().enumerate() {
				match &patterns[i].segments[0] {
					Segment::Fixed(content) => assert_eq!(content, expected_pattern),
					Segment::Wildcard(_) => {
						// For wildcard patterns, we just check that they exist
						// The exact structure depends on the pattern complexity
					}
				}
			}
		}
	}
}

#[test]
fn fossil_ignore_character_classes() {
	// Test character class patterns
	let char_class_tests = vec![
		"*.[ch]",
		"*.[0-9]",
		"test[a-z].log",
		"*.[^ch]",
		"file[^0-9].txt",
		"[a-zA-Z]*.txt",
		"test[abc].log",
	];

	for pattern in char_class_tests {
		let result = line().parse(pattern).into_result();
		assert!(
			result.is_ok(),
			"Character class pattern should parse: {}",
			pattern
		);

		if let Ok(Line::Patterns(patterns)) = result {
			assert_eq!(patterns.len(), 1);
			// Verify that the pattern contains a character class
			let contains_class = patterns[0].segments.iter().any(|segment| match segment {
				Segment::Wildcard(tokens) => tokens
					.iter()
					.any(|token| matches!(token, WildcardToken::Class(_))),
				_ => false,
			});
			assert!(
				contains_class,
				"Pattern should contain character class: {}",
				pattern
			);
		}
	}
}

#[test]
fn fossil_ignore_comments_and_empty_lines() {
	// Test comment and empty line handling
	let comment_tests = vec![
		("# This is a comment", " This is a comment"),
		(
			"# Another comment with symbols !@#$%",
			" Another comment with symbols !@#$%",
		),
		("### Section header", "## Section header"),
		("#", ""),
		("# ", " "),
	];

	for (input, expected) in comment_tests {
		let result = line().parse(input).into_result();
		assert!(result.is_ok(), "Comment should parse: {}", input);

		if let Ok(Line::Comment(content)) = result {
			assert_eq!(content, expected);
		}
	}

	// Test empty lines
	let empty_tests = vec!["", "   ", "\t", " \t ", "\t  \t"];
	for input in empty_tests {
		let result = line().parse(input).into_result();
		assert!(result.is_ok(), "Empty line should parse: {:?}", input);
		assert_eq!(result.unwrap(), Line::Empty);
	}
}

#[test]
fn fossil_ignore_wildcard_patterns() {
	// Test wildcard patterns
	let wildcard_tests = vec![
		("*", vec!["*"]),
		("?", vec!["?"]),
		("*.txt", vec!["*", ".txt"]),
		("test?.log", vec!["test", "?", ".log"]),
		("temp???.dat", vec!["temp", "?", "?", "?", ".dat"]),
		("backup*.tar.gz", vec!["backup", "*", ".tar.gz"]),
		("src/*.[ch]", vec!["src/", "*", ".", "[ch]"]),
	];

	for (input, _expected_tokens) in wildcard_tests {
		let result = line().parse(input).into_result();
		assert!(result.is_ok(), "Wildcard pattern should parse: {}", input);

		if let Ok(Line::Patterns(patterns)) = result {
			assert_eq!(patterns.len(), 1);
			// Verify that the pattern contains wildcards
			let contains_wildcard = patterns[0]
				.segments
				.iter()
				.any(|segment| matches!(segment, Segment::Wildcard(_)));
			// Simple patterns like "README" won't contain wildcards
			if input.contains('*') || input.contains('?') || input.contains('[') {
				assert!(
					contains_wildcard,
					"Pattern should contain wildcards: {}",
					input
				);
			}
		}
	}
}

#[test]
fn fossil_ignore_edge_cases() {
	// Test edge cases and special scenarios
	let edge_cases = vec![
		// Pattern with just wildcards
		("*", true),
		("?", true),
		("*?*", true),
		// Empty character class (should parse)
		("*[]", true),
		// Character class with special chars
		("*[.]", true),
		("*[*]", true),
		("*[?]", true),
		// Mixed quotes (should parse as separate patterns)
		("\"foo\" 'bar'", true),
		// Trailing comma
		("*.txt,", true),
		// Multiple separators
		("*.c , , *.h", true),
	];

	for (input, should_parse) in edge_cases {
		let result = line().parse(input).into_result();
		if should_parse {
			assert!(result.is_ok(), "Edge case should parse: {}", input);
		} else {
			assert!(result.is_err(), "Edge case should not parse: {}", input);
		}
	}
}

#[test]
fn fossil_ignore_file_structure() {
	// Test parsing a complete fossil ignore file structure
	let fossil_structure = r#"# Fossil ignore patterns
# Generated files
*.o
*.tmp

# Documentation
doc/*.html, doc/*.pdf

# IDE files
"Visual Studio Files/"
'Code::Blocks Files/'

# Test files
test?.log
temp[0-9]*.dat

# Version control (nested comments)
# Git files
.git/
# Mercurial files
.hg/
"#;

	let result = file().parse(fossil_structure).into_result().unwrap();

	// Should have the right structure
	assert_eq!(
		result[0],
		Line::Comment(" Fossil ignore patterns".to_string())
	);
	assert_eq!(result[1], Line::Comment(" Generated files".to_string()));

	// Check that we have patterns
	let pattern_lines: Vec<_> = result
		.iter()
		.filter_map(|line| match line {
			Line::Patterns(patterns) => Some(patterns.len()),
			_ => None,
		})
		.collect();

	assert!(
		pattern_lines.len() > 5,
		"Should have multiple pattern lines"
	);
	assert!(
		pattern_lines.iter().sum::<usize>() > 8,
		"Should have many total patterns"
	);
}

fn pattern_to_string(pattern: &Pattern) -> String {
	if pattern.segments.len() == 1 {
		match &pattern.segments[0] {
			Segment::Fixed(s) => s.clone(),
			Segment::Wildcard(tokens) => tokens
				.iter()
				.map(|token| match token {
					WildcardToken::Any => "*".to_string(),
					WildcardToken::One => "?".to_string(),
					WildcardToken::Class(class) => {
						if class.negated {
							format!("[^{}]", class_to_string(&class.classes))
						} else {
							format!("[{}]", class_to_string(&class.classes))
						}
					}
					WildcardToken::Literal(s) => s.clone(),
				})
				.collect::<Vec<_>>()
				.join(""),
		}
	} else {
		// Multiple segments, reconstruct the pattern
		pattern
			.segments
			.iter()
			.map(|segment| match segment {
				Segment::Fixed(s) => s.clone(),
				Segment::Wildcard(tokens) => tokens
					.iter()
					.map(|token| match token {
						WildcardToken::Any => "*".to_string(),
						WildcardToken::One => "?".to_string(),
						WildcardToken::Class(class) => {
							if class.negated {
								format!("[^{}]", class_to_string(&class.classes))
							} else {
								format!("[{}]", class_to_string(&class.classes))
							}
						}
						WildcardToken::Literal(s) => s.clone(),
					})
					.collect::<Vec<_>>()
					.join(""),
			})
			.collect::<Vec<_>>()
			.join("")
	}
}

fn class_to_string(classes: &[ignore_files::parse::charclass::CharClass]) -> String {
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
