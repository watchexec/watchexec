use notify::{raw_watcher, PollWatcher, RecommendedWatcher, RecursiveMode};
use std::path::PathBuf;
use std::sync::mpsc::Sender;

/// Thin wrapper over the notify crate
///
/// `PollWatcher` and `RecommendedWatcher` are distinct types, but watchexec
/// really just wants to handle them without regard to the exact type
/// (e.g. polymorphically). This has the nice side effect of separating out
/// all coupling to the notify crate into this module.
pub struct Watcher {
    watcher_impl: WatcherImpl,
}

pub use notify::Error;
pub use notify::RawEvent as Event;

enum WatcherImpl {
    Recommended(RecommendedWatcher),
    Poll(PollWatcher),
}

impl Watcher {
    pub fn new(
        tx: Sender<Event>,
        paths: &[PathBuf],
        poll: bool,
        interval_ms: u32,
    ) -> Result<Self, Error> {
        use notify::Watcher;

        let imp = if poll {
            let mut watcher = PollWatcher::with_delay_ms(tx, interval_ms)?;
            for path in paths {
                watcher.watch(path, RecursiveMode::Recursive)?;
                debug!("Watching {:?}", path);
            }

            WatcherImpl::Poll(watcher)
        } else {
            let mut watcher = raw_watcher(tx)?;
            for path in paths {
                watcher.watch(path, RecursiveMode::Recursive)?;
                debug!("Watching {:?}", path);
            }

            WatcherImpl::Recommended(watcher)
        };

        Ok(Self { watcher_impl: imp })
    }

    pub fn is_polling(&self) -> bool {
        if let WatcherImpl::Poll(_) = self.watcher_impl {
            true
        } else {
            false
        }
    }
}
