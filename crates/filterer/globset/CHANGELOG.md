# Changelog

## Next (YYYY-MM-DD)

## v8.0.0 (2025-05-15)

## v7.0.0 (2025-02-09)

## v6.0.0 (2024-10-14)

- Deps: watchexec 5

## v5.0.0 (2024-10-13)

- Add whitelist parameter.

## v4.0.1 (2024-04-28)

- Hide fmt::Debug spew from ignore crate, use `full_debug` feature to restore.

## v4.0.0 (2024-04-20)

- Deps: watchexec 4

## v3.0.0 (2024-01-01)

- Deps: `watchexec-filterer-ignore` and `ignore-files`

## v2.0.1 (2023-12-09)

- Depend on `watchexec-events` instead of the `watchexec` re-export.

## v1.2.0 (2023-03-18)

- Ditch MSRV policy. The `rust-version` indication will remain, for the minimum estimated Rust version for the code features used in the crate's own code, but dependencies may have already moved on. From now on, only latest stable is assumed and tested for. ([#510](https://github.com/watchexec/watchexec/pull/510))

## v1.1.0 (2023-01-09)

- MSRV: bump to 1.61.0

## v1.0.1 (2022-09-07)

- Deps: update miette to 5.3.0

## v1.0.0 (2022-06-23)

- Initial release as a separate crate.
