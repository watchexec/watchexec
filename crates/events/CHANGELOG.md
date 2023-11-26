# Changelog

## Next (YYYY-MM-DD)

## v1.1.0 (2023-11-26)

- Add `ProcessEnd::into_exitstatus` testing-only utility method.
- Deps: upgrade to Notify 6.0
- Deps: upgrade to nix 0.27

## v1.0.0 (2023-03-18)

- Split off new `watchexec-events` crate (this one), to have a lightweight library that can parse
  and generate events and maintain the JSON event format.
