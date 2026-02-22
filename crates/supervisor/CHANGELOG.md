# Changelog

## Next (YYYY-MM-DD)

## v5.1.0 (2026-02-22)

- Add `is_running()` and clarify what `is_dead()` is measuring

## v5.0.2 (2026-01-20)

- Deps: process-wrap 9
- Fix: handle graceful stop when job handle dropped (#981, #982)

## v5.0.1 (2025-05-15)

## v5.0.0 (2025-05-15)

- Deps: process-wrap 8.2.1

## v4.0.0 (2025-02-09)

## v3.0.0 (2024-10-14)

- Deps: nix 0.29

## v2.0.0 (2024-04-20)

- Deps: replace command-group with process-wrap
- Deps: nix 0.28

## v1.0.3 (2023-12-19)

- Fix Start executing even when the job is running.
- Add kill-on-drop to guarantee no two processes run at the same time.

## v1.0.2 (2023-12-09)

- Add `trace`-level logging to Job task.

## v1.0.1 (2023-11-29)

- Deps: watchexec-events 2.0.1
- Deps: watchexec-signals 2.0.0

## v1.0.0 (2023-11-26)

- Initial release as a separate crate.
