/// Jaq's [Val](jaq_interpret::Val) uses Rc, but we want to use in Sync contexts. UGH!
use std::{rc::Rc, sync::Arc};

use indexmap::IndexMap;
use jaq_interpret::Val;

#[derive(Clone, Debug)]
pub enum SyncVal {
	Null,
	Bool(bool),
	Int(isize),
	Float(f64),
	Num(Arc<str>),
	Str(Arc<str>),
	Arr(Arc<[SyncVal]>),
	Obj(Arc<IndexMap<Arc<str>, SyncVal>>),
}

impl From<&Val> for SyncVal {
	fn from(val: &Val) -> Self {
		match val {
			Val::Null => Self::Null,
			Val::Bool(b) => Self::Bool(*b),
			Val::Int(i) => Self::Int(*i),
			Val::Float(f) => Self::Float(*f),
			Val::Num(s) => Self::Num(s.to_string().into()),
			Val::Str(s) => Self::Str(s.to_string().into()),
			Val::Arr(a) => Self::Arr({
				let mut arr = Vec::with_capacity(a.len());
				for v in a.iter() {
					arr.push(v.into());
				}
				arr.into()
			}),
			Val::Obj(m) => Self::Obj(Arc::new({
				let mut map = IndexMap::new();
				for (k, v) in m.iter() {
					map.insert(k.to_string().into(), v.into());
				}
				map
			})),
		}
	}
}

impl From<&SyncVal> for Val {
	fn from(val: &SyncVal) -> Self {
		match val {
			SyncVal::Null => Self::Null,
			SyncVal::Bool(b) => Self::Bool(*b),
			SyncVal::Int(i) => Self::Int(*i),
			SyncVal::Float(f) => Self::Float(*f),
			SyncVal::Num(s) => Self::Num(s.to_string().into()),
			SyncVal::Str(s) => Self::Str(s.to_string().into()),
			SyncVal::Arr(a) => Self::Arr({
				let mut arr = Vec::with_capacity(a.len());
				for v in a.iter() {
					arr.push(v.into());
				}
				arr.into()
			}),
			SyncVal::Obj(m) => Self::Obj(Rc::new({
				let mut map: IndexMap<_, _, ahash::RandomState> = Default::default();
				for (k, v) in m.iter() {
					map.insert(k.to_string().into(), v.into());
				}
				map
			})),
		}
	}
}
