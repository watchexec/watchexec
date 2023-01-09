# Changelog

## Next (YYYY-MM-DD)

## v2.1.0 (2023-01-08)

- MSRV: bump to 1.61.0
- Deps: drop explicit dependency on `libc` on Unix.
- Internal: remove all usage of `dunce`, replaced with either Tokio's `canonicalize` (properly async) or [normalize-path](https://docs.rs/normalize-path) (performs no I/O).
- Internal: drop support code for Fuchsia. MIO already didn't support it, so it never compiled there.
- Add `#[must_use]` annotations to a bunch of functions.
- Add missing `Send` bound to `HandlerLock`.
- Fix `summarise_events_to_env` on Windows to output paths with backslashes.

## v2.0.2 (2022-09-07)

- Deps: upgrade to miette 5.3.0

## v2.0.1 (2022-09-07)

- Deps: upgrade to Notify 5.0.0

## v2.0.0 (2022-06-17)

First "stable" release of the library.

- **Change: the library is split into even more crates**
    - Two new low-level crates, `project-origins` and `ignore-files`, extract standalone functionality
    - Filterers are now separate crates, so they can evolve independently (faster) to the main library crate
    - These five new crates live in the watchexec monorepo, rather than being completely separate like `command-group` and `clearscreen`
    - This makes the main library bit less likely to change as often as it did, so it was finally time to release 2.0.0!

- **Change: the Action worker now launches a set of Commands**
    - A new type `Command` replaces and augments `Shell`, making explicit which style of calling will be used
    - The action working data now takes a `Vec<Command>`, so multiple commands to be run as a set
    - Commands in the set are run sequentially, with an error interrupting the sequence
    - It is thus possible to run both "shelled" and "raw exec" commands in a set
    - `PreSpawn` and `PostSpawn` handlers are run per Command, not per command set
    - This new style should be preferred over sending command lines like `cmd1 && cmd2`

- **Change: the event queue is now a priority queue**
    - Shutting down the runtime is faster and more predictable. No more hanging after hitting Ctrl-C if there's tonnes of events coming in!
    - Signals sent to the main process have higher priority
    - Events marked "urgent" skip filtering entirely
    - SIGINT, SIGTERM, and Ctrl-C on Windows are marked urgent
        - This means it's no longer possible to accidentally filter these events out
        - They still require handling in `on_action` to do anything
    - The API for the `Filterer` trait changes slightly to let filterers use event priority

- Improvement: the main subtasks of the runtime are now aborted on error
- Improvement: the event queue is explicitly closed when shutting down
- Improvement: the action worker will check if the event queue is closed more often, to shutdown early
- Improvement: `kill_on_drop` is set on Commands, which will be a little more eager to terminate processes when we're done with them
- Feature: `Outcome::Sleep` waits for a given duration ([#79](https://github.com/watchexec/watchexec/issues/79))

Other miscellaneous:

- Deps: add the `log` feature to tracing so logs can be emitted to `log` subscribers
- Deps: upgrade to Tokio 1.19
- Deps: upgrade to Miette 4
- Deps: upgrade to Notify 5.0.0-pre.15

- Docs: fix the main example in lib.rs ([#297](https://github.com/watchexec/watchexec/pull/297))
- Docs: describe a tuple argument in the globset filterer interface
- Docs: the library crate gains a file-based CHANGELOG.md (and won't go in the Github releases tab anymore)
- Docs: the library's readme's code block example is now checked as a doc-test

- Meta: PRs are now merged by Bors

## v2.0.0-pre.14 (2022-04-04)

- Replace git2 dependency by git-config ([#267](https://github.com/watchexec/watchexec/pull/267)). This makes using the library more pleasant and will also avoid library version mismatch errors when the libgit2 library updates on the system.

## v2.0.0-pre.13 (2022-03-18)

- Revert backend switch on mac from previous release. We'll do it a different way later ([#269](https://github.com/watchexec/watchexec/issues/269))

## v2.0.0-pre.12 (2022-03-16)

- Upgraded to [Notify pre.14](https://github.com/notify-rs/notify/releases/tag/5.0.0-pre.14)
- Internal change: kqueue backend is used on mac. This _should_ reduce or eliminate some old persistent bugs on mac, and improve response times, but please report any issues you have!
- `Watchexec::new()` now reports the library's version at debug level
- Notify version is now specified with an exact (`=`) requirement, to avoid breakage ([#266](https://github.com/watchexec/watchexec/issues/266))

## v2.0.0-pre.11 (2022-03-07)

- New `error::FsWatcherError` enum split off from `RuntimeError`, and with additional variants to take advantage of targeted help text for known inotify errors on Linux
- Help text is now carried through elevated errors properly
- Globset filterer: `extensions` and `filters` are now cooperative rather than exclusionary. That is, a filters of `["Gemfile"]` and an extensions of `["js", "rb"]` will match _both_ `Gemfile` and `index.js` rather than matching nothing at all. This restores pre 2.0 behaviour.
- Globset filterer: on unix, a filter of `*/file` will match both `file` and `dir/file` instead of just `dir/file`. This is a compatibility fix and is incorrect behaviour which will be removed in the future. Do not rely on it.

## v2.0.0-pre.10 (2022-02-07)

- The `on_error` handler gets an upgraded parameter which lets it upgrade (runtime) errors to critical.
- `summarize_events_to_paths` now deduplicates paths within each variable.

## v2.0.0-pre.9 (2022-01-31)

- `Action`, `PreSpawn`, and `PostSpawn` structs passed to handlers now contain an `Arc<[Event]>` instead of an `Arc<Vec<Event>>`
- `Outcome` processing (the final bit of an action) now runs concurrently, so it doesn't block further event processing ([#247](https://github.com/watchexec/watchexec/issues/247), and to a certain extent, [#241](https://github.com/watchexec/watchexec/issues/241))

## v2.0.0-pre.8 (2022-01-26)

- Fix: globset filterer should pass all non-path events ([#248](https://github.com/watchexec/watchexec/pull/248))

## v2.0.0-pre.7 (2022-01-26) [YANKED]

**Yanked for critical bug in globset filterer (fixed in pre.8) on 2022-01-26**

- Fix: typo in logging/errors ([#242](https://github.com/watchexec/watchexec/pull/242))
- Globset: an extension filter should fail all paths that are about folders ([#244](https://github.com/watchexec/watchexec/issues/244))
- Globset: in the case of an event with multiple paths, any pass should pass the entire event
- Removal: `filter::check_glob` and `error::GlobParseError`

## v2.0.0-pre.6 (2022-01-19)

First version of library v2 that was used in a CLI release.

- Globset filterer was erroneously passing files with no extension when an extension filter was specified

## v2.0.0-pre.5 (2022-01-18)

- Update MSRV (to 1.58) and policy (bump incurs minor semver only)
- Some bugfixes around canonicalisation of paths
- Eliminate context-less IO errors
- Move error types around
- Prep library readme
- Update deps

## v2.0.0-pre.4 (2022-01-16)

- More logging, especially around ignore file discovery and filtering
- The const `paths::PATH_SEPARATOR` is now public, being `:` on Unix and `;` and Windows.
- Add Subversion to discovered ProjectTypes
- Add common (sub)Filterer for ignore files, so they benefit from a single consistent implementation. This also makes ignore file discovery correct and efficient by being able to interpret ignore files which searching for ignore files, or in other words, _not_ descending into directories which are ignored.
- Integrate this new IgnoreFilterer into the GlobsetFilterer and TaggedFilterer. This does mean that some old v1 behaviour of patterns in gitignores will not behave quite the same now, but that was arguably always a bug. The old "buggy" v1 behaviour around folder filtering remains for manual filters, which are those most likely to be surprising if "fixed".

## v2.0.0-pre.3 (2021-12-29)

- [`summarise_events_to_env`](https://docs.rs/watchexec/2.0.0-pre.3/watchexec/paths/fn.summarise_events_to_env.html) used to return `COMMON_PATH`, it now returns `COMMON`, in keeping with the other variable names.

## v2.0.0-pre.2 (2021-12-29)

- [`summarise_events_to_env`](https://docs.rs/watchexec/2.0.0-pre.2/watchexec/paths/fn.summarise_events_to_env.html) returns a `HashMap<&str, OsString>` rather than `HashMap<&OsStr, OsString>`, because the expectation is that the variable names are processed, e.g. in the CLI: `WATCHEXEC_{}_PATH`. `OsStr` makes that painful for no reason (the strings are static anyway).
- The [`Action`](https://docs.rs/watchexec/2.0.0-pre.2/watchexec/action/struct.Action.html) struct's `events` field changes to be an `Arc<Vec<Event>>` rather than a `Vec<Event>`: the intent is for the events to be immutable/read-only (and it also made it easier/cheaper to implement the next change below).
- The [`PreSpawn`](https://docs.rs/watchexec/2.0.0-pre.2/watchexec/action/struct.PreSpawn.html) and [`PostSpawn`](https://docs.rs/watchexec/2.0.0-pre.2/watchexec/action/struct.PostSpawn.html) structs got a new `events: Arc<Vec<Event>>` field so these handlers get read-only access to the events that triggered the command.

## v2.0.0-pre.1 (2021-12-21)

- MSRV bumped to 1.56
- Rust 2021 edition
- More documentation around tagged filterer:
	- `==` and `!=` are case-insensitive
	- the mapping of matcher to tags
	- the mapping of matcher to auto op
- Finished the tagged filterer:
	- Proper path glob matching
	- Signal matching
	- Process completion matching
	- Allowlisting pattern works
	- More matcher aliases to the parser
	- Negated filters
	- Some silly filter parsing bugs
	- File event kind matching
	- Folder filtering (main confusing behaviour in v1)
- Lots of tests:
	- Globset filterer
	- Including the "buggy"/confusing behaviour of v1, for parity/compat
	- Tagged filterer:
		- Paths
		- Including verifying that the v1 confusing behaviour is fixed
		- Non-path filters
		- Filter parsing
	- Ignore files
	- Filter scopes
	- Outcomes
	- Change reporting in the environment
		- ...Specify behaviour a little more precisely through that process
- Prepare the watchexec event type to be serializable
	- A synthetic `FileType`
	- A synthetic `ProcessEnd` (`ExitStatus` replacement)
- Some ease-of-use improvements, mainly removing generics when overkill

## v2.0.0-pre.0 (2021-10-17)

- Placeholder release of v2 library (preview)

## v1.17.1 (2021-07-22)

- Process handling code replaced with the new [command-group](https://github.com/watchexec/command-group) crate.
- [#158](https://github.com/watchexec/watchexec/issues/158) New option `use_process_group` (default `true`) allows disabling use of process groups.
- [#168](https://github.com/watchexec/watchexec/issues/168) Default debounce time further decreased to 100ms.
- Binstall configuration and transitional `cargo install watchexec` stub removed.

## v1.16.1 (2021-07-10)

- [#200](https://github.com/watchexec/watchexec/issues/200): Expose when the process is done running
- [`ba26999`](https://github.com/watchexec/watchexec/commit/ba26999028cfcac410120330800a9a9026ca7274) Pin globset to 0.4.6 to avoid breakage due to a bugfix in 0.4.7

## v1.16.0 (2021-05-09)

- Initial release as a separate crate.
