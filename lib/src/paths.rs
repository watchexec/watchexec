use crate::pathop::PathOp;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

/// Collect `PathOp` details into op-categories to pass onto the exec'd command as env-vars
///
/// `WRITTEN` -> `notify::ops::WRITE`, `notify::ops::CLOSE_WRITE`
/// `META_CHANGED` -> `notify::ops::CHMOD`
/// `REMOVED` -> `notify::ops::REMOVE`
/// `CREATED` -> `notify::ops::CREATE`
/// `RENAMED` -> `notify::ops::RENAME`
pub fn collect_path_env_vars(pathops: &[PathOp]) -> Vec<(String, String)> {
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

pub fn get_longest_common_path(paths: &[PathBuf]) -> Option<String> {
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
mod tests {
    use crate::pathop::PathOp;
    use std::collections::HashSet;
    use std::path::PathBuf;

    use super::collect_path_env_vars;
    use super::get_longest_common_path;

    #[test]
    #[cfg(unix)]
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
    #[cfg(unix)]
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
