extern crate notify;
extern crate libc;

use std::env;
use libc::system;
use notify::{Event, RecommendedWatcher, Watcher};
use std::ffi::CString;
use std::path::{Path,PathBuf};
use std::string::String;
use std::sync::mpsc::{channel, Receiver, RecvError};
use std::{thread, time};

fn clear() {
    let s = CString::new("clear").unwrap();
    unsafe {
        system(s.as_ptr());
    }

}

fn ignored(relpath: &Path) -> bool {
    if relpath.to_str().unwrap().starts_with(".") {
        return true;
    }

    false
}

fn invoke(cmd: &String) {
    let s = CString::new(cmd.clone()).unwrap();
    unsafe {
      system(s.as_ptr());
    }
}

fn wait(rx: &Receiver<Event>, cwd: &PathBuf) -> Result<Event, RecvError> {
    loop {
        let e = try!(rx.recv());

        let ignored = match e.path {
            Some(ref path)  => {
                let stripped = path.strip_prefix(cwd).unwrap();
                ignored(stripped)
            },
            None        => false
        };

        if ignored {
            continue;
        }

        thread::sleep(time::Duration::from_millis(250));
        loop {
            match rx.try_recv() {
                Ok(_) => continue,
                Err(_) => break,
            }
        }

        return Ok(e);
    }
}

fn main() {
    let cmd = env::args().nth(1).expect("Argument 1 needs to be a command");
    let cwd = env::current_dir().unwrap();

    let (tx, rx) = channel();
    let mut watcher: RecommendedWatcher = Watcher::new(tx)
        .expect("unable to create watcher");
    watcher.watch(".")
        .expect("unable to start watching directory");

    loop {
        clear();
        let e = wait(&rx, &cwd)
            .expect("error when waiting for filesystem changes");

        println!("{:?} {:?}", e.op, e.path);
        invoke(&cmd);
    }
}
