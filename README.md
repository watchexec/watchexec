#watchexec

[![Build Status](https://travis-ci.org/mattgreen/watchexec.svg?branch=master)](https://travis-ci.org/mattgreen/watchexec)
[![Build status](https://ci.appveyor.com/api/projects/status/ivxu31g4rcf4740t?svg=true)](https://ci.appveyor.com/project/mattgreen/watchexec)

Software development often involves running the same commands over and over. Boring!

`watchexec` is a **simple**, standalone tool that watches a path and runs a command whenever it detects modifications.

Example use cases:

* Automatically run unit tests
* Run linters/syntax checkers

##Features

* Simple invocation and use
* Runs on OS X, Linux and Windows
* Monitors current directory and all subdirectories for changes (use `--watch` to override)
	* Uses most efficient event polling mechanism for your platform (except for [BSD](https://github.com/passcod/rsnotify#todo))
* Coalesces multiple filesystem events into one, for editors that use swap/backup files during saving
* By default, uses `.gitignore` to determine which files to ignore notifications for
* Support for watching files with a specific extension
* Support for filtering/ignoring events based on glob patterns
* Optionally clears screen between executions
* Optionally restarts the command with every modification (good for servers)
* Does not require a language runtime

##Anti-Features

* Not tied to any particular language or ecosystem
* Does not require a cryptic command line involving `xargs`

##Usage Examples

Watch all JavaScript, CSS and HTML files in the current directory and all subdirectories for changes, running `make` when a change is detected:

	$ watchexec --exts js,css,html make

Watch all files below `src` and subdirectories for changes, running `make test` when a change is detected:

    $ watchexec --watch src make test

Call `make test` when any file changes in this directory/subdirectory, except for everything below `target`:

    $ watchexec -i target make test

Call/restart `python server.py` when any Python file in the current directory (and all subdirectories) changes:

    $ watchexec -e py -r python server.py

Run `make` when any file changes, using the `.gitignore` file in the current directory to filter:

    $ watchexec make

##Installation

###Cargo

    $ cargo install watchexec

###OS X with Homebrew

    $ brew install https://raw.githubusercontent.com/mattgreen/watchexec/master/pkg/brew/watchexec.rb

###Linux

For now, use the GitHub Releases tab to obtain the binary. PRs for packaging in various distros are welcomed.

###Windows

Use the GitHub Releases tab to obtain the binary. In the future, I'll look at adding it to Chocolatey.

##Credits

* [notify](https://github.com/passcod/rsnotify) for doing most of the heavy-lifting
