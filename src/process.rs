use std::path::PathBuf;

pub use self::imp::*;

#[cfg(target_family = "unix")]
mod imp {
    use libc::pid_t;
    use std::io::Result;
    use std::path::PathBuf;
    use std::process::Command;

    pub struct Process {
        pgid: pid_t,
    }

    impl Process {
        pub fn new(cmd: &str, updated_paths: Vec<PathBuf>) -> Result<Process> {
            use libc::exit;
            use nix::unistd::*;
            use std::io;
            use std::os::unix::process::CommandExt;

            let mut command = Command::new("sh");
            command.arg("-c").arg(cmd);

            if let Some(single_path) = super::get_single_updated_path(&updated_paths) {
                command.env("WATCHEXEC_UPDATED_PATH", single_path);
            }

            if let Some(common_path) = super::get_longest_common_path(&updated_paths) {
                command.env("WATCHEXEC_COMMON_PATH", common_path);
            }

            // Until process_exec lands in stable, handle fork/exec ourselves
            //command.before_exec(|| setpgid(0, 0).map_err(io::Error::from))
                //.spawn()
                //.and_then(|p| Ok(Process { pid: p.id() as i32 }))

            // Wait for child to call setpgid()
            // Else, we risk racing waitpid/killpg (mostly just in tests, but hey)
            let (r, w) = try!(pipe());

            match fork() {
                Ok(ForkResult::Parent {child, .. }) => {
                    let mut buffer = vec![0];
                    let _ = read(r, &mut buffer);
                    let _ = close(r);
                    let _ = close(w);

                    Ok(Process {
                        pgid: child
                    })
                },
                Ok(ForkResult::Child) => {
                    let _ = setpgid(0, 0);

                    let _ = write(w, &[42]);
                    let _ = close(w);
                    let _ = close(r);

                    let _ = command.exec();

                    // If we get here, there isn't much we can do
                    unsafe {
                        exit(1);
                    }
                }
                Err(e) => { Err(io::Error::from(e)) }
            }
        }

        pub fn kill(&self) {
            use libc::*;

            extern "C" {
                fn killpg(pgrp: pid_t, sig: c_int) -> c_int;
            }

            unsafe {
                killpg(self.pgid, SIGTERM);
            }
        }

        pub fn wait(&self) {
            use nix::sys::wait::waitpid;

            while let Ok(_) = waitpid(-self.pgid, None) {}
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

            command.spawn()
                .and_then(|p| {
                    let r = unsafe { AssignProcessToJobObject(job, p.into_raw_handle()) };
                    if r == 0 {
                        panic!("failed to add to job object: {}", last_err());
                    }

                    Ok(Process { job: job })
                })
        }

        pub fn kill(&self) {
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
    use std::path::{Path, PathBuf};

    use mktemp::Temp;

    use super::imp::Process;
    use super::get_longest_common_path;

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

        assert!(process.is_ok());
    }

    #[test]
    fn test_wait() {
        let file = Temp::new_file().unwrap();
        let path = file.to_path_buf();
        let process = Process::new(&format!("echo hi > {}", path.to_str().unwrap()), vec![])
            .unwrap();
        process.wait();

        assert!(file_contents(&path).starts_with("hi"));
    }

    #[test]
    fn test_kill() {
        let file = Temp::new_file().unwrap();
        let path = file.to_path_buf();

        let process = Process::new(&format!("sleep 20; echo hi > {}", path.to_str().unwrap()),
                                       vec![])
            .unwrap();
        process.kill();
        process.wait();

        assert!(file_contents(&path) == "");
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
