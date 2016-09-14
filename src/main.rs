extern crate notify;
extern crate libc;

use libc::system;
use notify::{RecommendedWatcher, Watcher};
use notify::RecursiveMode;
use std::ffi::CString;
use std::string::String;
use std::sync::mpsc::channel;

fn invoke(cmd: &String) {
    let s = CString::new(cmd.clone()).unwrap();
    unsafe {
      system(s.as_ptr());
    }
}

fn main() {
    let cmd = std::env::args().nth(1).expect("Argument 1 needs to be a command");

    let (tx, rx) = channel();
    let mut watcher: RecommendedWatcher = Watcher::new(tx)
        .expect("unable to create watcher");
    watcher.watch(".", RecursiveMode::Recursive)
        .expect("unable to start watching directory");

    loop {
        match rx.recv() {
            Ok(notify::Event{ path: Some(path), op:Ok(op), cookie:_ }) => {
                println!("{:?} {:?}", op, path);
                invoke(&cmd);
            },
            Ok(event) => println!("broken event: {:?}", event),
            Err(e) => println!("watch error: {}", e),
        }
    }
}
