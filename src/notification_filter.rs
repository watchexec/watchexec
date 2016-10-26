extern crate glob;

use gitignore;
use std::io;
use std::path::{Path, PathBuf};

use self::glob::{Pattern, PatternError};

pub struct NotificationFilter {
    cwd: PathBuf,
    filters: Vec<Pattern>,
    ignores: Vec<Pattern>,
    ignore_file: Option<gitignore::PatternSet>,
}

#[derive(Debug)]
pub enum NotificationError {
    BadPattern(PatternError),
    Io(io::Error),
}

impl NotificationFilter {
    pub fn new(current_dir: &Path,
               ignore_file: Option<gitignore::PatternSet>)
               -> Result<NotificationFilter, io::Error> {
        let canonicalized = try!(current_dir.canonicalize());

        Ok(NotificationFilter {
            cwd: canonicalized,
            filters: vec![],
            ignores: vec![],
            ignore_file: ignore_file,
        })
    }

    pub fn add_filter(&mut self, pattern: &str) -> Result<(), NotificationError> {
        let compiled = try!(self.pattern_for(pattern));
        self.filters.push(compiled);

        debug!("Adding filter: {}", pattern);

        Ok(())
    }

    pub fn add_ignore(&mut self, pattern: &str) -> Result<(), NotificationError> {
        let compiled = try!(self.pattern_for(pattern));
        self.ignores.push(compiled);

        debug!("Adding ignore: {}", pattern);

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
