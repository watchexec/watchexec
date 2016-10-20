extern crate glob;

use std::fs;
use std::io;
use std::io::Read;
use std::path::{Path, PathBuf};

// Immutable, ordered set of Patterns
// Used to implement whitelisting
pub struct PatternSet {
    patterns: Vec<Pattern>
}

// Represents a single gitignore rule
//
// Currently we ignore rules about whether to match
// only a directory since it's a bit weird for what
// we want to use a gitignore file for.
struct Pattern {
    pattern: glob::Pattern,
    str: String,
    root: PathBuf,
    whitelist: bool,
    #[allow(dead_code)]
    directory: bool,
    anchored: bool
}

#[derive(Debug)]
pub enum Error {
    Glob(glob::PatternError),
    Io(io::Error),
}

pub fn parse(paths: &[PathBuf]) -> Result<PatternSet, Error> {
    let mut all_patterns = Vec::new();

    for path in paths {
        let file_patterns = try!(parse_file(path));

        for pattern in file_patterns {
            all_patterns.push(pattern);
        }
    }

    Ok(PatternSet::new(all_patterns))
}

fn parse_file(path: &Path) -> Result<Vec<Pattern>, Error> {
    let mut file = try!(fs::File::open(path));
    let mut contents = String::new();
    try!(file.read_to_string(&mut contents));

    // If we've opened the file, we'll have at least one other path component
    let root = path.parent().unwrap();
    let patterns = try!(contents
        .lines()
        .filter(|l| !l.is_empty())
        .filter(|l| !l.starts_with("#"))
        .map(|l| Pattern::new(l, root))
        .collect());

    Ok(patterns)
}

impl PatternSet {
    fn new(patterns: Vec<Pattern>) -> PatternSet {
        PatternSet {
            patterns: patterns
        }
    }

    // Apply the patterns to the path one-by-one
    //
    // If there are whitelisting, we need to run through the whole set.
    // Otherwise, we can stop at the first exclusion.
    pub fn is_excluded(&self, path: &Path) -> bool {
        let mut excluded = false;
        let has_whitelistings = self.patterns.iter().any(|p| p.whitelist);

        for pattern in &self.patterns {
            let matched = pattern.matches(path);

            if matched {
                if pattern.whitelist {
                    excluded = false;
                }
                else {
                    excluded = true;

                    // We can stop running rules in this case
                    if !has_whitelistings {
                        break;
                    }
                }
            }
        }

        excluded
    }
}

impl Pattern {
    fn new(pattern: &str, root: &Path) -> Result<Pattern, Error> {
        let mut normalized = String::from(pattern);

        let whitelisted = if normalized.starts_with('!') {
            normalized.remove(0);
            true
        } else { false };

        let anchored = if normalized.starts_with('/') {
            normalized.remove(0);
            true
        } else { false };

        let directory = if normalized.ends_with('/') {
            normalized.pop();
            true
        } else { false };

        if normalized.starts_with("\\#") || normalized.starts_with("\\!") {
            normalized.remove(0);
        }

        let pat = try!(glob::Pattern::new(&normalized));

        Ok(Pattern {
            pattern: pat,
            str: String::from(normalized),
            root: root.to_path_buf(),
            whitelist: whitelisted,
            directory: directory,
            anchored: anchored
        })
    }

    fn matches(&self, path: &Path) -> bool {
        let options = glob::MatchOptions {
            case_sensitive: false,
            require_literal_separator: true,
            require_literal_leading_dot: false
        };

        let stripped_path = match path.strip_prefix(&self.root) {
            Ok(p)   => p,
            Err(_)  => return false
        };

        let mut result = false;

        if self.anchored {
            let first_component = stripped_path.iter().next();
            result = match first_component {
                Some(s)     => self.pattern.matches_path_with(Path::new(&s), &options),
                None        => false
            }
        }
        else if !self.str.contains('/') {
            result = stripped_path.iter().any(|c| {
                self.pattern.matches_path_with(Path::new(c), &options)
            });
        }
        else if self.pattern.matches_path_with(stripped_path, &options) {
            result = true;
        }

        result
    }
}

impl From<glob::PatternError> for Error {
    fn from(error: glob::PatternError) -> Error {
        Error::Glob(error)
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Error {
        Error::Io(error)
    }
}

#[cfg(test)]
mod tests {
    use super::Pattern;
    use std::path::PathBuf;

    fn base_dir() -> PathBuf {
        PathBuf::from("/home/user/dir")
    }

    fn build_pattern(pattern: &str) -> Pattern {
        Pattern::new(pattern, &base_dir()).unwrap()
    }

    #[test]
    fn test_matches_exact() {
        let pattern = build_pattern("Cargo.toml");

        assert!(pattern.matches(&base_dir().join("Cargo.toml")));
    }

    #[test]
    fn test_matches_simple_wildcard() {
        let pattern = build_pattern("targ*");

        assert!(pattern.matches(&base_dir().join("target")));
    }

    #[test]
    fn test_does_not_match() {
        let pattern = build_pattern("Cargo.toml");

        assert!(!pattern.matches(&base_dir().join("src").join("main.rs")));
    }

    #[test]
    fn test_matches_subdir() {
        let pattern = build_pattern("target");

        assert!(pattern.matches(&base_dir().join("target").join("file")));
        assert!(pattern.matches(&base_dir().join("target").join("subdir").join("file")));
    }

    #[test]
    fn test_wildcard_with_dir() {
        let pattern = build_pattern("target/f*");

        assert!(pattern.matches(&base_dir().join("target").join("file")));
        assert!(!pattern.matches(&base_dir().join("target").join("subdir").join("file")));
    }

    #[test]
    fn test_leading_slash() {
        let pattern = build_pattern("/*.c");

        assert!(pattern.matches(&base_dir().join("cat-file.c")));
        assert!(!pattern.matches(&base_dir().join("mozilla-sha1").join("sha1.c")));
    }

    #[test]
    fn test_leading_double_wildcard() {
        let pattern = build_pattern("**/foo");

        assert!(pattern.matches(&base_dir().join("foo")));
        assert!(pattern.matches(&base_dir().join("target").join("foo")));
        assert!(pattern.matches(&base_dir().join("target").join("subdir").join("foo")));
    }

    #[test]
    fn test_trailing_double_wildcard() {
        let pattern = build_pattern("abc/**");

        assert!(!pattern.matches(&base_dir().join("def").join("foo")));
        assert!(pattern.matches(&base_dir().join("abc").join("foo")));
        assert!(pattern.matches(&base_dir().join("abc").join("subdir").join("foo")));
    }

    #[test]
    fn test_sandwiched_double_wildcard() {
        let pattern = build_pattern("a/**/b");

        assert!(pattern.matches(&base_dir().join("a").join("b")));
        assert!(pattern.matches(&base_dir().join("a").join("x").join("b")));
        assert!(pattern.matches(&base_dir().join("a").join("x").join("y").join("b")));
    }

    use super::PatternSet;

    #[test]
    fn test_empty_pattern_set_never_excludes() {
        let set = PatternSet::new(vec![]);

        assert!(!set.is_excluded(&base_dir().join("target")));
    }

    #[test]
    fn test_set_tests_all_patterns() {
        let patterns = vec![build_pattern("target"), build_pattern("target2")];
        let set = PatternSet::new(patterns);

        assert!(set.is_excluded(&base_dir().join("target").join("foo.txt")));
        assert!(set.is_excluded(&base_dir().join("target2").join("bar.txt")));
    }

    #[test]
    fn test_set_handles_whitelisting() {
        let patterns = vec![build_pattern("target"), build_pattern("!target/foo.txt")];
        let set = PatternSet::new(patterns);

        assert!(!set.is_excluded(&base_dir().join("target").join("foo.txt")));
    }
}

