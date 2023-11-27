# Changelog

## Next (YYYY-MM-DD)

## v2.0.0 (2023-11-27)

- Depend on `watchexec-events` instead of the `watchexec` re-export.

## v1.2.1 (2023-05-14)

- Use IO-free dunce::simplify to normalise paths on Windows.
- Known regression: some filtering patterns misbehave slightly on Windows with paths outside the project root.
  - As filters were previously completely broken on Windows, this is still considered an improvement.

## v1.2.0 (2023-03-18)

- Ditch MSRV policy. The `rust-version` indication will remain, for the minimum estimated Rust version for the code features used in the crate's own code, but dependencies may have already moved on. From now on, only latest stable is assumed and tested for. ([#510](https://github.com/watchexec/watchexec/pull/510))

## v1.1.0 (2023-01-09)

- MSRV: bump to 1.61.0

## v1.0.0 (2022-06-23)

- Initial release as a separate crate.
