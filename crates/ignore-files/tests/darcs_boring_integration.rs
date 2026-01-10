use chumsky::prelude::*;
use ignore_files::parse::darcs::*;

#[test]
fn darcs_boring_integration_test() {
	// The actual .boring file content from https://hub.darcs.net/ki11men0w/darcs-screened/browse/.boring
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
# fortran module files
\.mod$
# linux kernel
\.ko\.cmd$
\.mod\.c$
(^|/)\.tmp_versions($|/)
# *.ko files aren't boring by default because they might
# be Korean translations rather than kernel modules
# \.ko$
# python, emacs, java byte code
\.py[co]$
\.elc$
\.class$
# objects and libraries; lo and la are libtool things
\.(obj|a|exe|so|lo|la)$
# compiled zsh configuration files
\.zwc$
# Common LISP output files for CLISP and CMUCL
\.(fas|fasl|sparcf|x86f)$

### build and packaging systems
# cabal intermediates
\.installed-pkg-config
\.setup-config
# standard cabal build dir, might not be boring for everybody
# ^dist(/|$)
# autotools
(^|/)autom4te\.cache($|/)
(^|/)config\.(log|status)$
# microsoft web expression, visual studio metadata directories
\_vti_cnf$
\_vti_pvt$
# gentoo tools
\.revdep-rebuild.*
# generated dependencies
^\.depend$

### version control systems
# cvs
(^|/)CVS($|/)
\.cvsignore$
# cvs, emacs locks
^\.#
# rcs
(^|/)RCS($|/)
,v$
# subversion
(^|/)\.svn($|/)
# mercurial
(^|/)\.hg($|/)
# git
(^|/)\.git($|/)
# bzr
\.bzr$
# sccs
(^|/)SCCS($|/)
# darcs
(^|/)_darcs($|/)
(^|/)\.darcsrepo($|/)
^\.darcs-temp-mail$
-darcs-backup[[:digit:]]+$
# gnu arch
(^|/)(\+|,)
(^|/)vssver\.scc$
\.swp$
(^|/)MT($|/)
(^|/)\{arch\}($|/)
(^|/).arch-ids($|/)
# bitkeeper
(^|/)BitKeeper($|/)
(^|/)ChangeSet($|/)

### miscellaneous
# backup files
~$
\.bak$
\.BAK$
# patch originals and rejects
\.orig$
\.rej$
# X server
\..serverauth.*
# image spam
\#
(^|/)Thumbs\.db$
# vi, emacs tags
(^|/)(tags|TAGS)$
#(^|/)\.[^/]
# core dumps
(^|/|\.)core$
# partial broken files (KIO copy operations)
\.part$
# waf files, see http://code.google.com/p/waf/
(^|/)\.waf-[[:digit:].]+-[[:digit:]]+($|/)
(^|/)\.lock-wscript$
# mac os finder
(^|/)\.DS_Store$
# darcs stuff
^tests-.*$
^_test_playground($|/)
^(cabal-dev|dist)[^/]*/
^.cabal-sandbox($|/)
^cabal.sandbox.config$
^.hpc$
^hpctixdir$
^darcs-temp
^release/distributed-.*
^doc/manual/bigimg[0-9]+\.png$
^doc/manual/bigpage\.(html|css|tex)$
^doc/manual/bigimages\.(tex|log|aux)$
^doc/manual/images\.(tex|log|aux)+$
^doc/manual/img[0-9]+\.png$
^doc/manual/node[0-9]+\.html$
^doc/manual/(index|footnode)\.html$
^doc/manual/darcs_print\.[a-z]+$
^doc/manual/patch-theory\.[a-z]+$
^doc/manual/darcs\.[a-z]+$
^doc/manual/.*\.pl$
^doc/manual/TMP$
^doc/manual/WARNINGS$
^cabal\.project\.local$
^\.ghc\.environment\..*$
(^|/).stack-work($|/)
^stack.yaml.lock$
"#;

	let result = file().parse(boring_content).into_result().unwrap();

	// Count different types of lines
	let mut patterns = 0;
	let mut comments = 0;
	let mut empty_lines = 0;

	for line in &result {
		match line {
			Line::Empty => empty_lines += 1,
			Line::Comment(_) => comments += 1,
			Line::Pattern(_) => patterns += 1,
		}
	}

	// Verify we parsed a reasonable number of each type
	assert!(
		patterns > 50,
		"Should have many regex patterns, got {}",
		patterns
	);
	assert!(comments > 10, "Should have many comments, got {}", comments);
	assert!(
		empty_lines > 0,
		"Should have some empty lines, got {}",
		empty_lines
	);

	// Check specific patterns from the file
	let pattern_strings: Vec<String> = result
		.iter()
		.filter_map(|line| match line {
			Line::Pattern(p) => Some(p.clone()),
			_ => None,
		})
		.collect();

	// Test some specific patterns that should be present
	assert!(pattern_strings.contains(&r"\.hi$".to_string()));
	assert!(pattern_strings.contains(&r"\.o$".to_string()));
	assert!(pattern_strings.contains(&r"\.py[co]$".to_string()));
	assert!(pattern_strings.contains(&r"(^|/)\.git($|/)".to_string()));
	assert!(pattern_strings.contains(&r"(^|/)_darcs($|/)".to_string()));
	assert!(pattern_strings.contains(&r"-darcs-backup[[:digit:]]+$".to_string()));
	assert!(pattern_strings.contains(&r"~$".to_string()));
	assert!(pattern_strings.contains(&r"\.orig$".to_string()));
	assert!(pattern_strings.contains(&r"(^|/)\.DS_Store$".to_string()));

	// Check that comments are properly parsed
	let comment_strings: Vec<String> = result
		.iter()
		.filter_map(|line| match line {
			Line::Comment(c) => Some(c.clone()),
			_ => None,
		})
		.collect();

	assert!(comment_strings.contains(&" Boring file regexps:".to_string()));
	assert!(comment_strings.contains(&" haskell (ghc) interfaces".to_string()));
	assert!(comment_strings.contains(&" python, emacs, java byte code".to_string()));
	assert!(comment_strings.contains(&"## version control systems".to_string()));
}

#[test]
fn darcs_boring_pattern_categories() {
	// Test specific categories of patterns found in darcs boring files
	let test_patterns = vec![
		// Haskell files
		(r"\.hi$", true),
		(r"\.o$", true),
		(r"\.p_hi$", true),
		(r"\.tix$", true),
		// Python files
		(r"\.py[co]$", true),
		// Java files
		(r"\.class$", true),
		// Object files
		(r"\.(obj|a|exe|so|lo|la)$", true),
		// Version control
		(r"(^|/)\.git($|/)", true),
		(r"(^|/)_darcs($|/)", true),
		(r"(^|/)\.svn($|/)", true),
		// Backup files
		(r"~$", true),
		(r"\.bak$", true),
		(r"\.orig$", true),
		// Complex patterns
		(r"-darcs-backup[[:digit:]]+$", true),
		(r"(^|/)\.waf-[[:digit:].]+-[[:digit:]]+($|/)", true),
		// Mac OS
		(r"(^|/)\.DS_Store$", true),
	];

	for (pattern, should_parse) in test_patterns {
		let result = line().parse(pattern).into_result();
		if should_parse {
			assert!(result.is_ok(), "Pattern should parse: {}", pattern);
			if let Ok(Line::Pattern(parsed_pattern)) = result {
				assert_eq!(parsed_pattern, pattern);
			}
		} else {
			assert!(result.is_err(), "Pattern should not parse: {}", pattern);
		}
	}
}

#[test]
fn darcs_boring_comments_and_sections() {
	// Test comment parsing from the darcs boring file
	let test_comments = vec![
		"# Boring file regexps:",
		"# haskell (ghc) interfaces",
		"# object files",
		"# python, emacs, java byte code",
		"# version control systems",
		"# backup files",
		"# *.ko files aren't boring by default because they might",
		"# be Korean translations rather than kernel modules",
		"### compiler and interpreter intermediate files",
		"### build and packaging systems",
		"### version control systems",
		"### miscellaneous",
		"#",
		"# ",
	];

	for comment in test_comments {
		let result = line().parse(comment).into_result();
		assert!(result.is_ok(), "Comment should parse: {}", comment);

		if let Ok(Line::Comment(parsed_comment)) = result {
			let expected = comment.strip_prefix('#').unwrap_or(comment);
			assert_eq!(parsed_comment, expected);
		}
	}
}

#[test]
fn darcs_boring_empty_lines() {
	let empty_variations = vec!["", " ", "  ", "\t", " \t ", "\t  \t"];

	for empty in empty_variations {
		let result = line().parse(empty).into_result();
		assert!(result.is_ok(), "Empty line should parse: {:?}", empty);

		if let Ok(parsed) = result {
			assert_eq!(parsed, Line::Empty);
		}
	}
}

#[test]
fn darcs_boring_whitespace_handling() {
	// Test that patterns with surrounding whitespace are trimmed
	let whitespace_tests = vec![
		("  \\.o$", r"\.o$"),
		("\\.o$  ", r"\.o$"),
		("  \\.o$  ", r"\.o$"),
		("\t\\.py[co]$\t", r"\.py[co]$"),
		(r"   (^|/)\.git($|/)   ", r"(^|/)\.git($|/)"),
	];

	for (input, expected) in whitespace_tests {
		let result = line().parse(input).into_result();
		assert!(result.is_ok(), "Should parse: {}", input);

		if let Ok(Line::Pattern(parsed)) = result {
			assert_eq!(parsed, expected);
		}
	}
}

#[test]
fn darcs_boring_complex_regex_patterns() {
	// Test complex regex patterns that are commonly found in darcs boring files
	let complex_patterns = vec![
		r"(^|/)\.tmp_versions($|/)",
		r"(^|/)autom4te\.cache($|/)",
		r"(^|/)config\.(log|status)$",
		r"\.revdep-rebuild.*",
		r"(^|/)(\+|,)",
		r"\.(fas|fasl|sparcf|x86f)$",
		r"(^|/)\{arch\}($|/)",
		r"(^|/).arch-ids($|/)",
		r"\..serverauth.*",
		r"(^|/)(tags|TAGS)$",
		r"(^|/|\.)core$",
		r"^doc/manual/bigimg[0-9]+\.png$",
		r"^doc/manual/bigpage\.(html|css|tex)$",
		r"^doc/manual/images\.(tex|log|aux)+$",
		r"^\.ghc\.environment\..*$",
	];

	for pattern in complex_patterns {
		let result = line().parse(pattern).into_result();
		assert!(result.is_ok(), "Complex pattern should parse: {}", pattern);

		if let Ok(Line::Pattern(parsed)) = result {
			assert_eq!(parsed, pattern);
		}
	}
}

#[test]
fn darcs_boring_file_structure() {
	// Test parsing a complete boring file structure
	let boring_structure = r#"# Boring file regexps:

### compiler files
\.hi$
\.o$

# python files
\.py[co]$

### version control
(^|/)\.git($|/)
(^|/)_darcs($|/)

# backup files
~$
\.bak$
"#;

	let result = file().parse(boring_structure).into_result().unwrap();

	// Should have the right structure
	assert_eq!(
		result[0],
		Line::Comment(" Boring file regexps:".to_string())
	);
	assert_eq!(result[1], Line::Empty);
	assert_eq!(result[2], Line::Comment("## compiler files".to_string()));
	assert_eq!(result[3], Line::Pattern(r"\.hi$".to_string()));
	assert_eq!(result[4], Line::Pattern(r"\.o$".to_string()));
	assert_eq!(result[5], Line::Empty);
	assert_eq!(result[6], Line::Comment(" python files".to_string()));
	assert_eq!(result[7], Line::Pattern(r"\.py[co]$".to_string()));
}

#[test]
fn darcs_boring_edge_cases() {
	// Test edge cases and special characters
	let edge_cases = vec![
		// Hash character in regex (not a comment)
		r"\#",
		// Escaped characters
		r"\.\\test",
		// Multiple dots
		r"\.\.\.config",
		// Brackets and special regex chars
		r"[[:digit:]]+",
		r"[^/]*",
		// Dollar and caret anchors
		r"^test$",
		// Alternation
		r"(test|spec)",
		// Quantifiers
		r"test.*",
		r"test+",
		r"test?",
		r"test{1,3}",
	];

	for pattern in edge_cases {
		let result = line().parse(pattern).into_result();
		assert!(result.is_ok(), "Edge case should parse: {}", pattern);

		if let Ok(Line::Pattern(parsed)) = result {
			assert_eq!(parsed, pattern);
		}
	}
}
