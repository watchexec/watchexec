use std::{error::Error as StdError, fmt, io, sync::PoisonError};

pub type Result<T> = ::std::result::Result<T, Error>;

pub enum Error {
    Canonicalization(String, io::Error),
    Glob(globset::Error),
    Io(io::Error),
    Notify(notify::Error),
    Generic(String),
    PoisonedLock,
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
            Some(os_err) => match os_err {
                7 => {
                    let msg = "There are so many changed files that the environment variables of the command have been overrun. Try running with --no-meta or --no-environment.";
                    io::Error::new(io::ErrorKind::Other, msg)
                }
                _ => err,
            },
            None => err,
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
        };

        write!(f, "{} error: {}", error_type, error)
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
