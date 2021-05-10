use std::io::Write;

use watchexec::{error::Result, run};

mod args;

fn main() -> Result<()> {
    let (args, loglevel) = args::get_args()?;

    env_logger::Builder::new()
        .format(|buf, r| writeln!(buf, "*** {}", r.args()))
        .filter(None, loglevel)
        .init();

    run(args)
}
