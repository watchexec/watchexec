use std::path::Path;
use std::sync::mpsc::Sender;

use notify::{PollWatcher, RecommendedWatcher, RecursiveMode, raw_watcher};

/// Thin wrapper over the notify crate
///
/// `PollWatcher` and `RecommendedWatcher` are distinct types, but watchexec
/// really just wants to handle them without regard to the exact type
/// (e.g. polymorphically). This has the nice side effect of separating out
/// all coupling to the notify crate into this module.
pub struct Watcher {
    watcher_impl: WatcherImpl,
}

pub use notify::RawEvent as Event;
pub use notify::Error;

enum WatcherImpl {
    Recommended(RecommendedWatcher),
    Poll(PollWatcher),
}

impl Watcher {
    pub fn new(tx: Sender<Event>, poll: bool, interval_ms: u32) -> Result<Watcher, Error> {
        let imp = if poll {
            WatcherImpl::Poll(try!(PollWatcher::with_delay_ms(tx, interval_ms)))
        } else {
            WatcherImpl::Recommended(try!(raw_watcher(tx)))
        };

        Ok(Watcher { watcher_impl: imp })
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
            WatcherImpl::Recommended(ref mut w) => w.watch(path, RecursiveMode::Recursive),
            WatcherImpl::Poll(ref mut w) => w.watch(path, RecursiveMode::Recursive),
        }
    }
}
