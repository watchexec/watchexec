use std::io::Write;

use color_eyre::eyre::Result;
use watchexec::run;

mod args;

fn main() -> Result<()> {
    color_eyre::install()?;
    let (args, loglevel) = args::get_args()?;

    env_logger::Builder::new()
        .format(|buf, r| writeln!(buf, "*** {}", r.args()))
        .filter(None, loglevel)
        .init();

    run(args)?;
    Ok(())
}
