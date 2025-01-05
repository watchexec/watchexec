use std::{iter::once, sync::Arc};

use dashmap::DashMap;
use jaq_interpret::{Error, Native, ParseCtx, Val};
use once_cell::sync::OnceCell;
use tracing::trace;

use crate::filterer::syncval::SyncVal;

use super::macros::{return_err, string_arg};

type KvStore = Arc<DashMap<String, SyncVal>>;
fn kv_store() -> KvStore {
	static KV_STORE: OnceCell<KvStore> = OnceCell::new();
	KV_STORE.get_or_init(KvStore::default).clone()
}

pub fn load(jaq: &mut ParseCtx) {
	trace!("jaq: add kv_clear filter");
	jaq.insert_native(
		"kv_clear".into(),
		0,
		Native::new({
			move |_, (_, val)| {
				let kv = kv_store();
				kv.clear();
				Box::new(once(Ok(val)))
			}
		}),
	);

	trace!("jaq: add kv_store filter");
	jaq.insert_native(
		"kv_store".into(),
		1,
		Native::new({
			move |args, (ctx, val)| {
				let kv = kv_store();
				let key = match string_arg!(args, 0, ctx, val) {
					Ok(v) => v,
					Err(e) => return_err!(Err(e)),
				};

				kv.insert(key, (&val).into());
				Box::new(once(Ok(val)))
			}
		}),
	);

	trace!("jaq: add kv_fetch filter");
	jaq.insert_native(
		"kv_fetch".into(),
		1,
		Native::new({
			move |args, (ctx, val)| {
				let kv = kv_store();
				let key = match string_arg!(args, 0, ctx, val) {
					Ok(v) => v,
					Err(e) => return_err!(Err(e)),
				};

				Box::new(once(Ok(kv
					.get(&key)
					.map_or(Val::Null, |val| val.value().into()))))
			}
		}),
	);
}
