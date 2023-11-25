[![Crates.io page](https://badgen.net/crates/v/watchexec)](https://crates.io/crates/watchexec)
[![API Docs](https://docs.rs/watchexec/badge.svg)][docs]
[![Crate license: Apache 2.0](https://badgen.net/badge/license/Apache%202.0)][license]
[![CI status](https://github.com/watchexec/watchexec/actions/workflows/check.yml/badge.svg)](https://github.com/watchexec/watchexec/actions/workflows/check.yml)

# Watchexec library

_The library which powers [Watchexec CLI](https://watchexec.github.io) and other tools._

- **[API documentation][docs]**.
- Licensed under [Apache 2.0][license].
- Status: maintained.

[docs]: https://docs.rs/watchexec
[license]: ../../LICENSE


## Examples

Here's a complete example showing some of the library's features:

```rust ,no_run
use miette::{IntoDiagnostic, Result};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use watchexec::{
    command::{Command, Program, Shell},
    job::CommandState,
    Watchexec,
};
use watchexec_events::{Event, Priority};
use watchexec_signals::Signal;

#[tokio::main]
async fn main() -> Result<()> {
    // this is okay to start with, but Watchexec logs a LOT of data,
    // even at error level. you will quickly want to filter it down.
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // initialise Watchexec with a simple initial action handler
    let job = Arc::new(Mutex::new(None));
    let wx = Watchexec::new({
        let outerjob = job.clone();
        move |mut action| {
            let (_, job) = action.create_job(Arc::new(Command {
                program: Program::Shell {
                    shell: Shell::new("bash"),
                    command: "
                        echo 'Hello world'
                        trap 'echo Not quitting yet!' TERM
                        read
                    "
                    .into(),
                    args: Vec::new(),
                },
                options: Default::default(),
            }));

            // store the job outside this closure too
            *outerjob.lock().unwrap() = Some(job.clone());

            // block SIGINT
            #[cfg(unix)]
            job.set_spawn_hook(|cmd, _| {
                use nix::sys::signal::{sigprocmask, SigSet, SigmaskHow, Signal};
                unsafe {
                    cmd.pre_exec(|| {
                        let mut newset = SigSet::empty();
                        newset.add(Signal::SIGINT);
                        sigprocmask(SigmaskHow::SIG_BLOCK, Some(&newset), None)?;
                        Ok(())
                    });
                }
            });

            // start the command
            job.start();

            action
        }
    })?;

    // start the engine
    let main = wx.main();

    // send an event to start
    wx.send_event(Event::default(), Priority::Urgent)
        .await
        .unwrap();
    // ^ this will cause the action handler we've defined above to run,
    //   creating and starting our little bash program, and storing it in the mutex

    // spin until we've got the job
    while job.lock().unwrap().is_none() {
        tokio::task::yield_now().await;
    }

    // watch the job and restart it when it exits
    let job = job.lock().unwrap().clone().unwrap();
    let auto_restart = tokio::spawn(async move {
        loop {
            job.to_wait().await;
            job.run(|context| {
                if let CommandState::Finished {
                    status,
                    started,
                    finished,
                } = context.current
                {
                    let duration = *finished - *started;
                    eprintln!("[Program stopped with {status:?}; ran for {duration:?}]")
                }
            })
            .await;

            eprintln!("[Restarting...]");
            job.start().await;
        }
    });

    // now we change what the action does:
    let auto_restart_abort = auto_restart.abort_handle();
    wx.config.on_action(move |mut action| {
        // if we get Ctrl-C on the Watchexec instance, we quit
        if action.signals().any(|sig| sig == Signal::Interrupt) {
            eprintln!("[Quitting...]");
            auto_restart_abort.abort();
            action.quit_gracefully(Signal::ForceStop, Duration::ZERO);
            return action;
        }

        // if the action was triggered by file events, gracefully stop the program
        if action.paths().next().is_some() {
            // watchexec can manage ("supervise") more than one program;
            // here we only have one but we don't know its Id so we grab it out of the iterator
            if let Some(job) = action.list_jobs().next().map(|(_, job)| job.clone()) {
                eprintln!("[Asking program to stop...]");
                job.stop_with_signal(Signal::Terminate, Duration::from_secs(5));
            }
        }

        action
    });

    // and watch all files in the current directory:
    wx.config.pathset(["."]);

    // then keep running until Watchexec quits!
    let _ = main.await.into_diagnostic()?;
    auto_restart.abort();
    Ok(())
}
```

Other examples:
- [Only Commands](./examples/only_commands.rs): skip watching files, only use the supervisor.
- [Only Events](./examples/only_events.rs): never start any processes, only print events.
- [Restart `cargo run` only when `cargo build` succeeds](./examples/restart_run_on_successful_build.rs)


## Kitchen sink

Though not its primary usecase, the library exposes most of its relatively standalone components,
available to make other tools that are not Watchexec-shaped:

- **Event sources**: [Filesystem](https://docs.rs/watchexec/3/watchexec/sources/fs/index.html),
  [Signals](https://docs.rs/watchexec/3/watchexec/sources/signal/index.html),
  [Keyboard](https://docs.rs/watchexec/3/watchexec/sources/keyboard/index.html).

- Finding **[a common prefix](https://docs.rs/watchexec/3/watchexec/paths/fn.common_prefix.html)**
  of a set of paths.

- A **[Changeable](https://docs.rs/watchexec/3/watchexec/changeable/index.html)** type, which
  powers the "live" configuration system.

- And [more][docs]!

Filterers are split into their own crates, so they can be evolved independently:

- The **[Globset](https://docs.rs/watchexec-filterer-globset) filterer** implements the default
  Watchexec CLI filtering, based on the regex crate's ignore mechanisms.

- ~~The **[Tagged](https://docs.rs/watchexec-filterer-tagged) filterer**~~ was an experiment in
  creating a more powerful filtering solution, which could operate on every part of events, not
  just their paths, using a custom syntax. It is no longer maintained.

- The **[Ignore](https://docs.rs/watchexec-filterer-ignore) filterer** implements ignore-file
  semantics, and especially supports _trees_ of ignore files. It is used as a subfilterer in both
  of the main filterers above.

There are also separate, standalone crates used to build Watchexec which you can tap into:

- **[Supervisor](https://docs.rs/watchexec-supervisor)** is Watchexec's process supervisor and
  command abstraction.

- **[ClearScreen](https://docs.rs/clearscreen)** makes clearing the terminal screen in a
  cross-platform way easy by default, and provides advanced options to fit your usecase.

- **[Command Group](https://docs.rs/command-group)** augments the std and tokio `Command` with
  support for process groups, portable between Unix and Windows.

- **[Event types](https://docs.rs/watchexec-events)** contains the event types used by Watchexec,
  including the JSON format used for passing event data to child processes.

- **[Signal types](https://docs.rs/watchexec-signals)** contains the signal types used by Watchexec.

- **[Ignore files](https://docs.rs/ignore-files)** finds, parses, and interprets ignore files.

- **[Project Origins](https://docs.rs/project-origins)** finds the origin (or root) path of a
  project, and what kind of project it is.

## Rust version (MSRV)

Due to the unpredictability of dependencies changing their MSRV, this library no longer tries to
keep to a minimum supported Rust version behind stable. Instead, it is assumed that developers use
the latest stable at all times.

Applications that wish to support lower-than-stable Rust (such as the Watchexec CLI does) should:
- use a lock file
- recommend the use of `--locked` when installing from source
- provide pre-built binaries (and [Binstall](https://github.com/cargo-bins/cargo-binstall) support) for non-distro users
- avoid using newer features until some time has passed, to let distro users catch up
- consider recommending that distro-Rust users switch to distro `rustup` where available
