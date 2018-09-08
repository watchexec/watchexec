extern crate watchexec;
use watchexec::{cli, run};
use std::error::Error;

fn main() -> Result<(), Box<Error>> {
    run(cli::get_args())
}
