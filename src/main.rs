use std::io::Write;

extern crate watchexec;

use watchexec::{cli, error, run};

fn main() -> error::Result<()> {
    let args = cli::get_args()?;

    if args.debug {
        init_logger(log::LevelFilter::Debug);
    } else if args.changes {
        init_logger(log::LevelFilter::Info);
    } else {
        init_logger(log::LevelFilter::Warn);
    }

    run(args)
}

fn init_logger(level: log::LevelFilter) {
    let mut log_builder = env_logger::Builder::new();

    log_builder
        .format(|buf, r| writeln!(buf, "*** {}", r.args()))
        .filter(None, level)
        .init();
}
