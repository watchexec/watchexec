use std::process::{Child, Command};

pub struct Runner {
    process: Option<Child>,
    restart: bool,
    cls: bool,
}

impl Runner {
    pub fn new(restart: bool, clear: bool) -> Runner {
        Runner {
            process: None,
            restart: restart,
            cls: clear,
        }
    }

    #[cfg(target_family = "windows")]
    fn clear(&self) {
        let _ = Command::new("cls").status();
    }

    #[cfg(target_family = "unix")]
    fn clear(&self) {
        let _ = Command::new("clear").status();
    }

    #[cfg(target_family = "windows")]
    fn kill(&mut self) {
        if let Some(ref mut child) = self.process {
            debug!("Killing child process (pid: {})", child.id());

            let _ = child.kill();
        }
    }

    #[cfg(target_family = "unix")]
    fn kill(&mut self) {
        use libc;

        extern "C" {
            fn killpg(pgrp: libc::pid_t, sig: libc::c_int) -> libc::c_int;
        }

        if let Some(ref mut child) = self.process {
            debug!("Killing child process (pid: {})", child.id());

            unsafe {
                killpg(child.id() as i32, libc::SIGTERM);
            }
        }
    }

    #[cfg(target_family = "windows")]
    fn invoke(&self, cmd: &str, updated_paths: Vec<&str>) -> Option<Child> {
        let mut command = Command::new("cmd.exe");
        command.arg("/C").arg(cmd);

        if !updated_paths.is_empty() {
            command.env("WATCHEXEC_UPDATED_PATH", updated_paths[0]);
        }

        debug!("Executing: {}", cmd);

        command.spawn().ok()
    }

    #[cfg(target_family = "unix")]
    fn invoke(&self, cmd: &str, updated_paths: Vec<&str>) -> Option<Child> {
        use libc;
        use std::os::unix::process::CommandExt;

        let mut command = Command::new("sh");
        command.arg("-c").arg(cmd);

        if !updated_paths.is_empty() {
            command.env("WATCHEXEC_UPDATED_PATH", updated_paths[0]);
        }

        debug!("Executing: {}", cmd);

        command.before_exec(|| unsafe {
                libc::setpgid(0, 0);
                Ok(())
            })
            .spawn()
            .ok()
    }

    pub fn run_command(&mut self, cmd: &str, updated_paths: Vec<&str>) {
        if self.restart {
            self.kill();
        }

        self.wait();

        if self.cls {
            self.clear();
        }

        self.process = self.invoke(cmd, updated_paths);
    }

    #[cfg(target_family = "windows")]
    fn wait(&mut self) {
        if let Some(ref mut child) = self.process {
            debug!("Waiting for child process (pid: {})", child.id());
            let _ = child.wait();
        }
    }

    #[cfg(target_family = "unix")]
    fn wait(&mut self) {
        use nix::sys::wait::waitpid;

        if let Some(ref mut child) = self.process {
            debug!("Waiting for child process (pid: {})", child.id());

            let pid = child.id() as i32;
            let _ = waitpid(-pid, None);
        }
    }
}

impl Drop for Runner {
    fn drop(&mut self) {
        self.kill();
        self.wait();
    }
}
