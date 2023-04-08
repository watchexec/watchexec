use std::{
	fs::{metadata, File, FileType, Metadata},
	io::{BufReader, Read},
	iter::once,
	sync::Arc,
	time::{SystemTime, UNIX_EPOCH},
};

use dashmap::DashMap;
use jaq_core::{CustomFilter, Definitions, Error, Val};
use miette::miette;
use serde_json::{json, Value};
use tracing::{debug, error, info, trace, warn};

pub fn load_std_defs() -> miette::Result<Definitions> {
	debug!("loading jaq core library");
	let mut defs = Definitions::core();

	debug!("loading jaq standard library");
	let mut errs = Vec::new();
	jaq_std::std()
		.into_iter()
		.for_each(|def| defs.insert(def, &mut errs));

	if !errs.is_empty() {
		return Err(miette!("failed to load jaq standard library: {:?}", errs));
	}
	Ok(defs)
}

macro_rules! return_err {
	($err:expr) => {
		return Box::new(once($err))
	};
}

#[inline]
fn custom_err<T>(err: impl Into<String>) -> Result<T, Error> {
	Err(Error::Custom(err.into()))
}

macro_rules! string_arg {
	($args:expr, $n:expr, $ctx:expr, $val:expr) => {
		match $args[$n].run(($ctx.clone(), $val.clone())).next() {
			Some(Ok(Val::Str(v))) => Ok(v.to_string()),
			Some(Ok(val)) => custom_err(format!("expected string but got {val:?}")),
			Some(Err(e)) => Err(e),
			None => custom_err("value expected but none found"),
		}
	};
}

macro_rules! int_arg {
	($args:expr, $n:expr, $ctx:expr, $val:expr) => {
		match $args[$n].run(($ctx.clone(), $val.clone())).next() {
			Some(Ok(Val::Int(v))) => Ok(v as _),
			Some(Ok(val)) => custom_err(format!("expected int but got {val:?}")),
			Some(Err(e)) => Err(e),
			None => custom_err("value expected but none found"),
		}
	};
}

macro_rules! log_action {
	($level:expr, $val:expr) => {
		match $level.to_ascii_lowercase().as_str() {
			"trace" => trace!("jaq: {}", $val),
			"debug" => debug!("jaq: {}", $val),
			"info" => info!("jaq: {}", $val),
			"warn" => warn!("jaq: {}", $val),
			"error" => error!("jaq: {}", $val),
			_ => return_err!(custom_err("invalid log level")),
		}
	};
}

pub fn load_watchexec_defs(defs: &mut Definitions) -> miette::Result<()> {
	debug!("loading jaq watchexec library");

	trace!("jaq: add log filter");
	defs.insert_custom(
		"log",
		CustomFilter::with_update(
			1,
			|args, (ctx, val)| {
				let level = match string_arg!(args, 0, ctx, val) {
					Ok(v) => v,
					Err(e) => return_err!(Err(e)),
				};

				log_action!(level, val);

				// passthrough
				Box::new(once(Ok(val)))
			},
			|args, (ctx, val), _| {
				let level = match string_arg!(args, 0, ctx, val) {
					Ok(v) => v,
					Err(e) => return_err!(Err(e)),
				};

				log_action!(level, val);

				// passthrough
				Box::new(once(Ok(val)))
			},
		),
	);

	trace!("jaq: add stdout filter");
	defs.insert_custom(
		"stdout",
		CustomFilter::with_update(
			0,
			|_, (_, val)| {
				println!("{}", val);
				Box::new(once(Ok(val)))
			},
			|_, (_, val), _| {
				println!("{}", val);
				Box::new(once(Ok(val)))
			},
		),
	);

	trace!("jaq: add stderr filter");
	defs.insert_custom(
		"stderr",
		CustomFilter::with_update(
			0,
			|_, (_, val)| {
				eprintln!("{}", val);
				Box::new(once(Ok(val)))
			},
			|_, (_, val), _| {
				eprintln!("{}", val);
				Box::new(once(Ok(val)))
			},
		),
	);

	let kv: Arc<DashMap<String, Val>> = Arc::new(DashMap::new());

	trace!("jaq: add kv_clear filter");
	defs.insert_custom(
		"kv_clear",
		CustomFilter::new(0, {
			let kv = kv.clone();
			move |_, (_, val)| {
				kv.clear();
				Box::new(once(Ok(val)))
			}
		}),
	);

	trace!("jaq: add kv_store filter");
	defs.insert_custom(
		"kv_store",
		CustomFilter::new(1, {
			let kv = kv.clone();
			move |args, (ctx, val)| {
				let key = match string_arg!(args, 0, ctx, val) {
					Ok(v) => v,
					Err(e) => return_err!(Err(e)),
				};

				kv.insert(key, val.clone());
				Box::new(once(Ok(val)))
			}
		}),
	);

	trace!("jaq: add kv_fetch filter");
	defs.insert_custom(
		"kv_fetch",
		CustomFilter::new(1, {
			move |args, (ctx, val)| {
				let key = match string_arg!(args, 0, ctx, val) {
					Ok(v) => v,
					Err(e) => return_err!(Err(e)),
				};

				Box::new(once(Ok(kv
					.get(&key)
					.map(|val| val.clone())
					.unwrap_or(Val::Null))))
			}
		}),
	);

	trace!("jaq: add file_read filter");
	defs.insert_custom(
		"file_read",
		CustomFilter::new(1, {
			move |args, (ctx, val)| {
				let path = match &val {
					Val::Str(v) => v.to_string(),
					_ => return_err!(custom_err("expected string (path) but got {val:?}")),
				};

				let bytes = match int_arg!(args, 0, ctx, &val) {
					Ok(v) => v,
					Err(e) => return_err!(Err(e)),
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
	);

	trace!("jaq: add file_meta filter");
	defs.insert_custom(
		"file_meta",
		CustomFilter::new(0, {
			move |_, (_, val)| {
				let path = match &val {
					Val::Str(v) => v.to_string(),
					_ => return_err!(custom_err("expected string (path) but got {val:?}")),
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
	);

	trace!("jaq: add file_size filter");
	defs.insert_custom(
		"file_size",
		CustomFilter::new(0, {
			move |_, (_, val)| {
				let path = match &val {
					Val::Str(v) => v.to_string(),
					_ => return_err!(custom_err("expected string (path) but got {val:?}")),
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
	);

	trace!("jaq: add hash filter");
	defs.insert_custom(
		"hash",
		CustomFilter::new(0, {
			move |_, (_, val)| {
				let string = match &val {
					Val::Str(v) => v.to_string(),
					_ => return_err!(custom_err("expected string but got {val:?}")),
				};

				Box::new(once(Ok(Val::Str(
					blake3::hash(string.as_bytes()).to_hex().to_string().into(),
				))))
			}
		}),
	);

	trace!("jaq: add file_hash filter");
	defs.insert_custom(
		"file_hash",
		CustomFilter::new(0, {
			move |_, (_, val)| {
				let path = match &val {
					Val::Str(v) => v.to_string(),
					_ => return_err!(custom_err("expected string but got {val:?}")),
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
	);

	Ok(())
}

fn json_meta(meta: Metadata) -> Value {
	let perms = meta.permissions();
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
