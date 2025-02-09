use std::{
	iter::once,
	sync::{Arc, OnceLock},
};

use dashmap::DashMap;
use jaq_core::Native;
use jaq_json::Val;
use jaq_std::{v, Filter};

use crate::filterer::syncval::SyncVal;

type KvStore = Arc<DashMap<String, SyncVal>>;
fn kv_store() -> KvStore {
	static KV_STORE: OnceLock<KvStore> = OnceLock::new();
	KV_STORE.get_or_init(KvStore::default).clone()
}

pub fn funs() -> [Filter<Native<jaq_json::Val>>; 3] {
	[
		(
			"kv_clear",
			v(0),
			Native::new({
				move |_, (_, val)| {
					let kv = kv_store();
					kv.clear();
					Box::new(once(Ok(val)))
				}
			}),
		),
		(
			"kv_store",
			v(1),
			Native::new({
				move |_, (mut ctx, val)| {
					let kv = kv_store();

					let key = ctx.pop_var().to_string();
					kv.insert(key, (&val).into());
					Box::new(once(Ok(val)))
				}
			}),
		),
		(
			"kv_fetch",
			v(1),
			Native::new({
				move |_, (mut ctx, _)| {
					let kv = kv_store();
					let key = ctx.pop_var().to_string();

					Box::new(once(Ok(kv
						.get(&key)
						.map_or(Val::Null, |val| val.value().into()))))
				}
			}),
		),
	]
}
