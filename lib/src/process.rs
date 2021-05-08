#![allow(unsafe_code)]

use crate::error::Result;
use crate::pathop::PathOp;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    process::Command,
};

/// Shell to use to run commands.
///
/// `Cmd` and `Powershell` are special-cased because they have different calling
/// conventions. Also `Cmd` is only available in Windows, while `Powershell` is
/// also available on unices (provided the end-user has it installed, of course).
///
/// See [`Config.cmd`][crate::config::Config] for the semantics of `None` vs the
/// other options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shell {
    /// Use no shell, and execute the command directly.
    None,

    /// Use the given string as a unix shell invocation.
    ///
    /// This means two things:
    /// - the program is invoked with `-c` followed by the command, and
    /// - the string will be split on space, and the resulting vec used as
    ///   execvp(3) arguments: first is the shell program, rest are additional
    ///   arguments (which come before the `-c` mentioned above). This is a very
    ///   simplistic approach deliberately: it will not support quoted
    ///   arguments, for example. Use [`Shell::None`] with a custom command vec
    ///   if you want that.
    Unix(String),

    /// Use the Windows CMD.EXE shell.
    ///
    /// This is invoked with `/C` followed by the command.
    #[cfg(windows)]
    Cmd,

    /// Use Powershell, on Windows or elsewhere.
    ///
    /// This is invoked with `-Command` followed by the command.
    ///
    /// This is preferred over `Unix("pwsh")`, though that will also work
    /// on unices due to Powershell supporting the `-c` short option.
    Powershell,
}

impl Default for Shell {
    #[cfg(windows)]
    fn default() -> Self {
        Self::Powershell
    }

    #[cfg(not(windows))]
    fn default() -> Self {
        Self::Unix("sh".into())
    }
}

impl Shell {
    /// Obtain a [`Command`] given the cmd vec from [`Config`][crate::config::Config].
    ///
    /// Behaves as described in the enum documentation.
    ///
    /// # Panics
    ///
    /// - Panics if `cmd` is empty.
    /// - Panics if the string in the `Unix` variant is empty or only whitespace.
    pub fn to_command(&self, cmd: &[String]) -> Command {
        assert!(!cmd.is_empty(), "cmd was empty");

        match self {
            Shell::None => {
                // UNWRAP: checked by assert
                #[allow(clippy::unwrap_used)]
                let (first, rest) = cmd.split_first().unwrap();
                let mut c = Command::new(first);
                c.args(rest);
                c
            }

            #[cfg(windows)]
            Shell::Cmd => {
                let mut c = Command::new("cmd.exe");
                c.arg("/C").arg(cmd.join(" "));
                c
            }

            Shell::Powershell if cfg!(windows) => {
                let mut c = Command::new("powershell.exe");
                c.arg("-Command").arg(cmd.join(" "));
                c
            }

            Shell::Powershell => {
                let mut c = Command::new("pwsh");
                c.arg("-Command").arg(cmd.join(" "));
                c
            }

            Shell::Unix(name) => {
                assert!(!name.is_empty(), "shell program was empty");
                let sh = name.split_ascii_whitespace().collect::<Vec<_>>();

                // UNWRAP: checked by assert
                #[allow(clippy::unwrap_used)]
                let (shprog, shopts) = sh.split_first().unwrap();

                let mut c = Command::new(shprog);
                c.args(shopts);
                c.arg("-c").arg(cmd.join(" "));
                c
            }
        }
    }
}

pub fn spawn(
    cmd: &[String],
    updated_paths: &[PathOp],
    shell: Shell,
    environment: bool,
) -> Result<Process> {
    self::imp::Process::new(cmd, updated_paths, shell, environment).map_err(|e| e.into())
}

pub use self::imp::Process;

#[cfg(target_family = "unix")]
mod imp {
    use super::Shell;
    use crate::pathop::PathOp;
    use crate::signal::Signal;
    use nix::libc::*;
    use nix::{self, Error};
    use std::io::{self, Result};
    use std::sync::*;

    pub struct Process {
        pgid: pid_t,
        lock: Mutex<bool>,
        cvar: Condvar,
    }

    fn from_nix_error(err: nix::Error) -> io::Error {
        match err {
            Error::Sys(errno) => io::Error::from_raw_os_error(errno as i32),
            Error::InvalidPath => io::Error::new(io::ErrorKind::InvalidInput, err),
            _ => io::Error::new(io::ErrorKind::Other, err),
        }
    }

    #[allow(clippy::mutex_atomic)]
    impl Process {
        pub fn new(
            cmd: &[String],
            updated_paths: &[PathOp],
            shell: Shell,
            environment: bool,
        ) -> Result<Self> {
            use nix::unistd::*;
            use std::convert::TryInto;
            use std::os::unix::process::CommandExt;

            let mut command = shell.to_command(&cmd);
            debug!("Assembled command {:?}", command);

            let command_envs = if !environment {
                Vec::new()
            } else {
                super::collect_path_env_vars(updated_paths)
            };

            for &(ref name, ref val) in &command_envs {
                command.env(name, val);
            }

            unsafe {
                command.pre_exec(|| setsid().map_err(from_nix_error).map(|_| ()));
            }
            command.spawn().map(|p| Self {
                pgid: p
                    .id()
                    .try_into()
                    .expect("u32 -> i32 failed in process::new"),
                lock: Mutex::new(false),
                cvar: Condvar::new(),
            })
        }

        pub fn reap(&self) {
            use nix::sys::wait::*;
            use nix::unistd::Pid;

            let mut finished = true;
            loop {
                match waitpid(Pid::from_raw(-self.pgid), Some(WaitPidFlag::WNOHANG)) {
                    Ok(WaitStatus::Exited(_, _)) | Ok(WaitStatus::Signaled(_, _, _)) => {}
                    Ok(_) => {
                        finished = false;
                        break;
                    }
                    Err(_) => break,
                }
            }

            if finished {
                let mut done = self.lock.lock().expect("poisoned lock in process::reap");
                *done = true;
                self.cvar.notify_one();
            }
        }

        pub fn signal(&self, signal: Signal) {
            use crate::signal::ConvertToLibc;

            let signo = signal.convert_to_libc();
            debug!("Sending {:?} (int: {}) to the command", signal, signo);
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
            let mut done = self.lock.lock().expect("poisoned lock in process::wait");
            while !*done {
                done = self
                    .cvar
                    .wait(done)
                    .expect("poisoned cvar in process::wait");
            }
        }
    }
}

#[cfg(target_family = "windows")]
mod imp {
    use super::Shell;
    use crate::pathop::PathOp;
    use crate::signal::Signal;
    use std::{
        convert::TryInto,
        io::{self, Result},
        mem,
        os::windows::{io::IntoRawHandle, process::CommandExt},
        ptr,
    };
    use winapi::{
        shared::{
            basetsd::ULONG_PTR,
            minwindef::{DWORD, LPVOID},
        },
        um::{
            handleapi::{CloseHandle, INVALID_HANDLE_VALUE},
            ioapiset::{CreateIoCompletionPort, GetQueuedCompletionStatus},
            jobapi2::{
                AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject,
                TerminateJobObject,
            },
            minwinbase::LPOVERLAPPED,
            processthreadsapi::{GetProcessId, OpenThread, ResumeThread},
            tlhelp32::{
                CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD,
                THREADENTRY32,
            },
            winbase::{CREATE_SUSPENDED, INFINITE},
            winnt::{
                JobObjectAssociateCompletionPortInformation, JobObjectExtendedLimitInformation,
                HANDLE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO, PVOID,
            },
        },
    };

    pub struct Process {
        job: HANDLE,
        completion_port: HANDLE,
    }

    #[repr(C)]
    struct JOBOBJECT_ASSOCIATE_COMPLETION_PORT {
        completion_key: PVOID,
        completion_port: HANDLE,
    }

    impl Process {
        pub fn new(
            cmd: &[String],
            updated_paths: &[PathOp],
            shell: Shell,
            environment: bool,
        ) -> Result<Self> {
            fn last_err() -> io::Error {
                io::Error::last_os_error()
            }

            let job = unsafe { CreateJobObjectW(ptr::null_mut(), ptr::null()) };
            if job.is_null() {
                panic!("failed to create job object: {}", last_err());
            }

            let completion_port =
                unsafe { CreateIoCompletionPort(INVALID_HANDLE_VALUE, ptr::null_mut(), 0, 1) };
            if job.is_null() {
                panic!(
                    "unable to create IO completion port for job: {}",
                    last_err()
                );
            }

            let mut associate_completion: JOBOBJECT_ASSOCIATE_COMPLETION_PORT =
                unsafe { mem::zeroed() };
            associate_completion.completion_key = job;
            associate_completion.completion_port = completion_port;
            unsafe {
                let r = SetInformationJobObject(
                    job,
                    JobObjectAssociateCompletionPortInformation,
                    &mut associate_completion as *mut _ as LPVOID,
                    mem::size_of_val(&associate_completion)
                        .try_into()
                        .expect("cannot safely cast to DWORD"),
                );
                if r == 0 {
                    panic!(
                        "failed to associate completion port with job: {}",
                        last_err()
                    );
                }
            }

            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { mem::zeroed() };
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let r = unsafe {
                SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    &mut info as *mut _ as LPVOID,
                    mem::size_of_val(&info)
                        .try_into()
                        .expect("cannot safely cast to DWORD"),
                )
            };
            if r == 0 {
                panic!("failed to set job info: {}", last_err());
            }

            let mut command = shell.to_command(&cmd);
            command.creation_flags(CREATE_SUSPENDED);
            debug!("Assembled command {:?}", command);

            let command_envs = if !environment {
                Vec::new()
            } else {
                super::collect_path_env_vars(updated_paths)
            };

            for &(ref name, ref val) in &command_envs {
                command.env(name, val);
            }

            command.spawn().and_then(|p| {
                let handle = p.into_raw_handle();
                let r = unsafe { AssignProcessToJobObject(job, handle) };
                if r == 0 {
                    panic!("failed to add to job object: {}", last_err());
                }

                resume_threads(handle);

                Ok(Self {
                    job,
                    completion_port,
                })
            })
        }

        pub const fn reap(&self) {}

        pub fn signal(&self, _signal: Signal) {
            unsafe {
                let _ = TerminateJobObject(self.job, 1);
            }
        }

        pub fn wait(&self) {
            loop {
                let mut code: DWORD = 0;
                let mut key: ULONG_PTR = 0;
                let mut overlapped = mem::MaybeUninit::<LPOVERLAPPED>::uninit();
                unsafe {
                    GetQueuedCompletionStatus(
                        self.completion_port,
                        &mut code,
                        &mut key,
                        overlapped.as_mut_ptr(),
                        INFINITE,
                    );
                }

                if code == JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO && (key as HANDLE) == self.job {
                    break;
                }
            }
        }
    }

    impl Drop for Process {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.job);
                let _ = CloseHandle(self.completion_port);
            }
        }
    }

    unsafe impl Send for Process {}
    unsafe impl Sync for Process {}

    // This is pretty terrible, but it's either this or we re-implement all of Rust's std::process just to get at PROCESS_INFORMATION
    fn resume_threads(child_process: HANDLE) {
        unsafe {
            let child_id = GetProcessId(child_process);

            let h = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
            let mut entry = THREADENTRY32 {
                dwSize: 28,
                cntUsage: 0,
                th32ThreadID: 0,
                th32OwnerProcessID: 0,
                tpBasePri: 0,
                tpDeltaPri: 0,
                dwFlags: 0,
            };

            let mut result = Thread32First(h, &mut entry);
            while result != 0 {
                if entry.th32OwnerProcessID == child_id {
                    let thread_handle = OpenThread(0x0002, 0, entry.th32ThreadID);
                    ResumeThread(thread_handle);
                    CloseHandle(thread_handle);
                }

                result = Thread32Next(h, &mut entry);
            }

            CloseHandle(h);
        }
    }
}

/// Collect `PathOp` details into op-categories to pass onto the exec'd command as env-vars
///
/// `WRITTEN` -> `notify::ops::WRITE`, `notify::ops::CLOSE_WRITE`
/// `META_CHANGED` -> `notify::ops::CHMOD`
/// `REMOVED` -> `notify::ops::REMOVE`
/// `CREATED` -> `notify::ops::CREATE`
/// `RENAMED` -> `notify::ops::RENAME`
fn collect_path_env_vars(pathops: &[PathOp]) -> Vec<(String, String)> {
    #[cfg(target_family = "unix")]
    const ENV_SEP: &str = ":";
    #[cfg(not(target_family = "unix"))]
    const ENV_SEP: &str = ";";

    let mut by_op = HashMap::new(); // Paths as `String`s collected by `notify::op`
    let mut all_pathbufs = HashSet::new(); // All unique `PathBuf`s
    for pathop in pathops {
        if let Some(op) = pathop.op {
            // ignore pathops that don't have a `notify::op` set
            if let Some(s) = pathop.path.to_str() {
                // ignore invalid utf8 paths
                all_pathbufs.insert(pathop.path.clone());
                let e = by_op.entry(op).or_insert_with(Vec::new);
                e.push(s.to_owned());
            }
        }
    }

    let mut vars = Vec::new();
    // Only break off a common path if we have more than one unique path,
    // otherwise we end up with a `COMMON_PATH` being set and other vars
    // being present but empty.
    let common_path = if all_pathbufs.len() > 1 {
        let all_pathbufs: Vec<PathBuf> = all_pathbufs.into_iter().collect();
        get_longest_common_path(&all_pathbufs)
    } else {
        None
    };
    if let Some(ref common_path) = common_path {
        vars.push(("WATCHEXEC_COMMON_PATH".to_string(), common_path.to_string()));
    }
    for (op, paths) in by_op {
        let key = match op {
            op if PathOp::is_create(op) => "WATCHEXEC_CREATED_PATH",
            op if PathOp::is_remove(op) => "WATCHEXEC_REMOVED_PATH",
            op if PathOp::is_rename(op) => "WATCHEXEC_RENAMED_PATH",
            op if PathOp::is_write(op) => "WATCHEXEC_WRITTEN_PATH",
            op if PathOp::is_meta(op) => "WATCHEXEC_META_CHANGED_PATH",
            _ => continue, // ignore `notify::op::RESCAN`s
        };

        let paths = if let Some(ref common_path) = common_path {
            paths
                .iter()
                .map(|path_str| path_str.trim_start_matches(common_path).to_string())
                .collect::<Vec<_>>()
        } else {
            paths
        };
        vars.push((key.to_string(), paths.as_slice().join(ENV_SEP)));
    }
    vars
}

fn get_longest_common_path(paths: &[PathBuf]) -> Option<String> {
    match paths.len() {
        0 => return None,
        1 => return paths[0].to_str().map(ToString::to_string),
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

    result.to_str().map(ToString::to_string)
}

#[cfg(test)]
#[cfg(target_family = "unix")]
mod tests {
    use super::Shell;
    use crate::pathop::PathOp;
    use std::collections::HashSet;
    use std::path::PathBuf;

    use super::collect_path_env_vars;
    use super::get_longest_common_path;
    use super::spawn;

    #[test]
    fn test_shell_default() {
        let _ = spawn(&["echo".into(), "hi".into()], &[], Shell::default(), false);
    }

    #[test]
    fn test_shell_none() {
        let _ = spawn(&["echo".into(), "hi".into()], &[], Shell::None, false);
    }

    #[test]
    fn test_shell_alternate() {
        let _ = spawn(
            &["echo".into(), "hi".into()],
            &[],
            Shell::Unix("bash".into()),
            false,
        );
    }

    #[test]
    fn test_shell_alternate_shopts() {
        let _ = spawn(
            &["echo".into(), "hi".into()],
            &[],
            Shell::Unix("bash -o errexit".into()),
            false,
        );
    }

    #[test]
    fn longest_common_path_should_return_correct_value() {
        let single_path = vec![PathBuf::from("/tmp/random/")];
        let single_result =
            get_longest_common_path(&single_path).expect("failed to get longest common path");
        assert_eq!(single_result, "/tmp/random/");

        let common_paths = vec![
            PathBuf::from("/tmp/logs/hi"),
            PathBuf::from("/tmp/logs/bye"),
            PathBuf::from("/tmp/logs/bye"),
            PathBuf::from("/tmp/logs/fly"),
        ];

        let common_result =
            get_longest_common_path(&common_paths).expect("failed to get longest common path");
        assert_eq!(common_result, "/tmp/logs");

        let diverging_paths = vec![PathBuf::from("/tmp/logs/hi"), PathBuf::from("/var/logs/hi")];

        let diverging_result =
            get_longest_common_path(&diverging_paths).expect("failed to get longest common path");
        assert_eq!(diverging_result, "/");

        let uneven_paths = vec![
            PathBuf::from("/tmp/logs/hi"),
            PathBuf::from("/tmp/logs/"),
            PathBuf::from("/tmp/logs/bye"),
        ];

        let uneven_result =
            get_longest_common_path(&uneven_paths).expect("failed to get longest common path");
        assert_eq!(uneven_result, "/tmp/logs");
    }

    #[test]
    fn pathops_collect_to_env_vars() {
        let pathops = vec![
            PathOp::new(
                &PathBuf::from("/tmp/logs/hi"),
                Some(notify::op::CREATE),
                None,
            ),
            PathOp::new(
                &PathBuf::from("/tmp/logs/hey/there"),
                Some(notify::op::CREATE),
                None,
            ),
            PathOp::new(
                &PathBuf::from("/tmp/logs/bye"),
                Some(notify::op::REMOVE),
                None,
            ),
        ];
        let expected_vars = vec![
            ("WATCHEXEC_COMMON_PATH".to_string(), "/tmp/logs".to_string()),
            ("WATCHEXEC_REMOVED_PATH".to_string(), "/bye".to_string()),
            (
                "WATCHEXEC_CREATED_PATH".to_string(),
                "/hi:/hey/there".to_string(),
            ),
        ];
        let vars = collect_path_env_vars(&pathops);
        assert_eq!(
            vars.iter().collect::<HashSet<_>>(),
            expected_vars.iter().collect::<HashSet<_>>()
        );
    }
}

#[cfg(test)]
#[cfg(target_family = "windows")]
mod tests {
    use super::{spawn, Shell};

    #[test]
    fn test_shell_default() {
        let _ = spawn(&["echo".into(), "hi".into()], &[], Shell::default(), false);
    }

    #[test]
    fn test_shell_cmd() {
        let _ = spawn(&["echo".into(), "hi".into()], &[], Shell::Cmd, false);
    }

    #[test]
    fn test_shell_powershell() {
        let _ = spawn(&["echo".into(), "hi".into()], &[], Shell::Powershell, false);
    }

    #[test]
    fn test_shell_bash() {
        let _ = spawn(
            &["echo".into(), "hi".into()],
            &[],
            Shell::Unix("bash".into()),
            false,
        );
    }
}
