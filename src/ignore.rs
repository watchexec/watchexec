extern crate globset;

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use std::collections::HashSet;
use std::fs;
use std::io;
use std::io::Read;
use std::path::{Path, PathBuf};

pub struct Ignore {
    files: Vec<IgnoreFile>,
}

#[derive(Debug)]
pub enum Error {
    GlobSet(globset::Error),
    Io(io::Error),
}

struct IgnoreFile {
    set: GlobSet,
    patterns: Vec<Pattern>,
    root: PathBuf,
}

struct Pattern {
    pattern: String,
    pattern_type: PatternType,
    anchored: bool,
}

enum PatternType {
    Ignore,
    Whitelist,
}

#[derive(PartialEq)]
enum MatchResult {
    Ignore,
    Whitelist,
    None,
}

pub fn load(paths: &[PathBuf]) -> Ignore {
    let mut files = vec![];
    let mut checked_dirs = HashSet::new();

    for path in paths {
        let mut p = path.to_owned();

        loop {
            if !checked_dirs.contains(&p) {
                checked_dirs.insert(p.clone());

                let ignore_path = p.join(".ignore");
                if ignore_path.exists() {
                    match IgnoreFile::new(&ignore_path) {
                        Ok(f) => {
                            debug!("Loaded {:?}", ignore_path);
                            files.push(f);
                        }
                        Err(_) => debug!("Unable to load {:?}", ignore_path),
                    }
                }
            }

            if p.parent().is_none() {
                break;
            }

            p.pop();
        }
    }

    Ignore::new(files)
}

impl Ignore {
    fn new(files: Vec<IgnoreFile>) -> Ignore {
        Ignore { files: files }
    }

    pub fn is_excluded(&self, path: &Path) -> bool {
        let mut applicable_files: Vec<&IgnoreFile> = self
            .files
            .iter()
            .filter(|f| path.starts_with(&f.root))
            .collect();
        applicable_files.sort_by(|l, r| l.root_len().cmp(&r.root_len()));

        // TODO: add user ignores

        let mut result = MatchResult::None;

        for file in applicable_files {
            match file.matches(path) {
                MatchResult::Ignore => result = MatchResult::Ignore,
                MatchResult::Whitelist => result = MatchResult::Whitelist,
                MatchResult::None => {}
            }
        }

        result == MatchResult::Ignore
    }
}

impl IgnoreFile {
    pub fn new(path: &Path) -> Result<IgnoreFile, Error> {
        let mut file = fs::File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let lines = contents.lines().collect();
        let root = path.parent().unwrap();

        IgnoreFile::from_strings(lines, root)
    }

    pub fn from_strings(strs: Vec<&str>, root: &Path) -> Result<IgnoreFile, Error> {
        let mut builder = GlobSetBuilder::new();
        let mut patterns = vec![];

        let parsed_patterns = IgnoreFile::parse(strs);
        for p in parsed_patterns {
            let mut pat = String::from(p.pattern.clone());
            if !p.anchored && !pat.starts_with("**/") {
                pat = "**/".to_string() + &pat;
            }

            if !pat.ends_with("/**") {
                pat = pat + "/**";
            }

            let glob = GlobBuilder::new(&pat).literal_separator(true).build()?;

            builder.add(glob);
            patterns.push(p);
        }

        Ok(IgnoreFile {
            set: builder.build()?,
            patterns: patterns,
            root: root.to_owned(),
        })
    }

    #[cfg(test)]
    fn is_excluded(&self, path: &Path) -> bool {
        self.matches(path) == MatchResult::Ignore
    }

    fn matches(&self, path: &Path) -> MatchResult {
        let stripped = path.strip_prefix(&self.root);
        if !stripped.is_ok() {
            return MatchResult::None;
        }

        let matches = self.set.matches(stripped.unwrap());

        for &i in matches.iter().rev() {
            let pattern = &self.patterns[i];
            return match pattern.pattern_type {
                PatternType::Whitelist => MatchResult::Whitelist,
                PatternType::Ignore => MatchResult::Ignore,
            };
        }

        MatchResult::None
    }

    pub fn root_len(&self) -> usize {
        self.root.as_os_str().len()
    }

    fn parse(contents: Vec<&str>) -> Vec<Pattern> {
        contents
            .iter()
            .filter(|l| !l.is_empty())
            .filter(|l| !l.starts_with('#'))
            .map(|l| Pattern::parse(l))
            .collect()
    }
}

impl Pattern {
    fn parse(pattern: &str) -> Pattern {
        let mut normalized = String::from(pattern);

        let pattern_type = if normalized.starts_with('!') {
            normalized.remove(0);
            PatternType::Whitelist
        } else {
            PatternType::Ignore
        };

        let anchored = if normalized.starts_with('/') {
            normalized.remove(0);
            true
        } else {
            false
        };

        if normalized.ends_with('/') {
            normalized.pop();
        }

        if normalized.starts_with("\\#") || normalized.starts_with("\\!") {
            normalized.remove(0);
        }

        Pattern {
            pattern: normalized,
            pattern_type: pattern_type,
            anchored: anchored,
        }
    }
}

impl From<globset::Error> for Error {
    fn from(error: globset::Error) -> Error {
        Error::GlobSet(error)
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Error {
        Error::Io(error)
    }
}

#[cfg(test)]
mod tests {
    use super::IgnoreFile;
    use std::path::PathBuf;

    fn base_dir() -> PathBuf {
        PathBuf::from("/home/user/dir")
    }

    fn build_ignore(pattern: &str) -> IgnoreFile {
        IgnoreFile::from_strings(vec![pattern], &base_dir()).unwrap()
    }

    #[test]
    fn test_matches_exact() {
        let file = build_ignore("Cargo.toml");

        assert!(file.is_excluded(&base_dir().join("Cargo.toml")));
    }

    #[test]
    fn test_does_not_match() {
        let file = build_ignore("Cargo.toml");

        assert!(!file.is_excluded(&base_dir().join("src").join("main.rs")));
    }

    #[test]
    fn test_matches_simple_wildcard() {
        let file = build_ignore("targ*");

        assert!(file.is_excluded(&base_dir().join("target")));
    }

    #[test]
    fn test_matches_subdir_exact() {
        let file = build_ignore("target");

        assert!(file.is_excluded(&base_dir().join("target/")));
    }

    #[test]
    fn test_matches_subdir() {
        let file = build_ignore("target");

        assert!(file.is_excluded(&base_dir().join("target").join("file")));
        assert!(file.is_excluded(&base_dir().join("target").join("subdir").join("file")));
    }

    #[test]
    fn test_wildcard_with_dir() {
        let file = build_ignore("target/f*");

        assert!(file.is_excluded(&base_dir().join("target").join("file")));
        assert!(!file.is_excluded(&base_dir().join("target").join("subdir").join("file")));
    }

    #[test]
    fn test_leading_slash() {
        let file = build_ignore("/*.c");

        assert!(file.is_excluded(&base_dir().join("cat-file.c")));
        assert!(!file.is_excluded(&base_dir().join("mozilla-sha1").join("sha1.c")));
    }

    #[test]
    fn test_leading_double_wildcard() {
        let file = build_ignore("**/foo");

        assert!(file.is_excluded(&base_dir().join("foo")));
        assert!(file.is_excluded(&base_dir().join("target").join("foo")));
        assert!(file.is_excluded(&base_dir().join("target").join("subdir").join("foo")));
    }

    #[test]
    fn test_trailing_double_wildcard() {
        let file = build_ignore("abc/**");

        assert!(!file.is_excluded(&base_dir().join("def").join("foo")));
        assert!(file.is_excluded(&base_dir().join("abc").join("foo")));
        assert!(file.is_excluded(&base_dir().join("abc").join("subdir").join("foo")));
    }

    #[test]
    fn test_sandwiched_double_wildcard() {
        let file = build_ignore("a/**/b");

        assert!(file.is_excluded(&base_dir().join("a").join("b")));
        assert!(file.is_excluded(&base_dir().join("a").join("x").join("b")));
        assert!(file.is_excluded(&base_dir().join("a").join("x").join("y").join("b")));
    }

    #[test]
    fn test_empty_file_never_excludes() {
        let file = IgnoreFile::from_strings(vec![], &base_dir()).unwrap();

        assert!(!file.is_excluded(&base_dir().join("target")));
    }

    #[test]
    fn test_checks_all_patterns() {
        let patterns = vec!["target", "target2"];
        let file = IgnoreFile::from_strings(patterns, &base_dir()).unwrap();

        assert!(file.is_excluded(&base_dir().join("target").join("foo.txt")));
        assert!(file.is_excluded(&base_dir().join("target2").join("bar.txt")));
    }

    #[test]
    fn test_handles_whitelisting() {
        let patterns = vec!["target", "!target/foo.txt"];
        let file = IgnoreFile::from_strings(patterns, &base_dir()).unwrap();

        assert!(!file.is_excluded(&base_dir().join("target").join("foo.txt")));
        assert!(file.is_excluded(&base_dir().join("target").join("blah.txt")));
    }
}
