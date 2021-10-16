[![Crates.io page](https://badgen.net/crates/v/watchexec)](https://crates.io/crates/watchexec)
[![API Docs](https://docs.rs/watchexec/badge.svg)][docs]
[![Crate license: Apache 2.0](https://badgen.net/badge/license/Apache%202.0)][license]
![MSRV: 1.43.0 (breaking)](https://badgen.net/badge/MSRV/1.43.0%20%28breaking%29/green)
[![CI status](https://github.com/watchexec/watchexec/actions/workflows/check.yml/badge.svg)](https://github.com/watchexec/watchexec/actions/workflows/check.yml)

# Watchexec library

_The library which powers [Watchexec CLI](https://watchexec.github.io) and other tools._

- **[API documentation][docs]**.
- Licensed under [Apache 2.0][license].
- Minimum Supported Rust Version: 1.43.0.
- Status: in preview (`2.0.0-pre.N` series).

[docs]: https://docs.rs/watchexec
[license]: ../LICENSE


## Quick start

```rust
use miette::{IntoDiagnostic, Result};
use watchexec::{
    Watchexec,
    action::{Action, Outcome},
    config::{InitConfig, RuntimeConfig},
    handler::{Handler as _, PrintDebug},
};

#[tokio::main]
async fn main() -> Result<()> {
    let mut init = InitConfig::default();
    init.on_error(PrintDebug(std::io::stderr()));

    let mut runtime = RuntimeConfig::default();
    runtime.pathset(["watchexec.conf"]);

    let conf = YourConfigFormat::load_from_file("watchexec.conf").await?;
    conf.apply(&mut runtime);

    let we = Watchexec::new(init, runtime.clone())?;
    let w = we.clone();

    let c = runtime.clone();
    runtime.on_action(move |action: Action| {
        let mut c = c.clone();
        let w = w.clone();
        async move {
            for event in &action.events {
                if event.paths().any(|(p, _)| p.ends_with("/watchexec.conf")) {
                    let conf = YourConfigFormat::load_from_file("watchexec.conf").await?;

                    conf.apply(&mut c);
                    w.reconfigure(c.clone());
                    // tada! self-reconfiguring watchexec on config file change!

                    break;
                }
            }

            action.outcome(Outcome::if_running(
                Outcome::DoNothing,
                Outcome::both(Outcome::Clear, Outcome::Start),
            ));

            Ok(())
        }
    });

    we.main().await.into_diagnostic()?;
    Ok(())
}
```


## Kitchen sink

The library also exposes a large amount of components which are available to make your own tool, or
to make anything else you may want:

- **[Command handling](https://docs.rs/watchexec/2.0.0-pre.0/watchexec/command/index.html)**, to
  build a command with an arbitrary shell, deal with grouped and ungrouped processes the same way,
  and supervise a process while also listening for & acting on interventions such as sending signals.

- **Event sources**: [Filesystem](https://docs.rs/watchexec/2.0.0-pre.0/watchexec/fs/index.html),
  [Signals](https://docs.rs/watchexec/2.0.0-pre.0/watchexec/signal/source/index.html), (more to come).

- Finding **[a common prefix](https://docs.rs/watchexec/2.0.0-pre.0/watchexec/paths/fn.common_prefix.html)**
  of a set of paths.

- Detecting the **[origin(s)](https://docs.rs/watchexec/2.0.0-pre.0/watchexec/project/fn.origins.html)**
  and **[types](https://docs.rs/watchexec/2.0.0-pre.0/watchexec/project/fn.types.html)** of projects.

- Discovering project-local and global
  **[ignore files](https://docs.rs/watchexec/2.0.0-pre.0/watchexec/ignore_files/index.html)**.

There are also separate, standalone crates used to build Watchexec which you can tap into:

- **[ClearScreen](https://docs.rs/clearscreen)** makes clearing the terminal screen in a
  cross-platform way easy by default, and provides advanced options to fit your usecase.

- **[Command Group](https://docs.rs/command-group)** augments the std and tokio `Command` with
  support for process groups, portable between Unix and Windows.


## Tagged filters (alpha)

This library is also the home of Watchexec's current _two_ filtering implementations: the v1
behaviour which has proven confusing and inconsistent over the years, and an upcoming complete
overhaul called "tagged filtering."

Have a look at the [docs](https://docs.rs/watchexec/2.0.0-pre.0/watchexec/filter/tagged/struct.TaggedFilterer.html)!
