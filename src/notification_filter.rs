extern crate glob;

use std::io;
use std::path::{Path,PathBuf};

use self::glob::{Pattern,PatternError};

pub struct NotificationFilter {
    cwd: PathBuf,
    filters: Vec<Pattern>,
    ignores: Vec<Pattern>
}

#[derive(Debug)]
pub enum NotificationError {
    BadPattern(PatternError),
    Io(io::Error)
}

impl From<io::Error> for NotificationError {
    fn from(err: io::Error) -> NotificationError {
        NotificationError::Io(err)
    }
}

impl From<PatternError> for NotificationError {
    fn from(err: PatternError) -> NotificationError {
        NotificationError::BadPattern(err)
    }
}

impl NotificationFilter {
    pub fn new(current_dir: &Path) -> Result<NotificationFilter, io::Error> {
        let canonicalized = try!(current_dir.canonicalize());

        Ok(NotificationFilter {
            cwd: canonicalized,
            filters: vec![],
            ignores: vec![]
        })
    }

    pub fn add_extension(&mut self, extension: &str) -> Result<(), NotificationError> {
        let mut pattern = String::new();

        for ext in extension.split(",") {
            pattern.clear();
            pattern.push_str("*");

            if !ext.starts_with(".") {
                pattern.push_str(".");
            }
            pattern.push_str(ext);

            try!(self.add_filter(&pattern));
        }

        Ok(())
    }

    pub fn add_filter(&mut self, pattern: &str) -> Result<(), NotificationError> {
        let compiled = try!(self.pattern_for(pattern));
        self.filters.push(compiled);

        Ok(())
    }

    pub fn add_ignore(&mut self, pattern: &str) -> Result<(), NotificationError> {
        let compiled = try!(self.pattern_for(pattern));
        self.ignores.push(compiled);

        Ok(())
    }

    fn pattern_for(&self, p: &str) -> Result<Pattern, PatternError> {
        let mut path = PathBuf::from(p);
        if path.is_relative() {
            path = self.cwd.join(path.as_path());
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
                return true;
            }
        }

        for pattern in &self.filters {
            if pattern.matches(path_as_str) {
                return false;
            }
        }

        self.filters.len() > 0
    }
}
