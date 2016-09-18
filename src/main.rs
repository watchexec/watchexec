extern crate clap;
extern crate libc;
extern crate notify;

use std::ffi::CString;
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, RecvError};
use std::{thread, time};
use std::process::Command;

use libc::system;
use clap::{App, Arg};
use notify::{Event, RecommendedWatcher, Watcher};

fn clear() {
    // TODO: determine better way to do this
    let clear_cmd;
    if cfg!(target_os = "windows") {
        clear_cmd = "cls";
    }
    else {
        clear_cmd = "clear";
    }

    let _ = Command::new(clear_cmd).status();
}

fn invoke(cmd: &str) {
    // TODO: determine a better way to get at system()
    let s = CString::new(cmd.clone()).unwrap();
    unsafe {
      system(s.as_ptr());
    }
}

fn ignored(_: &Path) -> bool {
    // TODO: ignore *.pyc files
    // TODO: handle .git directory?
    false
}

fn wait(rx: &Receiver<Event>) -> Result<Event, RecvError> {
    loop {
        // Block on initial notification
        let e = try!(rx.recv());
        if let Some(ref path) = e.path {
            if ignored(&path) {
                continue;
            }
        }

        // Accumulate subsequent events
        thread::sleep(time::Duration::from_millis(250));

        // Drain rx buffer and drop them
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
    let args = App::new("watchexec")
        .version("0.7")
        .about("Runs a command when any of the specified files/directories are modified")
        .arg(Arg::with_name("path")
            .help("Path to watch for changes")
            .required(true))
        .arg(Arg::with_name("command")
            .help("Command to run")
            .required(true))
        .arg(Arg::with_name("clear")
            .help("Clear screen before running command")
            .short("c")
            .long("clear"))
        .arg(Arg::with_name("debug")
             .help("Enable debug messages")
             .short("d")
             .long("debug"))
        .get_matches();

    let (tx, rx) = channel();
    let mut watcher: RecommendedWatcher = Watcher::new(tx).expect("unable to create watcher");

    // TODO: handle multiple paths
    let paths = args.values_of("path").unwrap();
    for path in paths {
        watcher.watch(path).expect("unable to watch path");
    }

    let cmd = args.value_of("command").unwrap();
    let need_clear = args.is_present("clear");
    let debug = args.is_present("debug");

    loop {
        let e = wait(&rx).expect("error when waiting for filesystem changes");

        if need_clear {
            clear();
        }

        if debug {
            println!("*** {:?}: {:?}", e.op, e.path);
        }

        invoke(cmd);
    }
}
