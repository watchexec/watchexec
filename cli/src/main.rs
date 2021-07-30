use std::io::Write;

use color_eyre::eyre::Result;
use watchexec::watch;

mod args;
mod handler;

fn main() -> Result<()> {
    color_eyre::install()?;
    let handler = args::get_args()?;

    env_logger::Builder::new()
        .format(|buf, r| writeln!(buf, "*** {}", r.args()))
        .filter(None, handler.log_level)
        .init();

    watch(&handler)?;
    Ok(())
}
