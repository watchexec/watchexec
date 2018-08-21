extern crate watchexec;
use watchexec::{cli::get_args, run};

fn main() {
    run(get_args());
}
