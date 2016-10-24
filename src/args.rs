use std::path::Path;

use clap::{App, Arg};

#[derive(Debug)]
pub struct Args {
    pub cmd: String,
    pub paths: Vec<String>,
    pub filters: Vec<String>,
    pub ignores: Vec<String>,
    pub clear_screen: bool,
    pub restart: bool,
    pub debug: bool,
    pub run_initially: bool,
    pub no_vcs_ignore: bool,
    pub poll: bool,
    pub poll_interval: u32,
}

pub fn get_args() -> Args {
    let args = App::new("watchexec")
        .version(crate_version!())
        .about("Execute commands when watched files change")
        .arg(Arg::with_name("path")
            .help("Path to watch [default: .]")
            .short("w")
            .long("watch")
            .number_of_values(1)
            .multiple(true)
            .takes_value(true))
        .arg(Arg::with_name("command")
            .help("Command to execute")
            .multiple(true)
            .required(true))
        .arg(Arg::with_name("extensions")
            .help("Comma-separated list of file extensions to watch (js,css,html)")
            .short("e")
            .long("exts")
            .takes_value(true))
        .arg(Arg::with_name("clear")
            .help("Clear screen before executing command")
            .short("c")
            .long("clear"))
        .arg(Arg::with_name("restart")
            .help("Restart the process if it's still running")
            .short("r")
            .long("restart"))
        .arg(Arg::with_name("debug")
            .help("Print debugging messages to stderr")
            .short("d")
            .long("debug"))
        .arg(Arg::with_name("filter")
            .help("Ignore all modifications except those matching the pattern")
            .short("f")
            .long("filter")
            .number_of_values(1)
            .multiple(true)
            .takes_value(true)
            .value_name("pattern"))
        .arg(Arg::with_name("ignore")
            .help("Ignore modifications to paths matching the pattern")
            .short("i")
            .long("ignore")
            .number_of_values(1)
            .multiple(true)
            .takes_value(true)
            .value_name("pattern"))
        .arg(Arg::with_name("no-vcs-ignore")
            .help("Skip auto-loading of .gitignore files for filtering")
            .long("no-vcs-ignore"))
        .arg(Arg::with_name("run-initially")
            .help("Run command initially, before first file change")
            .long("run-initially"))
        .arg(Arg::with_name("poll")
            .help("Forces polling mode")
            .long("force-poll")
            .value_name("interval"))
        .get_matches();

    let cmd = values_t!(args.values_of("command"), String).unwrap().join(" ");
    let paths = values_t!(args.values_of("path"), String).unwrap_or(vec![String::from(".")]);
    let mut filters = values_t!(args.values_of("filter"), String).unwrap_or(vec![]);

    if let Some(extensions) = args.values_of("extensions") {
        for exts in extensions {
            filters.extend(exts.split(",")
                .filter(|ext| !ext.is_empty())
                .map(|ext| format!("*.{}", ext.replace(".", ""))));

        }
    }

    let dotted_dirs = Path::new(".*").join("*");
    let default_ignores = vec!["*/.DS_Store", "*.pyc", "*.swp", dotted_dirs.to_str().unwrap()];
    let mut ignores = vec![];

    for default_ignore in default_ignores {
        ignores.push(String::from(default_ignore));
    }

    ignores.extend(values_t!(args.values_of("ignore"), String).unwrap_or(vec![]));
    let poll_interval = if args.occurrences_of("poll") > 0 {
        value_t!(args.value_of("poll"), u32).unwrap_or_else(|e| e.exit())
    } else {
        1000
    };

    Args {
        cmd: cmd,
        paths: paths,
        filters: filters,
        ignores: ignores,
        clear_screen: args.is_present("clear"),
        restart: args.is_present("restart"),
        debug: args.is_present("debug"),
        run_initially: args.is_present("run-initially"),
        no_vcs_ignore: args.is_present("no-vcs-ignore"),
        poll: args.occurrences_of("poll") > 0,
        poll_interval: poll_interval,
    }
}
