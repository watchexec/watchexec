use std::process::Command;
use std::sync::mpsc::{channel, Receiver, Sender};

use threadpool::ThreadPool;

pub struct Runner {
    pool: ThreadPool,
    tx: Sender<()>,
}

impl Runner {
    pub fn new() -> (Runner, Receiver<()>) {
        let (tx, rx) = channel();
        (Runner {
            pool: ThreadPool::new(1),
            tx: tx,
        },
         rx)
    }

    #[cfg(target_family = "windows")]
    pub fn clear_screen(&self) {
        let _ = Command::new("cls").status();
    }

    #[cfg(target_family = "unix")]
    pub fn clear_screen(&self) {
        let _ = Command::new("clear").status();
    }

    pub fn run_command(&mut self,
                       cmd: &str,
                       updated_paths: Vec<&str>)
                       -> Option<Process> {
        let child = Process::new(cmd, updated_paths);

        if let Some(ref process) = child {
            let tx = self.tx.clone();
            let mut p = process.as_platform_process();

            self.pool.execute(move || {
                p.wait();

                let _ = tx.send(());
            });
        }

        child
    }
}

pub struct Process {
    process: platform::Process
}

impl Process {
    pub fn new(cmd: &str, updated_paths: Vec<&str>) -> Option<Process> {
        platform::Process::new(cmd, updated_paths).and_then(|p| {
            Some(Process { process: p })
        })
    }

    fn as_platform_process(&self) -> platform::Process {
        self.process.clone()
    }

    pub fn kill(&mut self) {
        self.process.kill();
    }

    #[allow(dead_code)]
    pub fn wait(&mut self) {
        self.process.wait();
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        self.kill();
    }
}

#[cfg(target_family = "unix")]
mod platform {
    use std::process::Command;

    #[derive(Clone)]
    pub struct Process {
        child_pid: i32,
    }

    #[cfg(target_family = "unix")]
    impl Process {
        pub fn new(cmd: &str, updated_paths: Vec<&str>) -> Option<Process> {
            use libc;
            use std::os::unix::process::CommandExt;

            let mut command = Command::new("sh");
            command.arg("-c").arg(cmd);

            if !updated_paths.is_empty() {
                command.env("WATCHEXEC_UPDATED_PATH", updated_paths[0]);
            }

            let c = command.before_exec(|| unsafe {
                    libc::setpgid(0, 0);
                    Ok(())
                })
                .spawn()
                .ok();

            match c {
                Some(process) => {
                    Some(Process {
                        child_pid: process.id() as i32,
                    })
                }
                None => None,
            }
        }

        pub fn kill(&mut self) {
            use libc;

            extern "C" {
                fn killpg(pgrp: libc::pid_t, sig: libc::c_int) -> libc::c_int;
            }

            unsafe {
                killpg(self.child_pid, libc::SIGTERM);
            }
        }

        pub fn wait(&mut self) {
            use nix::sys::wait::waitpid;

            let _ = waitpid(-self.child_pid, None);
        }
    }
}

#[cfg(target_family = "windows")]
mod platform {
    use std::process::Command;
    use winapi::winnt::HANDLE;

    #[derive(Clone)]
    pub struct Process {
        child_handle: HANDLE,
    }

    unsafe impl Send for Process {}

    #[cfg(target_family = "windows")]
    impl Process {
        pub fn new(cmd: &str, updated_paths: Vec<&str>) -> Option<Process> {
            use std::os::windows::io::AsRawHandle;

            let mut command = Command::new("cmd.exe");
            command.arg("/C").arg(cmd);

            if !updated_paths.is_empty() {
                command.env("WATCHEXEC_UPDATED_PATH", updated_paths[0]);
            }

            match command.spawn().ok() {
                Some(process) => Some(Process { child_handle: process.as_raw_handle() }),
                None => None,
            }
        }

        pub fn kill(&mut self) {
            use kernel32::TerminateProcess;

            unsafe {
                let _ = TerminateProcess(self.child_handle, 0);
            }
        }

        pub fn wait(&mut self) {
            use kernel32::WaitForSingleObject;
            use winapi::winbase::INFINITE;

            unsafe {
                let _ = WaitForSingleObject(self.child_handle, INFINITE);
            }
        }
    }
}
