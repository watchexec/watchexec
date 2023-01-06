[![Crates.io page](https://badgen.net/crates/v/watchexec-filterer-ignore)](https://crates.io/crates/watchexec-filterer-ignore)
[![API Docs](https://docs.rs/watchexec-filterer-ignore/badge.svg)][docs]
[![Crate license: Apache 2.0](https://badgen.net/badge/license/Apache%202.0)][license]
[![CI status](https://github.com/watchexec/watchexec/actions/workflows/check.yml/badge.svg)](https://github.com/watchexec/watchexec/actions/workflows/check.yml)

# Watchexec filterer: ignore

_(Sub)filterer implementation for ignore files._

- **[API documentation][docs]**.
- Licensed under [Apache 2.0][license].
- Minimum Supported Rust Version: 1.61.0 (incurs a minor semver bump).
- Status: maintained.

This is mostly a thin layer above the [ignore-files](../../ignore-files) crate, and is meant to be
used as part of another more general filterer. However, there's nothing wrong with using it
directly if all that's needed is to handle ignore files.

[docs]: https://docs.rs/watchexec-filterer-ignore
[license]: ../../../LICENSE
