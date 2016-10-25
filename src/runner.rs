use std::process::Command;
use std::sync::mpsc::{channel, Receiver};

use threadpool::ThreadPool;

pub struct Runner {
    pool: ThreadPool,
    process: Option<platform::Process>,
}

impl Runner {
    pub fn new() -> Runner {
        Runner {
            pool: ThreadPool::new(1),
            process: None,
        }
    }

    #[cfg(target_family = "windows")]
    pub fn clear_screen(&self) {
        let _ = Command::new("cls").status();
    }

    #[cfg(target_family = "unix")]
    pub fn clear_screen(&self) {
        let _ = Command::new("clear").status();
    }

    pub fn kill(&mut self) {
        if let Some(ref mut process) = self.process {
            process.kill();
        }
    }

    pub fn run_command(&mut self, cmd: &str, updated_paths: Vec<&str>) -> Receiver<()> {
        let (tx, rx) = channel();

        if let Some(mut process) = platform::Process::new(cmd, updated_paths) {
            self.process = Some(process.clone());

            self.pool.execute(move || {
                process.wait();

                let _ = tx.send(());
            });
        }

        rx
    }
}

impl Drop for Runner {
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
                Some(process) => Some(Process { child_pid: process.id() as i32 }),
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
