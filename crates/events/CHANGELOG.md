# Changelog

## Next (YYYY-MM-DD)

## v5.0.1 (2025-05-15)

- Deps: remove unused dependency `nix` ([#930](https://github.com/watchexec/watchexec/pull/930))

## v5.0.0 (2025-02-09)

## v4.0.0 (2024-10-14)

- Deps: nix 0.29

## v3.0.0 (2024-04-20)

- Deps: nix 0.28

## v2.0.1 (2023-11-29)

- Add `ProcessEnd::into_exitstatus` testing-only utility method.
- Deps: upgrade to Notify 6.0
- Deps: upgrade to nix 0.27
- Deps: upgrade to watchexec-signals 2.0.0

## v2.0.0 (2023-11-29)

Same as 2.0.1, but yanked.

## v1.1.0 (2023-11-26)

Same as 2.0.1, but yanked.

## v1.0.0 (2023-03-18)

- Split off new `watchexec-events` crate (this one), to have a lightweight library that can parse
  and generate events and maintain the JSON event format.
