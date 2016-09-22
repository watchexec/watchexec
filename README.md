#watchexec

Software development often involves running the same commands over and over. Boring!

`watchexec` is a **simple**, standalone tool that watches a path and runs a command whenever it detects modifications.

Example use cases:

* Automatically run unit tests
* Run linters/syntax checkers

##Status

Beta: CLI arguments subject to change

##Features

* Simple invocation and use
* Runs on OS X, Linux and Windows
* Monitors path specified on command line for changes
	* Uses most efficient event polling mechanism, based on platform (except for [BSD](https://github.com/passcod/rsnotify#todo))
* Coalesces multiple filesystem events into one, for editors that use swap/backup files during saving
* Support for filtering/ignoring events based on glob patterns
* Optionally clears screen between executions
* Does not require a language runtime
* Small (~100 LOC)

##Anti-Features

* Not tied to any particular language or ecosystem
* Does not require a cryptic command line involving `xargs`

##Usage

Call `make test` when there are any changes in the `src` directory:

    $ watchexec src make test

Call `make test` when any Python file changes in this directory, or a subdirectory:

    $ watchexec -f '*.py' . make test

Call `make test` when any file changes in this directory/subdirectory, except for everything below `target`:

    $ watchexec -i target . make test

Always quote glob patterns (*.py)!

##Installation

###OS X with Homebrew

    $ brew install mattgreen/watchexec/watchexec

##Credits

* [notify](https://github.com/passcod/rsnotify) for doing most of the heavy-lifting
