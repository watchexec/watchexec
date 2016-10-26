use threadpool::ThreadPool;

use std::sync::mpsc::Sender;

pub use self::imp::*;

#[cfg(target_family = "unix")]
mod imp {
    use std::io::Result;
    use std::process::Command;

    pub struct Process {
        pid: i32,
        killed: bool,
    }

    impl Process {
        pub fn new(cmd: &str, updated_paths: Vec<&str>) -> Result<Process> {
            use std::io;
            use std::os::unix::process::CommandExt;
            use nix::unistd::setpgid;

            let mut command = Command::new("sh");
            command.arg("-c").arg(cmd);

            if !updated_paths.is_empty() {
                command.env("WATCHEXEC_UPDATED_PATH", updated_paths[0]);
            }

            command.before_exec(|| setpgid(0, 0).map_err(io::Error::from))
                .spawn()
                .and_then(|p| {
                    Ok(Process {
                        pid: p.id() as i32,
                        killed: false,
                    })
                })
        }

        pub fn kill(&mut self) {
            use libc;

            if self.killed {
                return;
            }

            extern "C" {
                fn killpg(pgrp: libc::pid_t, sig: libc::c_int) -> libc::c_int;
            }

            unsafe {
                killpg(self.pid, libc::SIGTERM);
            }

            self.killed = true;
        }

        pub fn wait(&mut self) {
            use nix::sys::wait::waitpid;

            let _ = waitpid(-self.pid, None);
        }
    }

    impl Drop for Process {
        fn drop(&mut self) {
            self.kill();
        }
    }
}

#[cfg(target_family = "windows")]
mod imp {
    use std::io;
    use std::io::Result;
    use std::mem;
    use std::process::Command;
    use kernel32::*;
    use winapi::*;

    pub struct Process {
        job: HANDLE,
        killed: bool,
    }

    impl Process {
        pub fn new(cmd: &str, updated_paths: Vec<&str>) -> Result<Process> {
            use std::os::windows::io::IntoRawHandle;

            fn last_err() -> io::Error {
                io::Error::last_os_error()
            }

            let job = unsafe { CreateJobObjectW(0 as *mut _, 0 as *const _) };
            if job.is_null() {
                panic!("failed to create job object: {}", last_err());
            }

            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { mem::zeroed() };
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let r = unsafe {
                SetInformationJobObject(job,
                                        JobObjectExtendedLimitInformation,
                                        &mut info as *mut _ as LPVOID,
                                        mem::size_of_val(&info) as DWORD)
            };
            if r == 0 {
                panic!("failed to set job info: {}", last_err());
            }

            let mut command = Command::new("cmd.exe");
            command.arg("/C").arg(cmd);

            if !updated_paths.is_empty() {
                command.env("WATCHEXEC_UPDATED_PATH", updated_paths[0]);
            }

            command.spawn()
                .and_then(|p| {
                    let r = unsafe { AssignProcessToJobObject(job, p.into_raw_handle()) };
                    if r == 0 {
                        panic!("failed to add to job object: {}", last_err());
                    }

                    Ok(Process {
                        job: job,
                        killed: false,
                    })
                })
        }

        pub fn kill(&mut self) {
            if self.killed {
                return;
            }

            unsafe {
                let _ = TerminateJobObject(self.job, 1);
            }

            self.killed = true;
        }

        pub fn wait(&mut self) {
            unsafe {
                let _ = WaitForSingleObject(self.job, INFINITE);
            }
        }
    }

    impl Drop for Process {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.job);
            }
        }
    }

    unsafe impl Send for Process {}
}

/// Watches for child process death, notifying callers via a channel.
///
/// On Windows, we don't have SIGCHLD, and even if we did, we'd still need
/// to relay that over a channel.
pub struct ProcessReaper {
    pool: ThreadPool,
    tx: Sender<()>,
}

impl ProcessReaper {
    pub fn new(tx: Sender<()>) -> ProcessReaper {
        ProcessReaper {
            pool: ThreadPool::new(1),
            tx: tx,
        }
    }

    pub fn wait_process(&self, mut process: imp::Process) {
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

    use super::imp::Process;

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
        let mut process = Process::new(&format!("echo hi > {}", path.to_str().unwrap()), vec![])
            .unwrap();
        process.wait();

        assert!(file_contents(&path).starts_with("hi"));
    }

    #[test]
    fn test_kill() {
        let file = Temp::new_file().unwrap();
        let path = file.to_path_buf();

        let mut process = Process::new(&format!("sleep 20; echo hi > {}", path.to_str().unwrap()),
                                       vec![])
            .unwrap();
        thread::sleep(Duration::from_millis(250));
        process.kill();
        process.wait();

        assert!(file_contents(&path) == "");
    }
}
