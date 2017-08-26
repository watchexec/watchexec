#[macro_use]
extern crate clap;
extern crate globset;
extern crate env_logger;
#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;
extern crate notify;

#[cfg(unix)]
extern crate nix;
#[cfg(windows)]
extern crate winapi;
#[cfg(windows)]
extern crate kernel32;

#[cfg(test)]
extern crate mktemp;

mod cli;
mod gitignore;
mod notification_filter;
mod process;
mod run;
mod signal;
mod watcher;

fn main() {
    let args = cli::get_args();
    run::run(args);
}
