use color_eyre::eyre;
use log::{warn, LevelFilter};
use notify_rust::Notification;
use watchexec::{
    config::Config,
    error::Result,
    pathop::PathOp,
    run::{ExecHandler, Handler},
};

pub struct CliHandler {
    pub inner: ExecHandler,
    pub log_level: LevelFilter,
    pub notify: bool,
}

impl CliHandler {
    pub fn new(config: Config, log_level: LevelFilter, notify: bool) -> eyre::Result<Self> {
        Ok(Self {
            inner: ExecHandler::new(config)?,
            log_level,
            notify,
        })
    }
}

impl Handler for CliHandler {
    fn args(&self) -> Config {
        self.inner.args()
    }

    fn on_manual(&self) -> Result<bool> {
        self.inner.on_manual()
    }

    fn on_update(&self, ops: &[PathOp]) -> Result<bool> {
        self.inner.on_update(ops).map(|o| {
            if self.notify {
                Notification::new()
                    .summary("Watchexec observed a change")
                    .body("Watchexec has seen a change, the command may have restarted.")
                    .show()
                    .map(drop)
                    .unwrap_or_else(|err| {
                        warn!("Failed to send desktop notification: {}", err);
                    });
            }

            o
        })
    }
}
