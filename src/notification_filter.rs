extern crate glob;

use gitignore;
use std::io;
use std::path::{Path, PathBuf};

use self::glob::{Pattern, PatternError};

pub struct NotificationFilter {
    filters: Vec<Pattern>,
    ignores: Vec<Pattern>,
    ignore_file: Option<gitignore::PatternSet>,
}

#[derive(Debug)]
pub enum Error {
    BadPattern(PatternError),
    Io(io::Error),
}

impl NotificationFilter {
    pub fn new(current_dir: &Path,
               filters: Vec<String>,
               ignores: Vec<String>,
               ignore_file: Option<gitignore::PatternSet>)
               -> Result<NotificationFilter, Error> {
        let cwd = try!(current_dir.canonicalize());

        let compiled_filters = try!(filters.iter()
            .map(|p| NotificationFilter::pattern_for(&cwd, p))
            .collect());

        let compiled_ignores = try!(ignores.iter()
            .map(|p| NotificationFilter::pattern_for(&cwd, p))
            .collect());

        for compiled_filter in &compiled_filters {
            debug!("Adding filter: {}", compiled_filter);
        }

        for compiled_ignore in &compiled_ignores {
            debug!("Adding ignore: {}", compiled_ignore);
        }

        Ok(NotificationFilter {
            filters: compiled_filters,
            ignores: compiled_ignores,
            ignore_file: ignore_file,
        })
    }

    fn pattern_for(cwd: &PathBuf, p: &str) -> Result<Pattern, PatternError> {
        let mut path = PathBuf::from(p);
        if path.is_relative() {
            path = cwd.join(path.as_path());
        }

        if let Ok(metadata) = path.metadata() {
            if metadata.is_dir() {
                path = path.join("*");
            }
        }

        Pattern::new(path.to_str().unwrap())
    }

    pub fn is_excluded(&self, path: &Path) -> bool {
        let path_as_str = path.to_str().unwrap();

        for pattern in &self.ignores {
            if pattern.matches(path_as_str) {
                debug!("Ignoring {:?}: matched ignore filter", path);
                return true;
            }
        }

        for pattern in &self.filters {
            if pattern.matches(path_as_str) {
                return false;
            }
        }

        if let Some(ref ignore_file) = self.ignore_file {
            if ignore_file.is_excluded(path) {
                debug!("Ignoring {:?}: matched gitignore file", path);
                return true;
            }
        }

        if !self.filters.is_empty() {
            debug!("Ignoring {:?}: did not match any given filters", path);
        }

        !self.filters.is_empty()
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::Io(err)
    }
}

impl From<PatternError> for Error {
    fn from(err: PatternError) -> Error {
        Error::BadPattern(err)
    }
}

#[cfg(test)]
mod tests {
    use super::NotificationFilter;
    use std::path::Path;

    #[test]
    fn test_allows_everything_by_default() {
        let filter = NotificationFilter::new(&Path::new("."), vec![], vec![], None).unwrap();

        assert!(!filter.is_excluded(&Path::new("foo")));
    }

    #[test]
    fn test_multiple_filters() {
        let filters = vec![String::from("*.rs"), String::from("*.toml")];
        let filter = NotificationFilter::new(&Path::new("."), filters, vec![], None).unwrap();
        let cwd = Path::new(".").canonicalize().unwrap();

        assert!(!filter.is_excluded(&cwd.join("hello.rs")));
        assert!(!filter.is_excluded(&cwd.join("Cargo.toml")));
        assert!(filter.is_excluded(&cwd.join("README.md")));
    }

    #[test]
    fn test_multiple_ignores() {
        let ignores = vec![String::from("*.rs"), String::from("*.toml")];
        let filter = NotificationFilter::new(&Path::new("."), vec![], ignores, None).unwrap();
        let cwd = Path::new(".").canonicalize().unwrap();

        assert!(filter.is_excluded(&cwd.join("hello.rs")));
        assert!(filter.is_excluded(&cwd.join("Cargo.toml")));
        assert!(!filter.is_excluded(&cwd.join("README.md")));
    }

    #[test]
    fn test_ignores_take_precedence() {
        let ignores = vec![String::from("*.rs"), String::from("*.toml")];
        let filter = NotificationFilter::new(&Path::new("."), ignores.clone(), ignores, None).unwrap();
        let cwd = Path::new(".").canonicalize().unwrap();

        assert!(filter.is_excluded(&cwd.join("hello.rs")));
        assert!(filter.is_excluded(&cwd.join("Cargo.toml")));
        assert!(filter.is_excluded(&cwd.join("README.md")));
    }
}
