use std::{fs::File, io::Read, iter::once};

use jaq_core::{Error, Native};
use jaq_json::Val;
use jaq_std::{v, Filter};
use tracing::{debug, error};

use super::macros::return_err;

pub fn funs() -> [Filter<Native<jaq_json::Val>>; 2] {
	[
		(
			"hash",
			v(0),
			Native::new({
				move |_, (_, val)| {
					let string = match &val {
						Val::Str(v) => v.to_string(),
						_ => return_err!(Err(Error::str("expected string but got {val:?}"))),
					};

					Box::new(once(Ok(Val::Str(
						blake3::hash(string.as_bytes()).to_hex().to_string().into(),
					))))
				}
			}),
		),
		(
			"file_hash",
			v(0),
			Native::new({
				move |_, (_, val)| {
					let path = match &val {
						Val::Str(v) => v.to_string(),
						_ => return_err!(Err(Error::str("expected string but got {val:?}"))),
					};

					Box::new(once(Ok(match File::open(&path) {
						Ok(mut file) => {
							const BUFFER_SIZE: usize = 1024 * 1024;
							let mut hasher = blake3::Hasher::new();
							let mut buf = vec![0; BUFFER_SIZE];
							while let Ok(bytes) = file.read(&mut buf) {
								debug!("jaq: read {bytes} bytes from {path:?}");
								if bytes == 0 {
									break;
								}
								hasher.update(&buf[..bytes]);
								buf = vec![0; BUFFER_SIZE];
							}

							Val::Str(hasher.finalize().to_hex().to_string().into())
						}
						Err(err) => {
							error!("jaq: failed to open file {path:?}: {err:?}");
							Val::Null
						}
					})))
				}
			}),
		),
	]
}
