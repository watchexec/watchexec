extern crate notify;
extern crate libc;

use std::env;
use libc::system;
use notify::{Event, RecommendedWatcher, Watcher};
use std::ffi::CString;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
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

fn ignored(relpath: &Path, ignores: &Vec<String>) -> bool {
    for i in ignores.iter() {
        if relpath.to_str().unwrap().starts_with(i) {
            //println!("Ignoring {} because {}", relpath.to_str().unwrap(), i);
            return true;
        }
    }

    false
}

fn invoke(cmd: &String) {
    let s = CString::new(cmd.clone()).unwrap();
    unsafe {
      system(s.as_ptr());
    }
}

fn read_gitignore(path: &str) -> Result<Vec<String>, std::io::Error>   {
    let f = try!(File::open(path));
    let reader = BufReader::new(f);

    let mut entries = vec![];
    for line in reader.lines() {
        let l = try!(line).trim().to_string();

        if l.starts_with("#") || l.len() == 0 {
            continue;
        }

        //println!("Read {}", l);
        entries.push(l);
    }

    Ok(entries)
}

fn wait(rx: &Receiver<Event>, cwd: &PathBuf, ignore: &Vec<String>) -> Result<Event, RecvError> {
    loop {
        let e = try!(rx.recv());

        let ignored = match e.path {
            Some(ref path)  => {
                let stripped = path.strip_prefix(cwd).unwrap();
                ignored(stripped, &ignore)
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

    let mut ignored = vec![];
    ignored.push(String::from("."));
    match read_gitignore(".gitignore") {
        Ok(gitignores)  => ignored.extend(gitignores),
        Err(_)          => ()
    }

    let (tx, rx) = channel();
    let mut watcher: RecommendedWatcher = Watcher::new(tx)
        .expect("unable to create watcher");
    watcher.watch(".")
        .expect("unable to start watching directory");

    loop {
        //clear();
        let e = wait(&rx, &cwd, &ignored)
            .expect("error when waiting for filesystem changes");

        //println!("{:?} {:?}", e.op, e.path);
        invoke(&cmd);
    }
}
