extern crate glob;

use error;
use gitignore::Gitignore;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::path::Path;

pub struct NotificationFilter {
    filters: GlobSet,
    filter_count: usize,
    ignores: GlobSet,
    ignore_files: Gitignore,
}

impl NotificationFilter {
    pub fn new(
        filters: &[String],
        ignores: &[String],
        ignore_files: Gitignore,
    ) -> error::Result<NotificationFilter> {
        let mut filter_set_builder = GlobSetBuilder::new();
        for f in filters {
            filter_set_builder.add(Glob::new(f)?);
            debug!("Adding filter: \"{}\"", f);
        }

        let mut ignore_set_builder = GlobSetBuilder::new();
        for i in ignores {
            let mut ignore_path = Path::new(i).to_path_buf();
            if ignore_path.is_relative() && !i.starts_with("*") {
                ignore_path = Path::new("**").join(&ignore_path);
            }
            let pattern = ignore_path.to_str().unwrap();
            ignore_set_builder.add(Glob::new(pattern)?);
            debug!("Adding ignore: \"{}\"", pattern);
        }

        Ok(NotificationFilter {
            filters: filter_set_builder.build()?,
            filter_count: filters.len(),
            ignores: ignore_set_builder.build()?,
            ignore_files: ignore_files,
        })
    }

    pub fn is_excluded(&self, path: &Path) -> bool {
        if self.ignores.is_match(path) {
            debug!("Ignoring {:?}: matched ignore filter", path);
            return true;
        }

        if self.filters.is_match(path) {
            return false;
        }

        if self.ignore_files.is_excluded(path) {
            debug!("Ignoring {:?}: matched gitignore file", path);
            return true;
        }

        if self.filter_count > 0 {
            debug!("Ignoring {:?}: did not match any given filters", path);
        }

        self.filter_count > 0
    }
}

#[cfg(test)]
mod tests {
    use super::NotificationFilter;
    use gitignore;
    use std::path::Path;

    #[test]
    fn test_allows_everything_by_default() {
        let filter = NotificationFilter::new(&[], &[], gitignore::load(&[])).unwrap();

        assert!(!filter.is_excluded(&Path::new("foo")));
    }

    #[test]
    fn test_filename() {
        let filter = NotificationFilter::new(
            &[],
            &["test.json".into()],
            gitignore::load(&[]),
        ).unwrap();

        assert!(filter.is_excluded(&Path::new("/path/to/test.json")));
        assert!(filter.is_excluded(&Path::new("test.json")));
    }

    #[test]
    fn test_multiple_filters() {
        let filters = &["*.rs".into(), "*.toml".into()];
        let filter = NotificationFilter::new(filters, &[], gitignore::load(&[])).unwrap();

        assert!(!filter.is_excluded(&Path::new("hello.rs")));
        assert!(!filter.is_excluded(&Path::new("Cargo.toml")));
        assert!(filter.is_excluded(&Path::new("README.md")));
    }

    #[test]
    fn test_multiple_ignores() {
        let ignores = &["*.rs".into(), "*.toml".into()];
        let filter = NotificationFilter::new(&[], ignores, gitignore::load(&vec![])).unwrap();

        assert!(filter.is_excluded(&Path::new("hello.rs")));
        assert!(filter.is_excluded(&Path::new("Cargo.toml")));
        assert!(!filter.is_excluded(&Path::new("README.md")));
    }

    #[test]
    fn test_ignores_take_precedence() {
        let ignores = &["*.rs".into(), "*.toml".into()];
        let filter =
            NotificationFilter::new(ignores, ignores, gitignore::load(&[])).unwrap();

        assert!(filter.is_excluded(&Path::new("hello.rs")));
        assert!(filter.is_excluded(&Path::new("Cargo.toml")));
        assert!(filter.is_excluded(&Path::new("README.md")));
    }
}
