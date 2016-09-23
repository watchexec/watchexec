#watchexec

Software development often involves running the same commands over and over. Boring!

`watchexec` is a **simple**, standalone tool that watches a path and runs a command whenever it detects modifications.

Example use cases:

* Automatically run unit tests
* Run linters/syntax checkers

##Status

Beta: CLI arguments stabilizing

##Features

* Simple invocation and use
* Runs on OS X, Linux and Windows
* Monitors path specified on command line for changes
	* Uses most efficient event polling mechanism for your platform (except for [BSD](https://github.com/passcod/rsnotify#todo))
* Coalesces multiple filesystem events into one, for editors that use swap/backup files during saving
* Support for watching files with a specific extension
* Support for filtering/ignoring events based on glob patterns
* Optionally clears screen between executions
* Does not require a language runtime
* Small (~100 LOC)

##Anti-Features

* Not tied to any particular language or ecosystem
* Does not require a cryptic command line involving `xargs`

##Usage Examples

Watch all JavaScript, CSS and HTML files in the current directory and all subdirectories for changes, running `make` when a change is detected:

	$ watchexec --exts js,css,html make

Watch all files below `src` and subdirectories for changes, running `make test` when a change is detected:

    $ watchexec --watch src make test

Call `make test` when any file changes in this directory/subdirectory, except for everything below `target`:

    $ watchexec -i target make

##Installation

###OS X with Homebrew

    $ brew install mattgreen/watchexec/watchexec

##Credits

* [notify](https://github.com/passcod/rsnotify) for doing most of the heavy-lifting
