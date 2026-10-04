# Plan: `--wrap-process=auto` — platform default wrap + JIT terminal foreground grant

## Background

Programs that interact with the terminal (pagers, TUIs, password prompts, REPLs) do not
work under the default `--wrap-process=group` on Linux: the command is placed in a process
group that is not the foreground group of the controlling terminal, so the kernel stops the
program with SIGTTOU the moment it calls `tcsetattr` (and with SIGTTIN when it reads the
tty), before it writes anything. The user sees no output and the run hangs. Verified
end-to-end on 2026-10-05 with `jj log`/`less -FRXK` under the real binary; strace shows
`ioctl(TCSETSW2) = ERESTARTSYS; --- stopped by SIGTTOU ---`. Relevant kernel logic:
`__tty_check_change()` in `drivers/tty/tty_jobctrl.c` (and `tiocspgrp()`, which begins with
that same check). `--wrap-process=none` (shares watchexec's foreground group) and
`--wrap-process=session` (`setsid`, no controlling terminal — the check's first line is
`if (current->signal->tty != tty) return 0;`) both avoid it. Ignoring SIGTTIN/SIGTTOU in
the child is not a fix: ignored SIGTTOU allows termios ops, but ignored SIGTTIN makes tty
reads return `EIO`, so TUIs render then die on the first keypress.

The full fix is a PTY (separate effort). This plan ships the interim: detect the stop at
runtime and grant the child the terminal foreground just-in-time, so `auto` behaves like
today's platform default until the exact moment a program demands the tty.

## Decisions (maintainer-signed)

- D1: New `WrapMode::Auto`, which becomes the clap default on all platforms. `auto`
  resolves to today's platform behaviour — `session` on macOS, `group` on other unix, Job
  Object on Windows — **plus** the JIT grant where it can apply (unix, grouped).
- D2: Explicit `--wrap-process={group,session,none}` is the opt-out and behaves exactly as
  today, except `group` additionally gets stop detection + a warning (no grant), since
  that's the mode where the hang occurs.
- D3: The grant is unix-only. On Windows, `auto` == today (Job Object; no tty stop signals
  exist). On macOS, `auto` == `session` == today (no controlling terminal, so the grant is
  inert under the default; it can only fire if a user explicitly picks `group`).
- D4: Grant is disabled when `keyboard_events` is enabled (i.e. `--stdin-quit` or
  `--interactive`): those put watchexec itself in raw mode reading stdin, which both
  competes with the child for the tty and would SIGTTIN-stop watchexec's own reader once
  the child holds the foreground. `auto` degrades to plain `group` in that case; help text
  documents it.
- D5: Stops by SIGTSTP/SIGSTOP (deliberate suspension): reclaim the foreground, do **not**
  SIGCONT, print a notice. Stops by SIGTTIN/SIGTTOU with grant enabled: grant + SIGCONT.
- D6: termios snapshot at grant time, restore at reclaim, so a SIGKILLed raw-mode child
  doesn't leave the terminal broken.
- D7: SIGTTOU must be ignored **in watchexec, scoped** around its own tty ioctls
  (`tcsetpgrp` itself runs the tty-foreground check; a background+orphaned caller gets
  `-EIO`→`ENOTTY`). The ignore must never be active at spawn time, so children always
  inherit default dispositions (inherited SIG_IGN would silently change child job-control
  behaviour and mask the very stops we detect).

## Supervisor changes (`crates/supervisor`)

1. Unix-gated dependency: `nix` `0.31.1` (match process-wrap 9.1.1's version), features
   settled with `cargo add` (need process, signal, termios equivalents).
2. `SpawnOptions` gains two unix-relevant fields (default off, so library behaviour is
   unchanged unless opted in; docs already recommend `..Default::default()`):
   - `observe_stops: bool` — watch the running child for terminal stops (SIGTTIN/SIGTTOU/
     SIGTSTP) and report via the stop hook.
   - `grant_foreground: bool` — additionally grant the foreground + SIGCONT on
     TTIN/TTOU stops, reclaim (and D6-restore) on every run end. Implies `observe_stops`.
3. New unix-only module `foreground` (or `tty`): lazily open `/dev/tty`; `getpgrp` /
   `tcgetpgrp` / `tcsetpgrp` with D7-scoped SIGTTOU ignore; termios snapshot/restore.
   Any failure (no ctty, closed tty) disables the feature for the current run and fires
   the stop hook once with a "unavailable" notice; never errors the job.
4. Stop watcher: one std thread per running child, spawned after a successful spawn when
   observe/grant is on. Loop: `waitid(WSTOPPED | WNOWAIT | WNOHANG)` peek → consume the
   stop with `waitid(WSTOPPED | WNOHANG)` (never touches exit events, so tokio's reaping
   is undisturbed) → send over a tokio unbounded channel into the job task. Exit on
   `ECHILD` (child reaped) or a per-run cancel flag. No polling of `/proc`, no SIGCHLD
   interception.
5. Job task loop (`job/task.rs`): new select branch receiving stop events (active while
   running). On TTIN/TTOU with grant enabled+grouped: `tcsetpgrp(tty, child_pgid)` then
   `killpg(SIGCONT)`, fire stop hook (`GrantedForeground`). On other stops, and on every
   run-end path (`wait` → `Ok(true)`, `Stop`, `TryRestart`,
   `ContinueTryGracefulRestart`): if we hold the grant, reclaim
   (`tcsetpgrp(tty, watchexec_pgid)`) + restore termios snapshot, fire hook
   (`ReclaimedForeground` or `Stopped { signal }`).
6. Stop hook plumbing mirroring `ErrorHandler`: `sync_async_callbox!` enum, `Control::Set/
   UnsetSync|AsyncStopHook`, `Job::on_stop` / `on_stop_async` setters, cancellable.
7. Invariants (assert in code where cheap): grant only when `grouped && !session`; spawn
   paths never execute with SIGTTOU ignored in the supervisor process.
8. `lib.rs` theory-of-operation: document the foreground-grant lifecycle.

## CLI changes (`crates/cli`)

9. `args/command.rs`: `WrapMode::{Auto, Group, Session, None}` with `Auto` as clap default
   (`default_value = "auto"`); `WRAP_DEFAULT` repurposed/removed in favour of a resolution
   function used by config. Help text rewritten: what `auto` does (platform default + JIT
   grant), the Ctrl+C-while-granted semantics change, the opt-out story ("the old modes
   are all still here and behave exactly as before"), and the D4 keyboard caveat.
10. `config.rs` (SpawnOptions construction, ~line 1185): `Auto` → platform wrap + 
    `grant_foreground(true)` (unix; false on Windows); explicit `Group` → `grouped` +
    `observe_stops(true)` (warn-only); `Session`/`None` unchanged. When
    `keyboard_events`, force grant off (D4).
11. Stop hook → styled stderr notices in the `[Running: …]` banner family: under grant, a
    dim notice when the command takes the terminal (once per run) and what Ctrl+C now
    does; under explicit-group warnings, the SIGTTOU/SIGTTIN explanation plus "use
    --wrap-process=auto (the default) or =session" hint.
12. Terminal-hygiene audit: ensure any watchexec-side tty `tcsetattr` reachable while a
    grant is held (clearscreen reset path, exit-path restores) runs under D7-scoped
    SIGTTOU ignore. Escape-sequence-only paths need nothing.
13. Man page: run `bin/manpage` to regenerate `doc/watchexec.1{,.md}` from the new help.

## Commit sequence

- `plan:` this file.
- `feat(supervisor)`: SpawnOptions fields, foreground module, stop watcher, task-loop
  branch, hooks, docs.
- `feat(cli)`: Auto mode, config mapping, notices, help text.
- `docs(cli)`: regenerate man page.

Library note: `SpawnOptions` gains fields (struct literals become non-exhaustive);
release-plz will treat it as a breaking bump for `watchexec-supervisor` (5.x) per its
config — acceptable, the crate's own docs mandate `..Default::default()`.
