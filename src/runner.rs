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
    fn invoke(&self, cmd: &str, updated_paths: Vec<&str>) -> Option<Child> {
        let mut command = Command::new("cmd.exe");
        command.arg("/C").arg(cmd);

        if updated_files.len() > 0 {
            command.env("WATCHEXEC_UPDATED_PATH", updated_paths[0]);
        }

        command.spawn().ok()
    }

    #[cfg(target_family = "unix")]
    fn invoke(&self, cmd: &str, updated_paths: Vec<&str>) -> Option<Child> {
        let mut command = Command::new("sh");
        command.arg("-c").arg(cmd);

        if updated_paths.len() > 0 {
            command.env("WATCHEXEC_UPDATED_PATH", updated_paths[0]);
        }

        command.spawn().ok()
    }

    pub fn run_command(&mut self, cmd: &str, updated_paths: Vec<&str>) {
        if let Some(ref mut child) = self.process {
            if self.restart {
                debug!("Killing child process (pid: {})", child.id());
                let _ = child.kill();
            }

            debug!("Waiting for child process (pid: {})", child.id());
            let _ = child.wait();
        }

        if self.cls {
            self.clear();
        }

        debug!("Executing: {}", cmd);

        self.process = self.invoke(cmd, updated_paths);
    }
}
