# Watchexec CLI

A simple standalone tool that watches a path and runs a command whenever it detects modifications.

Example use cases:

* Automatically run unit tests
* Run linters/syntax checkers

## Features

* Simple invocation and use
* Runs on Linux, Mac, Windows, and more
* Monitors current directory and all subdirectories for changes
    * Uses efficient event polling mechanism (on Linux, Mac, Windows, BSD)
* Coalesces multiple filesystem events into one, for editors that use swap/backup files during saving
* By default, uses `.gitignore`, `.ignore`, and other such files to determine which files to ignore notifications for
* Support for watching files with a specific extension
* Support for filtering/ignoring events based on [glob patterns](https://docs.rs/globset/*/globset/#syntax)
* Launches the command in a new process group (can be disabled with `--no-process-group`)
* Optionally clears screen between executions
* Optionally restarts the command with every modification (good for servers)
* Optionally sends a desktop notification on command start and end
* Does not require a language runtime
* Sets the following environment variables in the process:

    `$WATCHEXEC_COMMON_PATH` is set to the longest common path of all of the below variables, and so should be prepended to each path to obtain the full/real path.

    | Variable name | Event kind |
    |---|---|
    | `$WATCHEXEC_CREATED_PATH` | files/folders were created |
    | `$WATCHEXEC_REMOVED_PATH` | files/folders were removed |
    | `$WATCHEXEC_RENAMED_PATH` | files/folders were renamed |
    | `$WATCHEXEC_WRITTEN_PATH` | files/folders were modified |
    | `$WATCHEXEC_META_CHANGED_PATH` | files/folders' metadata were modified |
    | `$WATCHEXEC_OTHERWISE_CHANGED_PATH` | every other kind of event |

    These variables may contain multiple paths: these are separated by the platform's path separator, as with the `PATH` system environment variable. On Unix that is `:`, and on Windows `;`. Within each variable, paths are deduplicated and sorted in binary order (i.e. neither Unicode nor locale aware).

    This can be disabled or limited with `--no-environment` (doesn't set any of these variables) and `--no-meta` (ignores metadata changes).

## Anti-Features

* Not tied to any particular language or ecosystem
* Not tied to Git or the presence of a repository/project
* Does not require a cryptic command line involving `xargs`

## Usage Examples

Watch all JavaScript, CSS and HTML files in the current directory and all subdirectories for changes, running `make` when a change is detected:

    $ watchexec --exts js,css,html make

Call `make test` when any file changes in this directory/subdirectory, except for everything below `target`:

    $ watchexec -i "target/**" make test

Call `ls -la` when any file changes in this directory/subdirectory:

    $ watchexec -- ls -la

Call/restart `python server.py` when any Python file in the current directory (and all subdirectories) changes:

    $ watchexec -e py -r python server.py

Call/restart `my_server` when any file in the current directory (and all subdirectories) changes, sending `SIGKILL` to stop the command:

    $ watchexec -r --stop-signal SIGKILL my_server

Send a SIGHUP to the command upon changes (Note: using `-n` here we're executing `my_server` directly, instead of wrapping it in a shell:

    $ watchexec -n --signal SIGHUP my_server

Run `make` when any file changes, using the `.gitignore` file in the current directory to filter:

    $ watchexec make

Run `make` when any file in `lib` or `src` changes:

    $ watchexec -w lib -w src make

Run `bundle install` when the `Gemfile` changes:

    $ watchexec -w Gemfile bundle install

Run two commands:

    $ watchexec 'date; make'

Get desktop ("toast") notifications when the command starts and finishes:

    $ watchexec -N go build

Only run when files are created:

    $ watchexec --fs-events create -- s3 sync . s3://my-bucket

If you come from `entr`, note that the watchexec command is run in a shell by default. You can use `-n` or `--shell=none` to not do that:

    $ watchexec -n -- echo ';' lorem ipsum

On Windows, you may prefer to use Powershell:

    $ watchexec --shell=pwsh -- Test-Connection example.com

You can eschew running commands entirely and get a stream of events to process on your own:

```console
$ watchexec --emit-events-to=json-stdin --only-emit-events

{"tags":[{"kind":"source","source":"filesystem"},{"kind":"fs","simple":"modify","full":"Modify(Data(Any))"},{"kind":"path","absolute":"/home/code/rust/watchexec/crates/cli/README.md","filetype":"file"}]}
{"tags":[{"kind":"source","source":"filesystem"},{"kind":"fs","simple":"modify","full":"Modify(Data(Any))"},{"kind":"path","absolute":"/home/code/rust/watchexec/crates/lib/Cargo.toml","filetype":"file"}]}
{"tags":[{"kind":"source","source":"filesystem"},{"kind":"fs","simple":"modify","full":"Modify(Data(Any))"},{"kind":"path","absolute":"/home/code/rust/watchexec/crates/cli/src/args.rs","filetype":"file"}]}
```

Print the time commands take to run:

```console
$ watchexec --timings -- make
[Running: make]
...
[Command was successful, lasted 52.748081074s]
```

## Installation

### Package manager

Watchexec is in many package managers. A full list of [known packages](../../doc/packages.md) is available,
and there may be more out there! Please contribute any you find to the list :)

Common package managers:

- Alpine: `$ apk add watchexec`
- ArchLinux: `$ pacman -S watchexec`
- Nix: `$ nix-shell -p watchexec`
- Debian/Ubuntu via [apt.cli.rs](https://apt.cli.rs): `$ apt install watchexec`
- Homebrew on Mac:  `$ brew install watchexec`
- Chocolatey on Windows: `#> choco install watchexec`

### [Binstall](https://github.com/cargo-bins/cargo-binstall)

    $ cargo binstall watchexec-cli

### Pre-built binaries

Use the download section on [Github](https://github.com/watchexec/watchexec/releases/latest)
or [the website](https://watchexec.github.io/downloads/) to obtain the package appropriate for your
platform and architecture, extract it, and place it in your `PATH`.

There are also Debian/Ubuntu (DEB) and Fedora/RedHat (RPM) packages.

Checksums and signatures are available.

### Cargo (from source)

Only the latest Rust stable is supported, but older versions may work.

    $ cargo install watchexec-cli

## Shell completions

Currently available shell completions:

- bash: `completions/bash` should be installed to `/usr/share/bash-completion/completions/watchexec`
- elvish: `completions/elvish` should be installed to `$XDG_CONFIG_HOME/elvish/completions/`
- fish: `completions/fish` should be installed to `/usr/share/fish/vendor_completions.d/watchexec.fish`
- nu: `completions/nu` should be installed to `$XDG_CONFIG_HOME/nu/completions/`
- powershell: `completions/powershell` should be installed to `$PROFILE/`
- zsh: `completions/zsh` should be installed to `/usr/share/zsh/site-functions/_watchexec`

If not bundled, you can generate completions for your shell with `watchexec --completions <shell>`.

## Manual

There's a manual page at `doc/watchexec.1`. Install it to `/usr/share/man/man1/`.
If not bundled, you can generate a manual page with `watchexec --manual > /path/to/watchexec.1`, or view it inline with `watchexec --manual` (requires `man`).

You can also [read a text version](../../doc/watchexec.1.md) or a [PDF](../../doc/watchexec.1.pdf).

Note that it is automatically generated from the help text, so it is not as pretty as a carefully hand-written one.
