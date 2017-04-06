use std::path::PathBuf;

pub fn spawn(cmd: &str, updated_paths: Vec<PathBuf>, no_shell: bool) -> Process {
    self::imp::Process::new(cmd, updated_paths, no_shell).expect("unable to spawn process")
}

pub use self::imp::Process;

#[cfg(target_family = "unix")]
mod imp {
    use libc::*;
    use std::io::Result;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::*;
    use signal::Signal;

    pub struct Process {
        pgid: pid_t,
        lock: Mutex<bool>,
        cvar: Condvar,
    }

    #[allow(unknown_lints)]
    #[allow(mutex_atomic)]
    impl Process {
        pub fn new(cmd: &str, updated_paths: Vec<PathBuf>, no_shell: bool) -> Result<Process> {
            use nix::unistd::*;
            use std::io;
            use std::os::unix::process::CommandExt;

            // Assemble command to run.
            // This is either the first argument from cmd (if no_shell was given) or "sh".
            // Using "sh -c" gives us features like supportin pipes and redirects,
            // but is a little less performant and can cause trouble when using custom signals
            // (e.g. --signal SIGHUP)
            let mut iter_args = cmd.split_whitespace();
            let arg0 = match no_shell {
                true => iter_args.next().unwrap(),
                false => "sh",
            };

            // TODO: There might be a better way of doing this with &str.
            //       I've had to fall back to String, as I wasn't able to join(" ") a Vec<&str>
            //       into a &str
            let args: Vec<String> = match no_shell {
                true => iter_args.map(str::to_string).collect(),
                false => vec!["-c".to_string(), iter_args.collect::<Vec<&str>>().join(" ")],
            };

            let mut command = Command::new(arg0);
            command.args(args);
            debug!("Assembled command {:?}", command);

            if let Some(single_path) = super::get_single_updated_path(&updated_paths) {
                command.env("WATCHEXEC_UPDATED_PATH", single_path);
            }

            if let Some(common_path) = super::get_longest_common_path(&updated_paths) {
                command.env("WATCHEXEC_COMMON_PATH", common_path);
            }

            command
                .before_exec(|| setpgid(0, 0).map_err(io::Error::from))
                .spawn()
                .and_then(|p| {
                              Ok(Process {
                                     pgid: p.id() as i32,
                                     lock: Mutex::new(false),
                                     cvar: Condvar::new(),
                                 })
                          })
        }

        pub fn reap(&self) {
            use nix::sys::wait::*;

            let mut finished = true;
            loop {
                match waitpid(-self.pgid, Some(WNOHANG)) {
                    Ok(WaitStatus::Exited(_, _)) |
                    Ok(WaitStatus::Signaled(_, _, _)) => finished = finished && true,
                    Ok(_) => {
                        finished = false;
                        break;
                    }
                    Err(_) => break,
                }
            }

            if finished {
                let mut done = self.lock.lock().unwrap();
                *done = true;
                self.cvar.notify_one();
            }
        }

        pub fn signal(&self, signal: Signal) {
            use signal::ConvertToLibc;

            let signo = signal.convert_to_libc();
            debug!("Sending {:?} (int: {}) to child process", signal, signo);
            self.c_signal(signo);
        }

        fn c_signal(&self, sig: c_int) {
            extern "C" {
                fn killpg(pgrp: pid_t, sig: c_int) -> c_int;
            }

            unsafe {
                killpg(self.pgid, sig);
            }

        }

        pub fn wait(&self) {
            let mut done = self.lock.lock().unwrap();
            while !*done {
                done = self.cvar.wait(done).unwrap();
            }
        }
    }
}

#[cfg(target_family = "windows")]
mod imp {
    use std::io;
    use std::io::Result;
    use std::mem;
    use std::path::PathBuf;
    use std::process::Command;
    use kernel32::*;
    use winapi::*;
    use signal::Signal;

    pub struct Process {
        job: HANDLE,
    }

    impl Process {
        pub fn new(cmd: &str, updated_paths: Vec<PathBuf>) -> Result<Process> {
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

            if let Some(single_path) = super::get_single_updated_path(&updated_paths) {
                command.env("WATCHEXEC_UPDATED_PATH", single_path);
            }

            if let Some(common_path) = super::get_longest_common_path(&updated_paths) {
                command.env("WATCHEXEC_COMMON_PATH", common_path);
            }

            command
                .spawn()
                .and_then(|p| {
                              let r = unsafe { AssignProcessToJobObject(job, p.into_raw_handle()) };
                              if r == 0 {
                                  panic!("failed to add to job object: {}", last_err());
                              }

                              Ok(Process { job: job })
                          })
        }

        pub fn reap(&self) {}

        pub fn signal(&self, signal: Signal) {
            debug!("Ignoring signal {:?} (not supported by Windows)", signal);
            unsafe {
                let _ = TerminateJobObject(self.job, 1);
            }
        }

        pub fn wait(&self) {
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
    unsafe impl Sync for Process {}
}

fn get_single_updated_path(paths: &[PathBuf]) -> Option<&str> {
    paths.get(0).and_then(|p| p.to_str())
}

fn get_longest_common_path(paths: &[PathBuf]) -> Option<String> {
    match paths.len() {
        0 => return None,
        1 => return paths[0].to_str().map(|ref_val| ref_val.to_string()),
        _ => {}
    };

    let mut longest_path: Vec<_> = paths[0].components().collect();

    for path in &paths[1..] {
        let mut greatest_distance = 0;
        for component_pair in path.components().zip(longest_path.iter()) {
            if component_pair.0 != *component_pair.1 {
                break;
            }

            greatest_distance += 1;
        }

        if greatest_distance != longest_path.len() {
            longest_path.truncate(greatest_distance);
        }
    }

    let mut result = PathBuf::new();
    for component in longest_path {
        result.push(component.as_os_str());
    }

    result.to_str().map(|ref_val| ref_val.to_string())
}


#[cfg(test)]
#[cfg(target_family = "unix")]
mod tests {
    use std::path::PathBuf;

    use super::spawn;
    use super::get_longest_common_path;

    #[test]
    fn test_start() {
        let _ = spawn("echo hi", vec![]);
    }

    #[test]
    fn longest_common_path_should_return_correct_value() {
        let single_path = vec![PathBuf::from("/tmp/random/")];
        let single_result = get_longest_common_path(&single_path).unwrap();
        assert_eq!(single_result, "/tmp/random/");

        let common_paths = vec![PathBuf::from("/tmp/logs/hi"),
                                PathBuf::from("/tmp/logs/bye"),
                                PathBuf::from("/tmp/logs/bye"),
                                PathBuf::from("/tmp/logs/fly")];

        let common_result = get_longest_common_path(&common_paths).unwrap();
        assert_eq!(common_result, "/tmp/logs");


        let diverging_paths = vec![PathBuf::from("/tmp/logs/hi"), PathBuf::from("/var/logs/hi")];

        let diverging_result = get_longest_common_path(&diverging_paths).unwrap();
        assert_eq!(diverging_result, "/");

        let uneven_paths = vec![PathBuf::from("/tmp/logs/hi"),
                                PathBuf::from("/tmp/logs/"),
                                PathBuf::from("/tmp/logs/bye")];

        let uneven_result = get_longest_common_path(&uneven_paths).unwrap();
        assert_eq!(uneven_result, "/tmp/logs");
    }
}
