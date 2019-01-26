use clap;
use globset;
use notify;
use std::{error::Error as StdError, fmt, io, sync::PoisonError};

pub type Result<T> = ::std::result::Result<T, Error>;

pub enum Error {
    Canonicalization(String, io::Error),
    Clap(clap::Error),
    Glob(globset::Error),
    Io(io::Error),
    Notify(notify::Error),
    PoisonedLock,
}

impl StdError for Error {
    fn description(&self) -> &str {
        // This method is soft-deprecated and shouldn't be used,
        // see Display for the actual description.
        "a watchexec error"
    }
}

impl From<clap::Error> for Error {
    fn from(err: clap::Error) -> Self {
        Error::Clap(err)
    }
}

impl From<globset::Error> for Error {
    fn from(err: globset::Error) -> Self {
        Error::Glob(err)
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}

impl From<notify::Error> for Error {
    fn from(err: notify::Error) -> Self {
        match err {
            notify::Error::Io(err) => Error::Io(err),
            other => Error::Notify(other),
        }
    }
}

impl<'a, T> From<PoisonError<T>> for Error {
    fn from(_err: PoisonError<T>) -> Self {
        Error::PoisonedLock
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} error: {}",
            match self {
                Error::Canonicalization(_, _) => "Path",
                Error::Clap(_) => "Argument",
                Error::Glob(_) => "Globset",
                Error::Io(_) => "I/O",
                Error::Notify(_) => "Notify",
                Error::PoisonedLock => "Internal",
            },
            match self {
                Error::Canonicalization(path, err) => {
                    format!("couldn't canonicalize '{}':\n{}", path, err)
                }
                err => format!("{}", err),
            }
        )
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
