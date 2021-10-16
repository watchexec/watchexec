[![CI status](https://github.com/watchexec/watchexec/actions/workflows/check.yml/badge.svg)](https://github.com/watchexec/watchexec/actions/workflows/check.yml)

# Watchexec

Software development often involves running the same commands over and over. Boring!

`watchexec` is a **simple**, standalone tool that watches a path and runs a command whenever it detects modifications.

Example use cases:

* Automatically run unit tests
* Run linters/syntax checkers


## Features

* Simple invocation and use, does not require a cryptic command line involving `xargs`
* Runs on OS X, Linux, and Windows
* Monitors current directory and all subdirectories for changes
* Coalesces multiple filesystem events into one, for editors that use swap/backup files during saving
* Loads `.gitignore` and `.ignore` files
* Uses process groups to keep hold of forking programs
* Provides the paths that changed in environment variables
* Does not require a language runtime, not tied to any particular language or ecosystem
* [And more!](./cli/#features)


## Quick start

Watch all JavaScript, CSS and HTML files in the current directory and all subdirectories for changes, running `npm run build` when a change is detected:

    $ watchexec -e js,css,html npm run build

Call/restart `python server.py` when any Python file in the current directory (and all subdirectories) changes:

    $ watchexec -r -e py -- python server.py

More usage examples: [in the CLI README](./cli/#usage-examples)!


## Install

<a href="https://repology.org/project/watchexec/versions"><img align="right" src="https://repology.org/badge/vertical-allrepos/watchexec.svg" alt="Packaging status"></a>

- As pre-built binary package: https://watchexec.github.io/downloads/
- With your package manager for Arch, Homebrew, Nix, Scoop, Chocolatey…
- From source with Cargo: `cargo install watchexec-cli`

All options in detail: [in the CLI README](./cli/#installation).


## Extend

- [watchexec library](./lib/): to create more specialised watchexec-powered tools! such as:
  - [cargo watch](https://github.com/watchexec/cargo-watch): for Rust/Cargo projects.
- [clearscreen](https://github.com/watchexec/clearscreen): to clear the (terminal) screen on every platform.
- [command group](https://github.com/watchexec/command-group): to run commands in process groups.
- [notify](https://github.com/notify-rs/notify): to respond to file modifications (third-party).
- [globset](https://crates.io/crates/globset): to match globs (third-party).
