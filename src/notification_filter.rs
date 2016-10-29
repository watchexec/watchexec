extern crate glob;

use gitignore;
use std::io;
use std::path::Path;

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
    pub fn new(filters: Vec<String>,
               ignores: Vec<String>,
               ignore_file: Option<gitignore::PatternSet>)
               -> Result<NotificationFilter, Error> {
        let compiled_filters = try!(filters.iter()
            .map(|p| Pattern::new(p))
            .collect());

        let compiled_ignores = try!(ignores.iter()
            .map(|p| Pattern::new(p))
            .collect());

        for compiled_filter in &compiled_filters {
            debug!("Adding filter: \"{}\"", compiled_filter);
        }

        for compiled_ignore in &compiled_ignores {
            debug!("Adding ignore: \"{}\"", compiled_ignore);
        }

        Ok(NotificationFilter {
            filters: compiled_filters,
            ignores: compiled_ignores,
            ignore_file: ignore_file,
        })
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
        let filter = NotificationFilter::new(vec![], vec![], None).unwrap();

        assert!(!filter.is_excluded(&Path::new("foo")));
    }

    #[test]
    fn test_multiple_filters() {
        let filters = vec![String::from("*.rs"), String::from("*.toml")];
        let filter = NotificationFilter::new(filters, vec![], None).unwrap();

        assert!(!filter.is_excluded(&Path::new("hello.rs")));
        assert!(!filter.is_excluded(&Path::new("Cargo.toml")));
        assert!(filter.is_excluded(&Path::new("README.md")));
    }

    #[test]
    fn test_multiple_ignores() {
        let ignores = vec![String::from("*.rs"), String::from("*.toml")];
        let filter = NotificationFilter::new(vec![], ignores, None).unwrap();

        assert!(filter.is_excluded(&Path::new("hello.rs")));
        assert!(filter.is_excluded(&Path::new("Cargo.toml")));
        assert!(!filter.is_excluded(&Path::new("README.md")));
    }

    #[test]
    fn test_ignores_take_precedence() {
        let ignores = vec![String::from("*.rs"), String::from("*.toml")];
        let filter = NotificationFilter::new(ignores.clone(), ignores, None).unwrap();

        assert!(filter.is_excluded(&Path::new("hello.rs")));
        assert!(filter.is_excluded(&Path::new("Cargo.toml")));
        assert!(filter.is_excluded(&Path::new("README.md")));
    }
}
