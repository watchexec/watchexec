//! Example usage of the Darcs boring file parser.
//!
//! This example demonstrates how to use the darcs parser to parse _darcs/prefs/boring files
//! and understand the regex patterns used by Darcs to ignore files.

use chumsky::prelude::*;
use ignore_files::parse::darcs::{file, line, Line};

fn main() {
	// Example _darcs/prefs/boring file content
	let boring_content = r#"# Boring file regexps:

### compiler and interpreter intermediate files
# haskell (ghc) interfaces
\.hi$
\.hi-boot$
\.o-boot$
# object files
\.o$
\.o\.cmd$
# profiling haskell
\.p_hi$
\.p_o$
# haskell program coverage resp. profiling info
\.tix$
\.prof$
# python, emacs, java byte code
\.py[co]$
\.elc$
\.class$
# objects and libraries; lo and la are libtool things
\.(obj|a|exe|so|lo|la)$

### version control systems
# git
(^|/)\.git($|/)
# mercurial
(^|/)\.hg($|/)
# darcs
(^|/)_darcs($|/)
(^|/)\.darcsrepo($|/)
^\.darcs-temp-mail$
-darcs-backup[[:digit:]]+$

### miscellaneous
# backup files
~$
\.bak$
\.BAK$
# patch originals and rejects
\.orig$
\.rej$
# core dumps
(^|/|\.)core$
# mac os finder
(^|/)\.DS_Store$
"#;

	println!("Parsing Darcs boring file:");
	println!("==========================");

	match file().parse(boring_content).into_result() {
		Ok(lines) => {
			for (i, line) in lines.iter().enumerate() {
				println!("{:2}: {}", i + 1, format_line(line));
			}

			println!("\nSummary:");
			println!("--------");
			let mut regex_patterns = 0;
			let mut comments = 0;
			let mut empty_lines = 0;

			for line in &lines {
				match line {
					Line::Empty => empty_lines += 1,
					Line::Comment(_) => comments += 1,
					Line::Pattern(_) => regex_patterns += 1,
				}
			}

			println!("  Total lines: {}", lines.len());
			println!("  Empty lines: {}", empty_lines);
			println!("  Comments: {}", comments);
			println!("  Regex patterns: {}", regex_patterns);
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
		r"\.hi$",
		r"\.o$",
		r"\.py[co]$",
		r"(^|/)\.git($|/)",
		r"(^|/)_darcs($|/)",
		r"-darcs-backup[[:digit:]]+$",
		r"~$",
		r"\.orig$",
		r"(^|/)\.DS_Store$",
		r"\.(obj|a|exe|so|lo|la)$",
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

	println!("\n\nExample regex patterns by category:");
	println!("===================================");

	let pattern_categories = vec![
		(
			"Haskell files",
			vec![r"\.hi$", r"\.o$", r"\.tix$", r"\.prof$"],
		),
		("Python files", vec![r"\.py[co]$"]),
		("Java files", vec![r"\.class$"]),
		("Object files", vec![r"\.(obj|a|exe|so|lo|la)$"]),
		(
			"Version control",
			vec![r"(^|/)\.git($|/)", r"(^|/)_darcs($|/)", r"(^|/)\.hg($|/)"],
		),
		(
			"Backup files",
			vec![r"~$", r"\.bak$", r"\.orig$", r"\.rej$"],
		),
		(
			"Darcs-specific",
			vec![r"-darcs-backup[[:digit:]]+$", r"^\.darcs-temp-mail$"],
		),
		("System files", vec![r"(^|/)\.DS_Store$", r"(^|/|\.)core$"]),
	];

	for (category, patterns) in pattern_categories {
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
		Line::Pattern(pattern) => format!("Regex: '{}'", pattern),
	}
}
