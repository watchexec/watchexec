use std::io::Write;

// until args.rs is removed from the lib
pub(crate) use watchexec::{config, error, run, Shell};

mod args;

fn main() -> error::Result<()> {
    #[allow(deprecated)]
    let (args, loglevel) = args::get_args()?;
    init_logger(loglevel);
    watchexec::run(args)
}

fn init_logger(level: log::LevelFilter) {
    let mut log_builder = env_logger::Builder::new();

    log_builder
        .format(|buf, r| writeln!(buf, "*** {}", r.args()))
        .filter(None, level)
        .init();
}
