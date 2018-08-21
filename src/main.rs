extern crate watchexec;
use watchexec::{cli, run};

fn main() {
    run(cli::get_args());
}
