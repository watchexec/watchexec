use std::path::Path;
use std::sync::mpsc::Sender;

use notify::{PollWatcher, RecommendedWatcher};

/// Thin wrapper over the notify crate
///
/// `PollWatcher` and `RecommendedWatcher` are distinct types, but watchexec
/// really just wants to handle them without regard to the exact type
/// (e.g. polymorphically). This has the nice side effect of separating out
/// all coupling to the notify crate into this module.
pub struct Watcher {
    watcher_impl: WatcherImpl
}

pub use notify::Event;
pub use notify::Error;

enum WatcherImpl {
    Recommended(RecommendedWatcher),
    Poll(PollWatcher)
}

impl Watcher {
    pub fn new(tx: Sender<Event>, poll: bool, interval_ms: u32) -> Result<Watcher, Error> {
        use notify::Watcher;

        let imp = if poll {
            WatcherImpl::Poll(try!(PollWatcher::with_delay(tx, interval_ms)))
        } else {
            WatcherImpl::Recommended(try!(RecommendedWatcher::new(tx)))
        };

        Ok(self::Watcher {
            watcher_impl: imp
        })
    }

    pub fn is_polling(&self) -> bool {
        if let WatcherImpl::Poll(_) = self.watcher_impl {
            true
        } else {
            false
        }
    }

    pub fn watch<P: AsRef<Path>>(&mut self, path: P) -> Result<(), Error> {
        use notify::Watcher;

        match self.watcher_impl {
            WatcherImpl::Recommended(ref mut w) => w.watch(path),
            WatcherImpl::Poll(ref mut w)        => w.watch(path)
        }
    }
}
