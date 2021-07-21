#![allow(unsafe_code)]

use crate::error::Result;
use crate::pathop::PathOp;
use command_group::{CommandGroup, GroupChild};
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
) -> Result<GroupChild> {
    let mut command = shell.to_command(&cmd);
    debug!("Assembled command {:?}", command);

    let command_envs = if !environment {
        Vec::new()
    } else {
        collect_path_env_vars(updated_paths)
    };

    for (name, val) in &command_envs {
        command.env(name, val);
    }

    let child = command.group_spawn()?;
    Ok(child)
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
