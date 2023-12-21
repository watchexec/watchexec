//! A filterer implementation that exposes the full capabilities of Watchexec.
//!
//! Filters match against [event tags][Tag]; can be exact matches, glob matches, regex matches, or
//! set matches; can reverse the match (equal/not equal, etc); and can be negated.
//!
//! [Filters][Filter] can be generated from your application and inserted directly, or they can be
//! parsed from a textual format:
//!
//! ```text
//! [!]{Matcher}{Op}{Value}
//! ```
//!
//! For example:
//!
//! ```text
//! path==/foo/bar
//! path*=**/bar
//! path~=bar$
//! !kind=file
//! ```
//!
//! There is a set of [operators][Op]:
//! - `==` and `!=`: exact match and exact not match (case insensitive)
//! - `~=` and `~!`: regex match and regex not match
//! - `*=` and `*!`: glob match and glob not match
//! - `:=` and `:!`: set match and set not match
//!
//! Sets are a list of values separated by `,`.
//!
//! In addition to the two-symbol operators, there is the `=` "auto" operator, which maps to the
//! most convenient operator for the given _matcher_. The current mapping is:
//!
//! | Matcher                                           | Operator      |
//! |---------------------------------------------------|---------------|
//! | [`Tag`](Matcher::Tag)                             | `:=` (in set) |
//! | [`Path`](Matcher::Path)                           | `*=` (glob)   |
//! | [`FileType`](Matcher::FileType)                   | `:=` (in set) |
//! | [`FileEventKind`](Matcher::FileEventKind)         | `*=` (glob)   |
//! | [`Source`](Matcher::Source)                       | `:=` (in set) |
//! | [`Process`](Matcher::Process)                     | `:=` (in set) |
//! | [`Signal`](Matcher::Signal)                       | `:=` (in set) |
//! | [`ProcessCompletion`](Matcher::ProcessCompletion) | `*=` (glob)   |
//! | [`Priority`](Matcher::Priority)                   | `:=` (in set) |
//!
//! [Matchers][Matcher] correspond to Tags, but are not one-to-one: the `path` matcher operates on
//! the `path` part of the `Path` tag, and the `type` matcher operates on the `file_type`, for
//! example.
//!
//! | Matcher                              | Syntax   | Tag                                          |
//! |-------------------------------------------|----------|----------------------------------------------|
//! | [`Tag`](Matcher::Tag)                | `tag`    | _the presence of a Tag on the event_         |
//! | [`Path`](Matcher::Path)              | `path`   | [`Path`](Tag::Path) (`path` field)             |
//! | [`FileType`](Matcher::FileType)      | `type`   | [`Path`](Tag::Path) (`file_type` field, when Some) |
//! | [`FileEventKind`](Matcher::FileEventKind) | `kind` or `fek` | [`FileEventKind`](Tag::FileEventKind) |
//! | [`Source`](Matcher::Source)          | `source` or `src`  | [`Source`](Tag::Source)              |
//! | [`Process`](Matcher::Process)        | `process` or `pid` | [`Process`](Tag::Process)            |
//! | [`Signal`](Matcher::Signal)          | `signal` | [`Signal`](Tag::Signal)                        |
//! | [`ProcessCompletion`](Matcher::ProcessCompletion) | `complete` or `exit` | [`ProcessCompletion`](Tag::ProcessCompletion) |
//! | [`Priority`](Matcher::Priority)      | `priority` | special: event [`Priority`] |
//!
//! Filters are checked in order, grouped per tag and per matcher. Filter groups may be checked in
//! any order, but the filters in the groups are checked in add order. Path glob filters are always
//! checked first, for internal reasons.
//!
//! The `negate` boolean field behaves specially: it is not operator negation, but rather the same
//! kind of behaviour that is applied to `!`-prefixed globs in gitignore files: if a negated filter
//! matches the event, the result of the event checking for that matcher is reverted to `true`, even
//! if a previous filter set it to `false`. Unmatched negated filters are ignored.
//!
//! Glob syntax is as supported by the [ignore] crate for Paths, and by [globset] otherwise. (As of
//! writing, the ignore crate uses globset internally). Regex syntax is the default syntax of the
//! [regex] crate.

#![doc(html_favicon_url = "https://watchexec.github.io/logo:watchexec.svg")]
#![doc(html_logo_url = "https://watchexec.github.io/logo:watchexec.svg")]
#![warn(clippy::unwrap_used, missing_docs)]
#![deny(rust_2018_idioms)]

// to make filters
pub use regex::Regex;

pub use error::*;
pub use files::*;
pub use filter::*;
pub use filterer::*;

mod error;
mod files;
mod filter;
mod filterer;
mod parse;
mod swaplock;
