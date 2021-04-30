use std::{error::Error as StdError, fmt, io, sync::PoisonError};

pub type Result<T> = ::std::result::Result<T, Error>;

#[non_exhaustive]
pub enum Error {
    Canonicalization(String, io::Error),
    Glob(globset::Error),
    Io(io::Error),
    Notify(notify::Error),
    Generic(String),
    PoisonedLock,
    ClearScreen(clearscreen::Error),
}

impl StdError for Error {}

impl From<String> for Error {
    fn from(err: String) -> Self {
        Self::Generic(err)
    }
}

impl From<globset::Error> for Error {
    fn from(err: globset::Error) -> Self {
        Self::Glob(err)
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Self::Io(match err.raw_os_error() {
            Some(7) => io::Error::new(io::ErrorKind::Other, "There are so many changed files that the environment variables of the command have been overrun. Try running with --no-meta or --no-environment."),
            _ => err,
        })
    }
}

impl From<notify::Error> for Error {
    fn from(err: notify::Error) -> Self {
        match err {
            notify::Error::Io(err) => Self::Io(err),
            other => Self::Notify(other),
        }
    }
}

impl<'a, T> From<PoisonError<T>> for Error {
    fn from(_err: PoisonError<T>) -> Self {
        Self::PoisonedLock
    }
}

impl From<clearscreen::Error> for Error {
    fn from(err: clearscreen::Error) -> Self {
        match err {
            clearscreen::Error::Io(err) => Self::Io(err),
            other => Self::ClearScreen(other),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let (error_type, error) = match self {
            Self::Canonicalization(path, err) => (
                "Path",
                format!("couldn't canonicalize '{}':\n{}", path, err),
            ),
            Self::Generic(err) => ("", err.clone()),
            Self::Glob(err) => ("Globset", err.to_string()),
            Self::Io(err) => ("I/O", err.to_string()),
            Self::Notify(err) => ("Notify", err.to_string()),
            Self::PoisonedLock => ("Internal", "poisoned lock".to_string()),
            Self::ClearScreen(err) => ("ClearScreen", err.to_string()),
        };

        write!(f, "{} error: {}", error_type, error)
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
