use threadpool::ThreadPool;

use std::process::{Child, Command};
use std::sync::mpsc::{Sender};

pub struct Process {
    process: Child,
    killed: bool
}

#[cfg(target_family = "unix")]
impl Process {
    pub fn new(cmd: &str, updated_paths: Vec<&str>) -> Option<Process>{
        use libc;
        use std::os::unix::process::CommandExt;

        let mut command = Command::new("sh");
        command.arg("-c").arg(cmd);

        if !updated_paths.is_empty() {
            command.env("WATCHEXEC_UPDATED_PATH", updated_paths[0]);
        }

        command.before_exec(|| unsafe {
            libc::setpgid(0, 0);
            Ok(())
        })
        .spawn()
        .ok()
        .and_then(|p| Some(Process { process: p, killed: false }))
    }

    pub fn kill(&mut self) {
        if self.killed {
            return;
        }

        use libc;

        extern "C" {
            fn killpg(pgrp: libc::pid_t, sig: libc::c_int) -> libc::c_int;
        }

        unsafe {
            killpg(self.process.id() as i32, libc::SIGTERM);
        }

        self.killed = true;
    }

    pub fn wait(&mut self) {
        use nix::sys::wait::waitpid;

        let pid = self.process.id() as i32;
        let _ = waitpid(-pid, None);
    }
}

#[cfg(target_family = "windows")]
impl Process {
    pub fn new(cmd: &str, updated_paths: Vec<&str>) -> Option<Process> {
        use std::os::windows::io::AsRawHandle;

        let mut command = Command::new("cmd.exe");
        command.arg("/C").arg(cmd);

        if !updated_paths.is_empty() {
            command.env("WATCHEXEC_UPDATED_PATH", updated_paths[0]);
        }

        command.spawn()
            .ok()
            .and_then(|p| { Some(Process { process: p, killed: false })})
    }

    pub fn kill(&mut self) {
        if self.killed {
            return;
        }

        self.process.kill();
        self.killed = true;
    }

    pub fn wait(&mut self) {
        self.process.wait();
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        self.kill();
    }
}

pub struct ProcessReaper {
    pool: ThreadPool,
    tx: Sender<()>,
}

impl ProcessReaper {
    pub fn new(tx: Sender<()>) -> ProcessReaper {
        ProcessReaper {
            pool: ThreadPool::new(1),
            tx: tx
        }
    }

    pub fn wait_process(&self, mut process: Process) {
        let tx = self.tx.clone();

        self.pool.execute(move || {
            process.wait();
            let _ = tx.send(());
        });
    }
}

#[cfg(test)]
#[cfg(target_family = "unix")]
mod tests {
    use std::path::Path;
    use std::thread;
    use std::time::Duration;

    use mktemp::Temp;

    use super::Process;

    fn file_contents(path: &Path) -> String {
        use std::fs::File;
        use std::io::Read;

        let mut f = File::open(path).unwrap();
        let mut s = String::new();
        f.read_to_string(&mut s).unwrap();

        s
    }

    #[test]
    fn test_start() {
        let process = Process::new("echo hi", vec![]);

        assert!(process.is_some());
    }

    #[test]
    fn test_wait() {
        let file = Temp::new_file().unwrap();
        let path = file.to_path_buf();
        let mut process = Process::new(&format!("echo hi > {}", path.to_str().unwrap()), vec![]).unwrap();
        process.wait();

        assert!(file_contents(&path).starts_with("hi"));
    }

    #[test]
    fn test_kill() {
        let file = Temp::new_file().unwrap();
        let path = file.to_path_buf();

        let mut process = Process::new(&format!("sleep 20; echo hi > {}", path.to_str().unwrap()), vec![]).unwrap();
        thread::sleep(Duration::from_millis(250));
        process.kill();
        process.wait();

        assert!(file_contents(&path) == "");
    }
}
