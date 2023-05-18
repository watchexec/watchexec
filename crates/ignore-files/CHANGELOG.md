# Changelog

## Next (YYYY-MM-DD)

## v1.3.0 (2023-05-14)

- Use IO-free dunce::simplify to normalise paths on Windows.
- Handle gitignores correctly (one GitIgnoreBuilder per path).
- Deps: update gix-config to 0.22.

## v1.2.0 (2023-03-18)

- Deps: update git-config to gix-config.
- Deps: update tokio to 1.24
- Ditch MSRV policy (only latest supported now).
- `from_environment()` no longer looks at `WATCHEXEC_IGNORE_FILES`.

## v1.1.0 (2023-01-08)

- Add missing `Send` bound to async functions.

## v1.0.1 (2022-09-07)

- Deps: update git-config to 0.7.1
- Deps: update miette to 5.3.0

## v1.0.0 (2022-06-16)

- Initial release as a separate crate.
