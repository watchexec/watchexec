use notify::op;
use std::path::{Path, PathBuf};

/// Info about a path and its corresponding `notify` event
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct PathOp {
    pub path: PathBuf,
    pub op: Option<op::Op>,
    pub cookie: Option<u32>,
}

impl PathOp {
    pub fn new(path: &Path, op: Option<op::Op>, cookie: Option<u32>) -> PathOp {
        PathOp {
            path: path.to_path_buf(),
            op,
            cookie,
        }
    }

    pub fn is_create(op_: op::Op) -> bool {
        op_.contains(op::CREATE)
    }

    pub fn is_remove(op_: op::Op) -> bool {
        op_.contains(op::REMOVE)
    }

    pub fn is_rename(op_: op::Op) -> bool {
        op_.contains(op::RENAME)
    }

    pub fn is_write(op_: op::Op) -> bool {
        let mut write_or_close_write = op::WRITE;
        write_or_close_write.toggle(op::CLOSE_WRITE);
        op_.intersects(write_or_close_write)
    }

    pub fn is_meta(op_: op::Op) -> bool {
        op_.contains(op::CHMOD)
    }
}
