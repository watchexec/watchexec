# Changelog

## Next (YYYY-MM-DD)

## v1.4.0 (2024-04-28)

- Add out-of-tree Git repositories (`.git` file instead of folder).

## v1.3.0 (2024-01-01)

- Remove `README.md` files from detection; those were causing too many false positives and were a weak signal anyway.
- Add Node.js package manager lockfiles.

## v1.2.1 (2023-11-26)

- Deps: upgrade Tokio requirement to 1.33.0

## v1.2.0 (2023-01-08)

- Add `const` qualifier to `ProjectType::is_vcs` and `::is_soft`.
- Use Tokio's canonicalize instead of dunce.
- Add missing `Send` bound to `origins()` and `types()`.

## v1.1.1 (2022-09-07)

- Deps: update miette to 5.3.0

## v1.1.0 (2022-08-24)

- Add support for Go.
- Add support for Zig.
- Add `Pipfile` support for Pip.
- Add detection of `CONTRIBUTING.md`.
- Document what causes the detection of each project type.

## v1.0.0 (2022-06-16)

- Initial release as a separate crate.
