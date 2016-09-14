extern crate notify;
extern crate libc;

use std::io;
use std::io::Write;
use libc::system;
use notify::{Event, RecommendedWatcher, Watcher};
use std::ffi::CString;
use std::string::String;
use std::sync::mpsc::{channel, Receiver, RecvError};
use std::{thread, time};

fn clear() {
    let s = CString::new("clear").unwrap();
    unsafe {
        system(s.as_ptr());
    }

}

fn invoke(cmd: &String) {
    let s = CString::new(cmd.clone()).unwrap();
    unsafe {
      system(s.as_ptr());
    }
}

fn wait(rx: &Receiver<Event>) -> Result<Event, RecvError> {
    let e = try!(rx.recv());

    thread::sleep(time::Duration::from_millis(250));

    loop {
        match rx.try_recv() {
            Ok(_) => continue,
            Err(_) => break,
        }
    }

    Ok(e)
}

fn main() {
    let cmd = std::env::args().nth(1).expect("Argument 1 needs to be a command");

    let (tx, rx) = channel();
    let mut watcher: RecommendedWatcher = Watcher::new(tx)
        .expect("unable to create watcher");
    watcher.watch(".")
        .expect("unable to start watching directory");

    loop {
        //clear();
        let e = wait(&rx)
            .expect("error when waiting for filesystem changes");

        println!("{:?} {:?}", e.op, e.path);
        invoke(&cmd);
    }
}
