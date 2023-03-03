use std::ffi::{OsStr, OsString};

pub trait OsStringSplit {
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

pub struct OsSplit {
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
pub fn os_strip_prefix(os: OsString, prefix: u8) -> OsString {
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
pub fn os_strip_prefix(os: OsString, prefix: u8) -> OsString {
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
