use std::{
	ffi::{OsStr, OsString},
	path::MAIN_SEPARATOR,
	sync::Arc,
};

use clap::ArgMatches;
use miette::{IntoDiagnostic, Result};
use tracing::info;
use watchexec::{
	error::RuntimeError,
	event::{
		filekind::{FileEventKind, ModifyKind},
		Event, Priority, Tag,
	},
	filter::Filterer,
};
use watchexec_filterer_globset::GlobsetFilterer;

pub async fn globset(args: &ArgMatches) -> Result<Arc<WatchexecFilterer>> {
	let (project_origin, workdir) = super::common::dirs(args).await?;
	let vcs_types = super::common::vcs_types(&project_origin).await;
	let ignore_files = super::common::ignores(args, &vcs_types, &project_origin).await;

	let mut ignores = Vec::new();

	if !args.is_present("no-default-ignore") {
		ignores.extend([
			(format!("**{MAIN_SEPARATOR}.DS_Store"), None),
			(String::from("*.py[co]"), None),
			(String::from("#*#"), None),
			(String::from(".#*"), None),
			(String::from(".*.kate-swp"), None),
			(String::from(".*.sw?"), None),
			(String::from(".*.sw?x"), None),
			(format!("**{MAIN_SEPARATOR}.bzr{MAIN_SEPARATOR}**"), None),
			(format!("**{MAIN_SEPARATOR}_darcs{MAIN_SEPARATOR}**"), None),
			(
				format!("**{MAIN_SEPARATOR}.fossil-settings{MAIN_SEPARATOR}**"),
				None,
			),
			(format!("**{MAIN_SEPARATOR}.git{MAIN_SEPARATOR}**"), None),
			(format!("**{MAIN_SEPARATOR}.hg{MAIN_SEPARATOR}**"), None),
			(format!("**{MAIN_SEPARATOR}.pijul{MAIN_SEPARATOR}**"), None),
			(format!("**{MAIN_SEPARATOR}.svn{MAIN_SEPARATOR}**"), None),
		]);
	}

	let filters = args
		.values_of("filter")
		.unwrap_or_default()
		.map(|f| (f.to_owned(), Some(workdir.clone())));

	ignores.extend(
		args.values_of("ignore")
			.unwrap_or_default()
			.map(|f| (f.to_owned(), Some(workdir.clone()))),
	);

	let exts = args
		.values_of_os("extensions")
		.unwrap_or_default()
		.flat_map(|s| s.split(b','))
		.map(|e| os_strip_prefix(e, b'.'));

	info!("initialising Globset filterer");
	Ok(Arc::new(WatchexecFilterer {
		inner: GlobsetFilterer::new(project_origin, filters, ignores, ignore_files, exts)
			.await
			.into_diagnostic()?,
		no_meta: args.is_present("no-meta"),
	}))
}

/// A custom filterer that combines the library's Globset filterer and a switch for --no-meta
#[derive(Debug)]
pub struct WatchexecFilterer {
	inner: GlobsetFilterer,
	no_meta: bool,
}

impl Filterer for WatchexecFilterer {
	fn check_event(&self, event: &Event, priority: Priority) -> Result<bool, RuntimeError> {
		let is_meta = event.tags.iter().any(|tag| {
			matches!(
				tag,
				Tag::FileEventKind(FileEventKind::Modify(ModifyKind::Metadata(_)))
			)
		});

		if self.no_meta && is_meta {
			Ok(false)
		} else {
			self.inner.check_event(event, priority)
		}
	}
}

trait OsStringSplit {
	fn split(&self, sep: u8) -> OsSplit;
}

impl OsStringSplit for OsStr {
	fn split(&self, sep: u8) -> OsSplit {
		OsSplit {
			os: self.to_os_string(),
			pos: 0,
			sep,
		}
	}
}

struct OsSplit {
	os: OsString,
	pos: usize,
	sep: u8,
}

#[cfg(unix)]
impl Iterator for OsSplit {
	type Item = OsString;

	fn next(&mut self) -> Option<Self::Item> {
		use std::os::unix::ffi::{OsStrExt, OsStringExt};
		let bytes = self.os.as_bytes();
		if self.pos >= bytes.len() {
			None
		} else {
			let mut pos = self.pos;
			while pos < bytes.len() && bytes[pos] != self.sep {
				pos += 1;
			}

			let res = OsString::from_vec(bytes[self.pos..pos].to_vec());
			self.pos = pos + 1;
			Some(res)
		}
	}
}

#[cfg(unix)]
fn os_strip_prefix(os: OsString, prefix: u8) -> OsString {
	use std::os::unix::ffi::{OsStrExt, OsStringExt};
	let bytes = os.as_bytes();
	if bytes.first().copied() == Some(prefix) {
		OsString::from_vec(bytes[1..].to_vec())
	} else {
		os
	}
}

#[cfg(windows)]
impl Iterator for OsSplit {
	type Item = OsString;

	fn next(&mut self) -> Option<Self::Item> {
		use std::os::windows::ffi::{OsStrExt, OsStringExt};
		let wides = self.os.encode_wide().skip(self.pos);

		let mut cur = Vec::new();
		for wide in wides {
			if wide == u16::from(self.sep) {
				break;
			}

			cur.push(wide);
		}

		self.pos += cur.len() + 1;
		if cur.is_empty() && self.pos >= self.os.len() {
			None
		} else {
			Some(OsString::from_wide(&cur))
		}
	}
}

#[cfg(windows)]
fn os_strip_prefix(os: OsString, prefix: u8) -> OsString {
	use std::os::windows::ffi::{OsStrExt, OsStringExt};
	let wides: Vec<u16> = os.encode_wide().collect();
	if wides.first().copied() == Some(u16::from(prefix)) {
		OsString::from_wide(&wides[1..])
	} else {
		os
	}
}

#[cfg(test)]
#[test]
fn os_split_none() {
	let os = OsString::from("");
	assert_eq!(
		os.split(b',').collect::<Vec<OsString>>(),
		Vec::<OsString>::new()
	);

	let mut split = os.split(b',');
	assert_eq!(split.next(), None);
}

#[cfg(test)]
#[test]
fn os_split_one() {
	let os = OsString::from("abc");
	assert_eq!(
		os.split(b',').collect::<Vec<OsString>>(),
		vec![OsString::from("abc")]
	);

	let mut split = os.split(b',');
	assert_eq!(split.next(), Some(OsString::from("abc")));
	assert_eq!(split.next(), None);
}

#[cfg(test)]
#[test]
fn os_split_multi() {
	let os = OsString::from("a,b,c");
	assert_eq!(
		os.split(b',').collect::<Vec<OsString>>(),
		vec![
			OsString::from("a"),
			OsString::from("b"),
			OsString::from("c"),
		]
	);

	let mut split = os.split(b',');
	assert_eq!(split.next(), Some(OsString::from("a")));
	assert_eq!(split.next(), Some(OsString::from("b")));
	assert_eq!(split.next(), Some(OsString::from("c")));
	assert_eq!(split.next(), None);
}

#[cfg(test)]
#[test]
fn os_split_leading() {
	let os = OsString::from(",a,b,c");
	assert_eq!(
		os.split(b',').collect::<Vec<OsString>>(),
		vec![
			OsString::from(""),
			OsString::from("a"),
			OsString::from("b"),
			OsString::from("c"),
		]
	);

	let mut split = os.split(b',');
	assert_eq!(split.next(), Some(OsString::from("")));
	assert_eq!(split.next(), Some(OsString::from("a")));
	assert_eq!(split.next(), Some(OsString::from("b")));
	assert_eq!(split.next(), Some(OsString::from("c")));
	assert_eq!(split.next(), None);
}

#[cfg(test)]
#[test]
fn os_strip_none() {
	let os = OsString::from("abc");
	assert_eq!(os_strip_prefix(os, b'.'), OsString::from("abc"));
}

#[cfg(test)]
#[test]
fn os_strip_left() {
	let os = OsString::from(".abc");
	assert_eq!(os_strip_prefix(os, b'.'), OsString::from("abc"));
}

#[cfg(test)]
#[test]
fn os_strip_not_right() {
	let os = OsString::from("abc.");
	assert_eq!(os_strip_prefix(os, b'.'), OsString::from("abc."));
}

#[cfg(test)]
#[test]
fn os_strip_only_left() {
	let os = OsString::from(".abc.");
	assert_eq!(os_strip_prefix(os, b'.'), OsString::from("abc."));
}

#[cfg(test)]
#[test]
fn os_strip_only_once() {
	let os = OsString::from("..abc");
	assert_eq!(os_strip_prefix(os, b'.'), OsString::from(".abc"));
}
