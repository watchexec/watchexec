use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::path::{Component, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

pub use self::imp::*;

#[cfg(target_family = "unix")]
mod imp {
    use std::io::Result;
    use std::path::PathBuf;
    use std::process::Command;

    pub struct Process {
        pid: i32,
        killed: bool,
    }

    impl Process {
        pub fn new(cmd: &str, updated_paths: Vec<PathBuf>) -> Result<Process> {
            use std::io;
            use std::os::unix::process::CommandExt;
            use nix::unistd::setpgid;

            let mut command = Command::new("sh");
            command.arg("-c").arg(cmd);

            if let Some(single_path) = super::get_single_updated_path(&updated_paths) {
                command.env("WATCHEXEC_UPDATED_PATH", single_path);
            }

            if let Some(common_path) = super::get_longest_common_path(&updated_paths) {
                command.env("WATCHEXEC_COMMON_PATH", common_path);
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
    use std::path::PathBuf;
    use std::process::Command;
    use kernel32::*;
    use winapi::*;

    pub struct Process {
        job: HANDLE,
        killed: bool,
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
    processes_tx: Sender<Process>,
}

impl ProcessReaper {
    pub fn new(tx: Sender<()>) -> ProcessReaper {
        let (processes_tx, processes_rx): (Sender<Process>, Receiver<Process>) = channel();

        thread::spawn(move || {
            loop {
                while let Ok(mut process) = processes_rx.recv() {
                    process.wait();

                    let _ = tx.send(());
                }
            }
        });

        ProcessReaper { processes_tx: processes_tx }
    }

    pub fn wait_process(&self, process: imp::Process) {
        let _ = self.processes_tx.send(process);
    }
}

fn get_single_updated_path(paths: &[PathBuf]) -> Option<&str> {
    paths.get(0).and_then(|p| p.to_str())
}

fn get_longest_common_path(paths: &[PathBuf]) -> Option<String> {
    struct TreeNode<'a> {
        value: Component<'a>,
        children: BTreeMap<Component<'a>, Rc<RefCell<TreeNode<'a>>>>,
    }

    match paths.len() {
        0 => return None,
        1 => return paths[0].to_str().map(|ref_val| ref_val.to_string()),
        _ => {}
    };

    // Step 1:
    // Build tree that contains each path component as a node value
    let tree = Rc::new(RefCell::new(TreeNode {
        value: Component::RootDir,
        children: BTreeMap::new(),
    }));

    for path in paths {
        let mut cur_node = tree.clone();

        for component in path.components() {
            if cur_node.borrow().value == component {
                continue;
            }

            let cur_clone = cur_node.clone();
            let mut borrowed = cur_clone.borrow_mut();

            cur_node = borrowed.children
                .entry(component)
                .or_insert_with(|| Rc::new(RefCell::new(TreeNode {
                    value: component,
                    children: BTreeMap::new(),
                })))
                .clone();
        }
    }

    // Step 2:
    // Navigate through tree until finding a divergence,
    // which indicates path is no longer common
    let mut queue = VecDeque::new();
    queue.push_back(tree.clone());

    let mut result = PathBuf::new();

    while let Some(node) = queue.pop_back() {
        let node = node.borrow();
        result.push(node.value.as_os_str());

        if node.children.len() > 1 {
            break;
        }

        for child in node.children.values() {
            queue.push_front(child.clone());
        }
    }

    result.to_str().map(|ref_val| ref_val.to_string())
}


#[cfg(test)]
#[cfg(target_family = "unix")]
mod tests {
    use std::path::{Path, PathBuf};
    use std::thread;
    use std::time::Duration;

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
