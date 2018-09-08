extern crate watchexec;
use watchexec::{cli, run};

fn main() -> run::Result<()> {
    run(cli::get_args())
}
