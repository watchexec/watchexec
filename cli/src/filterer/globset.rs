use std::{
	ffi::{OsStr, OsString},
	sync::Arc,
};

use clap::ArgMatches;
use miette::{IntoDiagnostic, Result};
use watchexec::filter::globset::GlobsetFilterer;

pub async fn globset(args: &ArgMatches<'static>) -> Result<Arc<GlobsetFilterer>> {
	let (project_origin, workdir) = super::common::dirs(args).await?;
	let ignorefiles = super::common::ignores(args, &project_origin).await?;

	let mut ignores = Vec::new();
	for ignore in ignorefiles {
		ignores.extend(GlobsetFilterer::list_from_ignore_file(&ignore).await?);
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
		.map(|s| s.split(b','))
		.flatten();

	Ok(Arc::new(
		GlobsetFilterer::new(project_origin, filters, ignores, exts).into_diagnostic()?,
	))
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

			let res = OsString::from_vec(bytes[self.pos..=pos].to_vec());
			self.pos = pos + 1;
			Some(res)
		}
	}
}

#[cfg(windows)]
impl Iterator for OsSplit {
	type Item = OsString;

	fn next(&mut self) -> Option<Self::Item> {
		use std::os::windows::ffi::{OsStrExt, OsStringExt};
		let mut wides = self.os.encode_wide();
		wides.skip(self.pos);

		let mut cur = Vec::new();
		for wide in wides {
			if wide == u16::from(self.sep) {
				break;
			}

			cur.push(wide);
		}

		let res = OsString::from_wide(&cur);
		self.pos = cur.len() + 1;
		Some(res)
	}
}
