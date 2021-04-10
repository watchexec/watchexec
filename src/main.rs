use std::io::Write;

extern crate watchexec;

use watchexec::{cli, error, run};

fn main() -> error::Result<()> {
    #[allow(deprecated)]
    let (args, loglevel) = cli::get_args()?;
    init_logger(loglevel);
    run(args)
}

fn init_logger(level: log::LevelFilter) {
    let mut log_builder = env_logger::Builder::new();

    log_builder
        .format(|buf, r| writeln!(buf, "*** {}", r.args()))
        .filter(None, level)
        .init();
}
