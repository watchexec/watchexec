use crate::error;
use crate::gitignore::Gitignore;
use crate::ignore::Ignore;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::path::Path;

pub struct NotificationFilter {
    filters: GlobSet,
    filter_count: usize,
    ignores: GlobSet,
    gitignore_files: Gitignore,
    ignore_files: Ignore,
}

impl NotificationFilter {
    pub fn new(
        filters: &[String],
        ignores: &[String],
        gitignore_files: Gitignore,
        ignore_files: Ignore,
    ) -> error::Result<Self> {
        let mut filter_set_builder = GlobSetBuilder::new();
        for f in filters {
            filter_set_builder.add(Glob::new(f)?);
            debug!("Adding filter: \"{}\"", f);
        }

        let mut ignore_set_builder = GlobSetBuilder::new();
        for i in ignores {
            let mut ignore_path = Path::new(i).to_path_buf();
            if ignore_path.is_relative() && !i.starts_with('*') {
                ignore_path = Path::new("**").join(&ignore_path);
            }
            let pattern = ignore_path.to_str().expect("corrupted memory (string -> path -> string)");
            ignore_set_builder.add(Glob::new(pattern)?);
            debug!("Adding ignore: \"{}\"", pattern);
        }

        Ok(Self {
            filters: filter_set_builder.build()?,
            filter_count: filters.len(),
            ignores: ignore_set_builder.build()?,
            gitignore_files,
            ignore_files,
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
            debug!("Ignoring {:?}: matched ignore file", path);
            return true;
        }

        if self.gitignore_files.is_excluded(path) {
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
    use crate::gitignore;
    use crate::ignore;
    use std::path::Path;

    #[test]
    fn test_allows_everything_by_default() {
        let filter =
            NotificationFilter::new(&[], &[], gitignore::load(&[]), ignore::load(&[]))
                .expect("test filter errors");

        assert!(!filter.is_excluded(Path::new("foo")));
    }

    #[test]
    fn test_filename() {
        let filter = NotificationFilter::new(
            &[],
            &["test.json".into()],
            gitignore::load(&[]),
            ignore::load(&[]),
        )
                .expect("test filter errors");

        assert!(filter.is_excluded(Path::new("/path/to/test.json")));
        assert!(filter.is_excluded(Path::new("test.json")));
    }

    #[test]
    fn test_multiple_filters() {
        let filters = &["*.rs".into(), "*.toml".into()];
        let filter =
            NotificationFilter::new(filters, &[], gitignore::load(&[]), ignore::load(&[]))
                .expect("test filter errors");

        assert!(!filter.is_excluded(Path::new("hello.rs")));
        assert!(!filter.is_excluded(Path::new("Cargo.toml")));
        assert!(filter.is_excluded(Path::new("README.md")));
    }

    #[test]
    fn test_multiple_ignores() {
        let ignores = &["*.rs".into(), "*.toml".into()];
        let filter =
            NotificationFilter::new(&[], ignores, gitignore::load(&[]), ignore::load(&[]))
                .expect("test filter errors");

        assert!(filter.is_excluded(Path::new("hello.rs")));
        assert!(filter.is_excluded(Path::new("Cargo.toml")));
        assert!(!filter.is_excluded(Path::new("README.md")));
    }

    #[test]
    fn test_ignores_take_precedence() {
        let ignores = &["*.rs".into(), "*.toml".into()];
        let filter =
            NotificationFilter::new(ignores, ignores, gitignore::load(&[]), ignore::load(&[]))
                .expect("test filter errors");

        assert!(filter.is_excluded(Path::new("hello.rs")));
        assert!(filter.is_excluded(Path::new("Cargo.toml")));
        assert!(filter.is_excluded(Path::new("README.md")));
    }
}
