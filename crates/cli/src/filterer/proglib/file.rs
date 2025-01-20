use std::{
	fs::{metadata, File, FileType, Metadata},
	io::{BufReader, Read},
	iter::once,
	time::{SystemTime, UNIX_EPOCH},
};

use jaq_core::{Error, Native};
use jaq_json::Val;
use jaq_std::{v, Filter};
use serde_json::{json, Value};
use tracing::{debug, error};

use super::macros::return_err;

pub fn funs() -> [Filter<Native<jaq_json::Val>>; 3] {
	[
		(
			"file_read",
			v(0),
			Native::new({
				move |_, (mut ctx, val)| {
					let path = match &val {
						Val::Str(v) => v.to_string(),
						_ => return_err!(Err(Error::str("expected string (path) but got {val:?}"))),
					};

					let Val::Int(bytes) = ctx.pop_var() else {
						return_err!(Err(Error::str("expected integer")));
					};

					let bytes = match u64::try_from(bytes) {
						Ok(b) => b,
						Err(err) => return_err!(Err(Error::str(format!(
							"expected positive integer; {err}"
						)))),
					};

					Box::new(once(Ok(match File::open(&path) {
						Ok(file) => {
							let buf_reader = BufReader::new(file);
							let mut limited = buf_reader.take(bytes);
							let mut buffer = String::with_capacity(bytes as _);
							match limited.read_to_string(&mut buffer) {
								Ok(read) => {
									debug!("jaq: read {read} bytes from {path:?}");
									Val::Str(buffer.into())
								}
								Err(err) => {
									error!("jaq: failed to read from {path:?}: {err:?}");
									Val::Null
								}
							}
						}
						Err(err) => {
							error!("jaq: failed to open file {path:?}: {err:?}");
							Val::Null
						}
					})))
				}
			}),
		),
		(
			"file_meta",
			v(0),
			Native::new({
				move |_, (_, val)| {
					let path = match &val {
						Val::Str(v) => v.to_string(),
						_ => return_err!(Err(Error::str("expected string (path) but got {val:?}"))),
					};

					Box::new(once(Ok(match metadata(&path) {
						Ok(meta) => Val::from(json_meta(meta)),
						Err(err) => {
							error!("jaq: failed to open {path:?}: {err:?}");
							Val::Null
						}
					})))
				}
			}),
		),
		(
			"file_size",
			v(0),
			Native::new({
				move |_, (_, val)| {
					let path = match &val {
						Val::Str(v) => v.to_string(),
						_ => return_err!(Err(Error::str("expected string (path) but got {val:?}"))),
					};

					Box::new(once(Ok(match metadata(&path) {
						Ok(meta) => Val::Int(meta.len() as _),
						Err(err) => {
							error!("jaq: failed to open {path:?}: {err:?}");
							Val::Null
						}
					})))
				}
			}),
		),
	]
}

fn json_meta(meta: Metadata) -> Value {
	let perms = meta.permissions();
	#[cfg_attr(not(unix), allow(unused_mut))]
	let mut val = json!({
		"type": filetype_str(meta.file_type()),
		"size": meta.len(),
		"modified": fs_time(meta.modified()),
		"accessed": fs_time(meta.accessed()),
		"created": fs_time(meta.created()),
		"dir": meta.is_dir(),
		"file": meta.is_file(),
		"symlink": meta.is_symlink(),
		"readonly": perms.readonly(),
	});

	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		let map = val.as_object_mut().unwrap();
		map.insert(
			"mode".to_string(),
			Value::String(format!("{:o}", perms.mode())),
		);
		map.insert("mode_byte".to_string(), Value::from(perms.mode()));
		map.insert(
			"executable".to_string(),
			Value::Bool(perms.mode() & 0o111 != 0),
		);
	}

	val
}

fn filetype_str(filetype: FileType) -> &'static str {
	#[cfg(unix)]
	{
		use std::os::unix::fs::FileTypeExt;
		if filetype.is_char_device() {
			return "char";
		} else if filetype.is_block_device() {
			return "block";
		} else if filetype.is_fifo() {
			return "fifo";
		} else if filetype.is_socket() {
			return "socket";
		}
	}

	#[cfg(windows)]
	{
		use std::os::windows::fs::FileTypeExt;
		if filetype.is_symlink_dir() {
			return "symdir";
		} else if filetype.is_symlink_file() {
			return "symfile";
		}
	}

	if filetype.is_dir() {
		"dir"
	} else if filetype.is_file() {
		"file"
	} else if filetype.is_symlink() {
		"symlink"
	} else {
		"unknown"
	}
}

fn fs_time(time: std::io::Result<SystemTime>) -> Option<u64> {
	time.ok()
		.and_then(|time| time.duration_since(UNIX_EPOCH).ok())
		.map(|dur| dur.as_secs())
}
