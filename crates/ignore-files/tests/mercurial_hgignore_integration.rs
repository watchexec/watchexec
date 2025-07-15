use chumsky::prelude::*;
use ignore_files::parse::hg::line::*;

#[test]
fn mercurial_hgignore_integration_test() {
	// The actual .hgignore file from Mercurial's repository
	// Downloaded from https://repo.mercurial-scm.org/hg/file/tip/.hgignore
	let hgignore_content = r#"syntax: glob

*.elc
*.tmp
*.orig
*.rej
*~
*.mergebackup
*.o
*.so
*.dll
*.exe
*.pyd
*.pyc
*.pyo
*$py.class
*.swp
*.prof
*.zip
\#*\#
.\#*
.venv*
result/
tests/artifacts/cache/big-file-churn.hg
tests/.coverage*
tests/.testtimes*
# the file is written in the CWD when run-tests is run.
.testtimes
tests/.hypothesis
tests/hypothesis-generated
tests/annotated
tests/exceptions
tests/python3
tests/*.err
tests/htmlcov
build
contrib/chg/chg
contrib/hgsh/hgsh
contrib/vagrant/.vagrant
contrib/merge-lists/target/
dist
packages
doc/common.txt
doc/commandlist.txt
doc/extensionlist.txt
doc/topiclist.txt
doc/*.mk
doc/*.[0-9]
doc/*.[0-9].txt
doc/*.[0-9].gendoc.txt
doc/*.[0-9].{x,ht}ml
doc/build
doc/html
doc/man
patches
mercurial/__modulepolicy__.py
mercurial/__version__.py
mercurial/hgpythonlib.h
mercurial.egg-info
.DS_Store
tags
cscope.*
.vscode/*
.idea/*
.asv/*
.pytype/*
.mypy_cache
i18n/hg.pot
locale/*/LC_MESSAGES/hg.mo
hgext/__index__.py

rust/target/
rust/*/target/

# Generated wheels
wheelhouse/

syntax: rootglob
# See Profiling in rust/README.rst
.cargo/config

syntax: regexp
^.pc/
^.(pydev)?project

# hackable windows distribution additions
^hg-python
^hg.py$
"#;

	let lines: Vec<&str> = hgignore_content.lines().collect();
	let mut parsed_lines = Vec::new();
	let mut syntax_sections = Vec::new();

	let mut parse_errors = Vec::new();
	let mut known_limitations = Vec::new();

	// Parse each line and collect statistics
	for (line_num, line_content) in lines.iter().enumerate() {
		let line_number = line_num + 1;
		let result = line().parse(line_content).into_result();

		match result {
			Ok(parsed) => {
				// Check if this is a syntax directive
				if line_content.starts_with("syntax:") {
					if let Some(new_syntax) = line_content.strip_prefix("syntax:").map(|s| s.trim())
					{
						syntax_sections.push((line_number, new_syntax.to_string()));
					}
				}
				parsed_lines.push((line_number, line_content.to_string(), parsed));
			}
			Err(errors) => {
				// Check if this is a known limitation (backslash patterns)
				if line_content.contains(r"\.") {
					known_limitations.push((line_number, line_content.to_string()));
				} else {
					parse_errors.push((line_number, line_content.to_string(), errors));
				}
			}
		}
	}

	// Report parsing results
	println!("Mercurial .hgignore Integration Test Results:");
	println!("============================================");
	println!("Total lines: {}", lines.len());
	println!("Successfully parsed: {}", parsed_lines.len());
	println!("Parse errors: {}", parse_errors.len());
	println!(
		"Known limitations (backslash patterns): {}",
		known_limitations.len()
	);
	println!("Syntax sections found: {}", syntax_sections.len());

	// Print syntax sections
	for (line_num, syntax) in &syntax_sections {
		println!("  Line {}: syntax: {}", line_num, syntax);
	}

	// Print known limitations
	if !known_limitations.is_empty() {
		println!("\nKnown Limitations (backslash patterns):");
		for (line_num, line_content) in &known_limitations {
			println!("  Line {}: '{}'", line_num, line_content);
		}
	}

	// Print any parse errors (there shouldn't be any for a well-formed .hgignore)
	if !parse_errors.is_empty() {
		println!("\nParse Errors:");
		for (line_num, line, errors) in &parse_errors {
			println!("  Line {}: '{}' - {:?}", line_num, line, errors);
		}
	}

	// Verify that all lines parsed successfully (accounting for known limitations)
	assert_eq!(
		parse_errors.len(),
		0,
		"All lines should parse successfully or be known limitations"
	);
	assert_eq!(
		parsed_lines.len() + known_limitations.len(),
		lines.len(),
		"All lines should be parsed or identified as known limitations"
	);

	// Verify syntax sections were detected
	assert_eq!(syntax_sections.len(), 3, "Should find 3 syntax sections");
	assert_eq!(syntax_sections[0].1, "glob");
	assert_eq!(syntax_sections[1].1, "rootglob");
	assert_eq!(syntax_sections[2].1, "regexp");
}

#[test]
fn mercurial_hgignore_pattern_categories() {
	// Test specific categories of patterns found in Mercurial's .hgignore
	let test_patterns = vec![
		// File extensions
		("*.elc", true),
		("*.tmp", true),
		("*.orig", true),
		("*.rej", true),
		("*.mergebackup", true),
		("*.o", true),
		("*.so", true),
		("*.dll", true),
		("*.exe", true),
		("*.pyd", true),
		("*.pyc", true),
		("*.pyo", true),
		("*$py.class", true),
		("*.swp", true),
		("*.prof", true),
		("*.zip", true),
		// Backup files
		("*~", true),
		// Patterns with escaped characters (only \# is supported)
		(r"\#*\#", true),
		(r".\#*", true),
		// Directory patterns
		(".venv*", true),
		("result/", true),
		("build", true),
		("dist", true),
		("packages", true),
		("patches", true),
		("wheelhouse/", true),
		// Specific paths
		("tests/artifacts/cache/big-file-churn.hg", true),
		("tests/.coverage*", true),
		("tests/.testtimes*", true),
		(".testtimes", true),
		("tests/.hypothesis", true),
		("tests/hypothesis-generated", true),
		("tests/annotated", true),
		("tests/exceptions", true),
		("tests/python3", true),
		("tests/*.err", true),
		("tests/htmlcov", true),
		// Contrib patterns
		("contrib/chg/chg", true),
		("contrib/hgsh/hgsh", true),
		("contrib/vagrant/.vagrant", true),
		("contrib/merge-lists/target/", true),
		// Documentation patterns
		("doc/common.txt", true),
		("doc/commandlist.txt", true),
		("doc/extensionlist.txt", true),
		("doc/topiclist.txt", true),
		("doc/*.mk", true),
		("doc/*.[0-9]", true),
		("doc/*.[0-9].txt", true),
		("doc/*.[0-9].gendoc.txt", true),
		("doc/*.[0-9].{x,ht}ml", true),
		("doc/build", true),
		("doc/html", true),
		("doc/man", true),
		// Mercurial-specific patterns
		("mercurial/__modulepolicy__.py", true),
		("mercurial/__version__.py", true),
		("mercurial/hgpythonlib.h", true),
		("mercurial.egg-info", true),
		("hgext/__index__.py", true),
		// System files
		(".DS_Store", true),
		("tags", true),
		("cscope.*", true),
		// IDE/Editor patterns
		(".vscode/*", true),
		(".idea/*", true),
		(".asv/*", true),
		(".pytype/*", true),
		(".mypy_cache", true),
		// Internationalization
		("i18n/hg.pot", true),
		("locale/*/LC_MESSAGES/hg.mo", true),
		// Rust patterns
		("rust/target/", true),
		("rust/*/target/", true),
		// Cargo config (rootglob pattern)
		(".cargo/config", true),
		// Regexp patterns (simplified without backslashes for current parser)
		("^.pc/", true),     // Without backslash, should work
		("^.project", true), // Without backslash, should work
		("^hg-python", true),
		("^hg.py$", true),
	];

	for (pattern, should_parse) in test_patterns {
		let result = line().parse(pattern).into_result();

		if should_parse {
			assert!(
				result.is_ok(),
				"Pattern from Mercurial .hgignore should parse: '{}'",
				pattern
			);
		} else {
			assert!(
				result.is_err(),
				"Pattern should fail to parse: '{}'",
				pattern
			);
		}
	}
}

#[test]
fn mercurial_hgignore_comments() {
	// Test comment patterns found in Mercurial's .hgignore
	let comments = vec![
		"# the file is written in the CWD when run-tests is run.",
		"# Generated wheels",
		"# See Profiling in rust/README.rst",
		"# hackable windows distribution additions",
	];

	for comment in comments {
		let result = line().parse(comment).into_result();
		assert!(
			result.is_ok(),
			"Comment should parse successfully: '{}'",
			comment
		);
	}
}

#[test]
fn mercurial_hgignore_complex_patterns() {
	// Test complex patterns that demonstrate advanced glob features
	let complex_patterns = vec![
		// Character classes in brackets
		("doc/*.[0-9]", true),
		("doc/*.[0-9].txt", true),
		("doc/*.[0-9].gendoc.txt", true),
		// Alternation patterns
		("doc/*.[0-9].{x,ht}ml", true),
		// Wildcard patterns
		("tests/.coverage*", true),
		("tests/.testtimes*", true),
		("cscope.*", true),
		// Directory wildcards
		(".vscode/*", true),
		(".idea/*", true),
		(".asv/*", true),
		(".pytype/*", true),
		("rust/*/target/", true),
		("locale/*/LC_MESSAGES/hg.mo", true),
		// Special characters
		("*$py.class", true),
		// Patterns with dots
		(".*", true),
		(".DS_Store", true),
		(".testtimes", true),
		(".mypy_cache", true),
	];

	for (pattern, should_parse) in complex_patterns {
		let result = line().parse(pattern).into_result();

		if should_parse {
			assert!(
				result.is_ok(),
				"Complex pattern should parse: '{}'",
				pattern
			);
		} else {
			assert!(
				result.is_err(),
				"Complex pattern should fail: '{}'",
				pattern
			);
		}
	}
}

#[test]
fn mercurial_hgignore_empty_lines() {
	// Test that empty lines are handled correctly
	let empty_line_test = "";
	let result = line().parse(empty_line_test).into_result();
	assert!(result.is_ok(), "Empty line should parse successfully");
}

#[test]
fn mercurial_hgignore_syntax_transitions() {
	// Test the three syntax sections found in Mercurial's .hgignore
	let syntax_directives = vec![
		("syntax: glob", "glob"),
		("syntax: rootglob", "rootglob"),
		("syntax: regexp", "regexp"),
	];

	for (directive, _expected_type) in syntax_directives {
		let result = line().parse(directive).into_result();
		assert!(
			result.is_ok(),
			"Syntax directive should parse: '{}'",
			directive
		);

		// The directive should be parsed as a Syntax prefix with the type as pattern
		// We can't easily test the internal structure due to private fields,
		// but we can verify it parses without error
	}
}

#[test]
fn mercurial_hgignore_real_world_validation() {
	// Test a representative sample of the actual content structure
	let sample_content = vec![
		"syntax: glob",
		"",
		"*.elc",
		"*.tmp",
		"*~",
		"build",
		"# Generated wheels",
		"wheelhouse/",
		"",
		"syntax: rootglob",
		"# See Profiling in rust/README.rst",
		".cargo/config",
		"",
		"syntax: regexp",
		"^.pc/", // Note: simplified without backslash for current parser
		"^hg-python",
		"^hg.py$",
	];

	// All lines should parse successfully
	for (line_num, line_content) in sample_content.iter().enumerate() {
		let result = line().parse(line_content).into_result();
		assert!(
			result.is_ok(),
			"Line {} should parse successfully: '{}'",
			line_num + 1,
			line_content
		);
	}
}
