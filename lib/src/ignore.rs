use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use std::collections::HashSet;
use std::fs;
use std::io;
use std::io::Read;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

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

        // walk up to root
        // FIXME: this makes zero sense and should be removed
        // but that would be a breaking change
        loop {
            if !checked_dirs.contains(&p) {
                checked_dirs.insert(p.clone());

                let ignore_path = p.join(".ignore");
                if ignore_path.exists() {
                    if let Ok(f) = IgnoreFile::new(&ignore_path) {
                        debug!("Loaded {:?}", ignore_path);
                        files.push(f);
                    } else {
                        debug!("Unable to load {:?}", ignore_path);
                    }
                }
            }

            if p.parent().is_none() {
                break;
            }

            p.pop();
        }

        //also look in subfolders
        for entry in WalkDir::new(path)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
            .filter(|e| e.file_name() == ".ignore")
        {
            let ignore_path = entry.path();
            if let Ok(f) = IgnoreFile::new(ignore_path) {
                debug!("Loaded {:?}", ignore_path);
                files.push(f);
            } else {
                debug!("Unable to load {:?}", ignore_path);
            }
        }
    }

    Ignore::new(files)
}

impl Ignore {
    const fn new(files: Vec<IgnoreFile>) -> Self {
        Self { files }
    }

    pub fn is_excluded(&self, path: &Path) -> bool {
        let mut applicable_files: Vec<&IgnoreFile> = self
            .files
            .iter()
            .filter(|f| path.starts_with(&f.root))
            .collect();
        applicable_files.sort_by_key(|f| f.root_len());

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
    pub fn new(path: &Path) -> Result<Self, Error> {
        let mut file = fs::File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let lines: Vec<_> = contents.lines().collect();
        let root = path.parent().expect("ignore file is at filesystem root");

        Self::from_strings(&lines, root)
    }

    pub fn from_strings(strs: &[&str], root: &Path) -> Result<Self, Error> {
        let mut builder = GlobSetBuilder::new();
        let mut patterns = vec![];

        let parsed_patterns = Self::parse(strs);
        for p in parsed_patterns {
            let mut pat = p.pattern.clone();
            if !p.anchored && !pat.starts_with("**/") {
                pat = "**/".to_string() + &pat;
            }

            if !pat.ends_with("/**") {
                pat += "/**";
            }

            let glob = GlobBuilder::new(&pat).literal_separator(true).build()?;

            builder.add(glob);
            patterns.push(p);
        }

        Ok(Self {
            set: builder.build()?,
            patterns,
            root: root.to_owned(),
        })
    }

    #[cfg(test)]
    fn is_excluded(&self, path: &Path) -> bool {
        self.matches(path) == MatchResult::Ignore
    }

    fn matches(&self, path: &Path) -> MatchResult {
        if let Ok(stripped) = path.strip_prefix(&self.root) {
            let matches = self.set.matches(stripped);
            if let Some(i) = matches.iter().rev().next() {
                let pattern = &self.patterns[*i];
                return match pattern.pattern_type {
                    PatternType::Whitelist => MatchResult::Whitelist,
                    PatternType::Ignore => MatchResult::Ignore,
                };
            }
        }

        MatchResult::None
    }

    pub fn root_len(&self) -> usize {
        self.root.as_os_str().len()
    }

    fn parse(contents: &[&str]) -> Vec<Pattern> {
        contents
            .iter()
            .filter_map(|l| {
                if !l.is_empty() && !l.starts_with('#') {
                    Some(Pattern::parse(l))
                } else {
                    None
                }
            })
            .collect()
    }
}

impl Pattern {
    fn parse(pattern: &str) -> Self {
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

        Self {
            pattern: normalized,
            pattern_type,
            anchored,
        }
    }
}

impl From<globset::Error> for Error {
    fn from(error: globset::Error) -> Self {
        Self::GlobSet(error)
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
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
        IgnoreFile::from_strings(&[pattern], &base_dir()).expect("test ignore file invalid")
    }

    #[test]
    fn matches_exact() {
        let file = build_ignore("Cargo.toml");

        assert!(file.is_excluded(&base_dir().join("Cargo.toml")));
    }

    #[test]
    fn does_not_match() {
        let file = build_ignore("Cargo.toml");

        assert!(!file.is_excluded(&base_dir().join("src").join("main.rs")));
    }

    #[test]
    fn matches_simple_wildcard() {
        let file = build_ignore("targ*");

        assert!(file.is_excluded(&base_dir().join("target")));
    }

    #[test]
    fn matches_subdir_exact() {
        let file = build_ignore("target");

        assert!(file.is_excluded(&base_dir().join("target/")));
    }

    #[test]
    fn matches_subdir() {
        let file = build_ignore("target");

        assert!(file.is_excluded(&base_dir().join("target").join("file")));
        assert!(file.is_excluded(&base_dir().join("target").join("subdir").join("file")));
    }

    #[test]
    fn wildcard_with_dir() {
        let file = build_ignore("target/f*");

        assert!(file.is_excluded(&base_dir().join("target").join("file")));
        assert!(!file.is_excluded(&base_dir().join("target").join("subdir").join("file")));
    }

    #[test]
    fn leading_slash() {
        let file = build_ignore("/*.c");

        assert!(file.is_excluded(&base_dir().join("cat-file.c")));
        assert!(!file.is_excluded(&base_dir().join("mozilla-sha1").join("sha1.c")));
    }

    #[test]
    fn leading_double_wildcard() {
        let file = build_ignore("**/foo");

        assert!(file.is_excluded(&base_dir().join("foo")));
        assert!(file.is_excluded(&base_dir().join("target").join("foo")));
        assert!(file.is_excluded(&base_dir().join("target").join("subdir").join("foo")));
    }

    #[test]
    fn trailing_double_wildcard() {
        let file = build_ignore("abc/**");

        assert!(!file.is_excluded(&base_dir().join("def").join("foo")));
        assert!(file.is_excluded(&base_dir().join("abc").join("foo")));
        assert!(file.is_excluded(&base_dir().join("abc").join("subdir").join("foo")));
    }

    #[test]
    fn sandwiched_double_wildcard() {
        let file = build_ignore("a/**/b");

        assert!(file.is_excluded(&base_dir().join("a").join("b")));
        assert!(file.is_excluded(&base_dir().join("a").join("x").join("b")));
        assert!(file.is_excluded(&base_dir().join("a").join("x").join("y").join("b")));
    }

    #[test]
    fn empty_file_never_excludes() {
        let file = IgnoreFile::from_strings(&[], &base_dir()).expect("test ignore file invalid");

        assert!(!file.is_excluded(&base_dir().join("target")));
    }

    #[test]
    fn checks_all_patterns() {
        let patterns = vec!["target", "target2"];
        let file =
            IgnoreFile::from_strings(&patterns, &base_dir()).expect("test ignore file invalid");

        assert!(file.is_excluded(&base_dir().join("target").join("foo.txt")));
        assert!(file.is_excluded(&base_dir().join("target2").join("bar.txt")));
    }

    #[test]
    fn handles_whitelisting() {
        let patterns = vec!["target", "!target/foo.txt"];
        let file =
            IgnoreFile::from_strings(&patterns, &base_dir()).expect("test ignore file invalid");

        assert!(!file.is_excluded(&base_dir().join("target").join("foo.txt")));
        assert!(file.is_excluded(&base_dir().join("target").join("blah.txt")));
    }
}
