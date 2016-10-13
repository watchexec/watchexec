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

    fn clear(&self) {
        // TODO: determine better way to do this
        let clear_cmd;
        if cfg!(target_os = "windows") {
            clear_cmd = "cls";
        }
        else {
            clear_cmd = "clear";
        }

        let _ = Command::new(clear_cmd).status();
    }


    fn invoke(&self, cmd: &str) -> Option<Child> {
        let shell;
        let shell_cmd_arg;

        if cfg!(target_os = "windows") {
            shell = "cmd.exe";
            shell_cmd_arg = "/C";
        }
        else {
            shell = "sh";
            shell_cmd_arg = "-c";
        }

        Command::new(shell)
            .arg(shell_cmd_arg)
            .arg(cmd)
            .spawn()
            .ok()
    }


    pub fn run_command(&mut self, cmd: &str) {
        if let Some(ref mut child) = self.process {
            if self.restart {
                let _ = child.kill();
            }

            let _ = child.wait();
        }

        if self.cls {
            self.clear();
        }

        debug!("Executing: {}", cmd);

        self.process = self.invoke(cmd);
    }
}
