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
            cls: clear
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
    fn kill(child: &mut Child) {
        let _ = child.kill();
    }

    #[cfg(target_family = "unix")]
    fn kill(child: &mut Child) {
        use libc;

        extern {
            fn killpg(pgrp: libc::pid_t, sig: libc::c_int) -> libc::c_int;
        }

        unsafe {
            killpg(child.id() as i32, libc::SIGTERM);
        }
    }

    #[cfg(target_family = "windows")]
    fn invoke(&self, cmd: &str, updated_paths: Vec<&str>) -> Option<Child> {
        let mut command = Command::new("cmd.exe");
        command.arg("/C").arg(cmd);

        if !updated_paths.is_empty() {
            command.env("WATCHEXEC_UPDATED_PATH", updated_paths[0]);
        }

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

        command
            .before_exec(|| unsafe { libc::setpgid(0, 0); Ok(()) })
            .spawn()
            .ok()
    }

    pub fn run_command(&mut self, cmd: &str, updated_paths: Vec<&str>) {
        if let Some(ref mut child) = self.process {
            if self.restart {
                debug!("Killing child process (pid: {})", child.id());
                Runner::kill(child);
            }

            debug!("Waiting for child process (pid: {})", child.id());
            Runner::wait(child);
        }

        if self.cls {
            self.clear();
        }

        debug!("Executing: {}", cmd);

        self.process = self.invoke(cmd, updated_paths);
    }

    #[cfg(target_family = "windows")]
    fn wait(child: &mut Child) {
        let _ = child.wait();
    }

    #[cfg(target_family = "unix")]
    fn wait(child: &mut Child) {
        use libc;

        unsafe {
            let pid = child.id() as i32;
            let status: Box<i32> = Box::new(0);
            libc::waitpid(-pid, Box::into_raw(status), 0);
        }
    }

}
