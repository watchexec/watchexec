# watchexec

[![Build status](https://badgen.net/travis/watchexec/watchexec/main)](https://travis-ci.org/watchexec/watchexec)
[![Crates.io status](https://badgen.net/crates/v/watchexec)](https://crates.io/crates/watchexec)
[![Docs status](https://docs.rs/watchexec/badge.svg)](https://docs.rs/watchexec)

Software development often involves running the same commands over and over. Boring!

`watchexec` is a **simple**, standalone tool that watches a path and runs a command whenever it detects modifications.

Example use cases:

* Automatically run unit tests
* Run linters/syntax checkers

## Features

* Simple invocation and use
* Runs on OS X, Linux and Windows
* Monitors current directory and all subdirectories for changes
    * Uses most efficient event polling mechanism for your platform (except for [BSD](https://github.com/passcod/notify#todo))
* Coalesces multiple filesystem events into one, for editors that use swap/backup files during saving
* By default, uses `.gitignore` to determine which files to ignore notifications for
* Support for watching files with a specific extension
* Support for filtering/ignoring events based on [glob patterns](https://docs.rs/globset/*/globset/#syntax)
* Launches child processes in a new process group
* Sets the following environment variables in the child process:
    * If a single file changed (depending on the event type):
        * `$WATCHEXEC_CREATED_PATH`, the path of the file that was created
        * `$WATCHEXEC_REMOVED_PATH`, the path of the file that was removed
        * `$WATCHEXEC_RENAMED_PATH`, the path of the file that was renamed
        * `$WATCHEXEC_WRITTEN_PATH`, the path of the file that was modified
        * `$WATCHEXEC_META_CHANGED_PATH`, the path of the file whose metadata changed
    * If multiple files changed:
        * `$WATCHEXEC_COMMON_PATH`, the longest common path of all of the files that triggered a change
* Optionally clears screen between executions
* Optionally restarts the command with every modification (good for servers)
* Does not require a language runtime

## Anti-Features

* Not tied to any particular language or ecosystem
* Does not require a cryptic command line involving `xargs`

## Usage Examples

Watch all JavaScript, CSS and HTML files in the current directory and all subdirectories for changes, running `make` when a change is detected:

    $ watchexec --exts js,css,html make

Call `make test` when any file changes in this directory/subdirectory, except for everything below `target`:

    $ watchexec -i target make test

Call `ls -la` when any file changes in this directory/subdirectory:

    $ watchexec -- ls -la

Call/restart `python server.py` when any Python file in the current directory (and all subdirectories) changes:

    $ watchexec -e py -r python server.py

Call/restart `my_server` when any file in the current directory (and all subdirectories) changes, sending `SIGKILL` to stop the child process:

    $ watchexec -r -s SIGKILL my_server

Send a SIGHUP to the child process upon changes (Note: with using `-n | --no-shell` here, we're executing `my_server` directly, instead of wrapping it in a shell:

    $ watchexec -n -s SIGHUP my_server

Run `make` when any file changes, using the `.gitignore` file in the current directory to filter:

    $ watchexec make

Run `make` when any file in `lib` or `src` changes:

    $ watchexec -w lib -w src make

## Installation

### Cargo

watchexec requires Rust 1.38 or later. You can install it using cargo:

    $ cargo install watchexec

### OS X with Homebrew

    $ brew install watchexec

### Linux

For now, use the GitHub Releases tab to obtain the binary. PRs for packaging in unsupported distros are welcomed.

#### Debian

A deb package is available for amd64 architectures in the GitHub Releases.

#### Arch Linux

Available in the **community** repository:

    $ pacman -S watchexec

### Windows

Available [using scoop](https://scoop.sh/):

    #> scoop install watchexec

Or [chocolatey](https://chocolatey.org/packages/watchexec):

    #> choco install watchexec

Or just unzip the binary from the GitHub Releases.

## Shell completions

Currently available shell completions:

- zsh: `completions/zsh` should be installed to `/usr/share/zsh/site-functions/_watchexec`

## Building

Rust 1.38 or later is required.

## Credits

* [notify](https://github.com/passcod/notify) for doing most of the heavy-lifting
* [globset](https://crates.io/crates/globset) for super-fast glob matching
