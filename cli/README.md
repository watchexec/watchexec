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

    $ watchexec -r -s SIGKILL my_server

Send a SIGHUP to the command upon changes (Note: using `-n` here we're executing `my_server` directly, instead of wrapping it in a shell:

    $ watchexec -n -s SIGHUP my_server

Run `make` when any file changes, using the `.gitignore` file in the current directory to filter:

    $ watchexec make

Run `make` when any file in `lib` or `src` changes:

    $ watchexec -w lib -w src make

Run `bundle install` when the `Gemfile` changes:

    $ watchexec -w Gemfile bundle install

Run two commands:

    $ watchexec 'date; make'

If you come from `entr`, note that the watchexec command is run in a shell by default. You can use `-n` or `--shell=none` to not do that:

    $ watchexec -n -- echo ';' lorem ipsum

On Windows, you may prefer to use Powershell:

    $ watchexec --shell=powershell -- test-connection localhost

## Installation

### First-party

#### Pre-built

Use the download section on **[the website](https://watchexec.github.io/downloads/)** to obtain the
package appropriate for your platform and architecture, extract it, and place it in your `PATH`.

There are also Debian/Ubuntu (DEB) and Fedora/RedHat (RPM) packages.

Checksums and signatures are available.

#### Cargo (from source)

Requires Rust 1.58.0 or later.

    $ cargo install watchexec-cli

#### [Binstall](https://github.com/ryankurte/cargo-binstall)

    $ cargo binstall watchexec-cli

### Third-party

These are provided by third parties, caveat emptor!

#### macOS

- Homebrew:  `$ brew install watchexec`
- Nix/NixOS: `$ nix-env -iA nixpkgs.watchexec`
- Webi:      `$ curl -sS https://webinstall.dev/watchexec | bash`

#### Linux

- ArchLinux: `$ pacman -S watchexec`
- Nix/NixOS: `$ nix-env -iA nixpkgs.watchexec`
- Webi:      `$ curl -sS https://webinstall.dev/watchexec | bash`

#### Windows

- Scoop:      `#> scoop install watchexec`
- Chocolatey: `#> choco install watchexec`
- Webi:       `#> curl.exe -A MS https://webinstall.dev/watchexec | powershell`

## Shell completions

Currently available shell completions:

- zsh: `completions/zsh` should be installed to `/usr/share/zsh/site-functions/_watchexec`
