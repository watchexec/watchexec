# Contribution guidebook


This is a fairly free-form project, with low contribution traffic.

Maintainers:

- Félix Saparelli (@passcod) (active)
- Matt Green (@mattgreen) (original author, mostly checked out)

There are a few anti goals:

- Calling watchexec is to be a **simple** exercise that remains intuitive. As a specific point, it
  should not involve any piping or require xargs.

- Watchexec will not be tied to any particular ecosystem or language. Projects that themselves use
  watchexec (the library) can be focused on a particular domain (for example Cargo Watch for Rust),
  but watchexec itself will remain generic, usable for any purpose.


## Debugging

To enable verbose logging in tests, run with:

```console
$ env RUST_LOG=watchexec=trace,info RUST_TEST_THREADS=1 RUST_NOCAPTURE=1 cargo test --test testfile -- testname
```

To use [Tokio Console](https://github.com/tokio-rs/console):

1. Add `--cfg tokio_unstable` to your `RUSTFLAGS`.
2. Run the CLI with the `dev-console` feature.


## PR etiquette

- Maintainers are busy or may not have the bandwidth, be patient.
- Do _not_ change the version number in the PR.
- Do _not_ change Cargo.toml or other project metadata, unless specifically asked for, or if that's
  the point of the PR (like adding a crates.io category).

Apart from that, welcome and thank you for your time!


## Releasing

A release goes like this:

1. A maintainer launches the ["Open a release PR" workflow](https://github.com/watchexec/watchexec/actions/workflows/release-pr.yml).

2. A PR bumping the chosen crate's version is opened. Maintainers may then add stuff to it if
   needed, like changelog entries for library crates. Release notes for CLI releases go directly on
   the PR.

3. When the PR is merged, the release is tagged. CLI releases also get built and distributed.

4. A maintainer then manually publishes the crate (automated publishing is blocked on crates.io
   implementing [scoped tokens](https://github.com/rust-lang/crates.io/issues/5443)).

### Release order

When all crates need releasing, the order is:

- project-origins (depends on nothing)
- ignore-files (depends on project-origins)
- watchexec lib
- ignore filterer (depends on watchexec lib)
- other filterers (depends on ignore filterer)
- watchexec cli

## Overview

The architecture of watchexec is roughly:

- sources gather events
- events are debounced and filtered
- event(s) make it through the debounce/filters and trigger an "action"
- `on_action` handler is called, returning an `Outcome`
- outcome is processed into managing the command that watchexec is running
  - outcome can also be to exit
- when a command is started, the `on_pre_spawn` and `on_post_spawn` handlers are called
- commands are also a source of events, so e.g. "command has finished" is handled by `on_action`

And this is the startup sequence:
- init config sets basic immutable facts about the runtime
- runtime starts:
  - source workers start, and are passed their runtime config
  - action worker starts, and is passed its runtime config
- (unless `--postpone` is given) a synthetic event is injected to kickstart things

## Guides

These are generic guides for implementing specific bits of functionality.

### Adding an event source

- add a worker for "sourcing" events. Looking at the [signal source
  worker](https://github.com/watchexec/watchexec/blob/main/crates/lib/src/signal/source.rs) is
  probably easiest to get started here.

- because we may not always want to enable this event source, and just to be flexible, add [runtime
  config](https://github.com/watchexec/watchexec/blob/main/crates/lib/src/config.rs) for the source.

- for convenience, probably add [a method on the runtime
  config](https://github.com/watchexec/watchexec/blob/main/crates/lib/src/config.rs) which
  configures the most common usecase.

- because watchexec is reconfigurable, in the worker you'll need to react to config changes. Look at
  how the [fs worker does it](https://github.com/watchexec/watchexec/blob/main/crates/lib/src/fs.rs)
  for reference.

- you may need to [add to the event tag
  enum](https://github.com/watchexec/watchexec/blob/main/crates/lib/src/event.rs).

- if you do, you should [add support to the "tagged
  filterer"](https://github.com/watchexec/watchexec/blob/main/crates/filterer/tagged/src/parse.rs),
  but this can be done in follow-up work.

### Process a new event in the CLI

- add an option to the
  [args](https://github.com/watchexec/watchexec/blob/main/crates/cli/src/args.rs) if necessary

- add to the [runtime
  config](https://github.com/watchexec/watchexec/blob/main/crates/cli/src/config/runtime.rs) when
  the option is present

- process relevant events [in the action
  handler](https://github.com/watchexec/watchexec/blob/main/crates/cli/src/config/runtime.rs)

- document the option in the [manual
  page](https://github.com/watchexec/watchexec/blob/main/doc/watchexec.1.ronn)

---
vim: tw=100
