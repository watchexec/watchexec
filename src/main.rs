use std::io::Write;

extern crate watchexec;

use watchexec::{cli, error, run};

fn main() -> error::Result<()> {
    run(cli::get_args()?)
}

fn init_logger(debug: bool) {
    let mut log_builder = env_logger::Builder::new();
    let level = if debug {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Warn
    };

    log_builder
        .format(|buf, r| writeln!(buf, "*** {}", r.args()))
        .filter(None, level)
        .init();
}
