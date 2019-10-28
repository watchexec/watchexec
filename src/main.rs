extern crate watchexec;

use watchexec::{cli, error, run};

fn main() -> error::Result<()> {
    run(&cli::get_args()?)
}
