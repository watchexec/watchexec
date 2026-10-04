# Plan: terminal pause handling for the foreground grant

## Background

#1138 shipped `--wrap-process=auto`: when the kernel pauses the command with
SIGTTIN/SIGTTOU for touching the terminal while backgrounded, the supervisor
grants it the terminal foreground and continues it, reclaiming on run end.
Deliberate suspensions (SIGSTOP/SIGTSTP) reclaim the terminal but leave the
command paused with no resume path: a continued command re-pauses on its next
terminal access instead of regaining the foreground. This plan covers the
resume path and guard rails around granting. Terminology: "stop" remains
taken by graceful termination; a process frozen by SIGSTOP/SIGTSTP/SIGTTIN/
SIGTTOU is "paused".

## Decisions

- D1: The pause watcher also observes continuations: `waitid` gains
  WCONTINUED in its peek and consume flags, and a continuation is forwarded
  to the job task as a SIGCONT marker over the existing pause channel.
- D2: On a command continuation, the supervisor re-grants the terminal
  foreground iff the grant is not currently held (our own post-grant SIGCONTs
  must not re-trigger) and the controlling terminal's foreground group is
  either watchexec's own process group (hand over the foreground we hold,
  e.g. after a reclaim) or the command's process group (re-assert after an
  external `fg`-style transfer). Otherwise it logs and does nothing: never
  take the terminal from a foreign group, such as the user's shell after
  `bg`.
- D3: The same ownership guard applies to the just-in-time grant on
  SIGTTIN/SIGTTOU: grant only from the foreground we hold, or re-assert the
  command's own. Without the guard, a backgrounded watchexec would steal the
  terminal from whatever group the user is using.
- D4: Deliberate suspensions (SIGSTOP/SIGTSTP) reclaim the terminal and leave
  the command paused, as in #1138; logs distinguish SIGTSTP (terminal
  generated) from SIGSTOP and other stops.
- D5: Non-goals for this change: propagating a child's self-suspension into
  watchexec's own job control (a ^Z that lands on the child suspends only the
  child; the terminal is reclaimed and watchexec keeps running), and resuming
  a command whose pause event was consumed before watchexec itself was
  suspended and resumed (no new event fires on watchexec's own resume). Both
  need cross-crate plumbing — the signal source already delivers
  `Signal::Continue` events when watchexec itself is continued, so the natural
  extension is a supervisor `Job` method driven from the CLI's action handler
  — or the pty work. Documented as limitations, not built here.

## Implementation (supervisor only; CLI unaffected)

1. `foreground.rs`: a `foreground_owner()` helper returning the controlling
   terminal's foreground process group, if any.
2. `pause_watch_loop`: add WCONTINUED to the peek and consume `waitid` flags;
   forward `WaitStatus::Continued` as the SIGCONT signal number.
3. `handle_pause_event`: a SIGCONT branch implementing D2; the D3 ownership
   guard in the SIGTTIN/SIGTTOU branch; D4 log wording.
4. `SpawnOptions::grant_foreground` doc: one factual sentence on the
   suspend/reclaim and resume/re-grant behaviour.

## Verification notes

Unit tests cannot reach the watcher (test builds stub the child); behaviour
is verified manually under a pty with supervisor debug logging: grant on tty
touch, reclaim on SIGSTOP/SIGTSTP, re-grant on SIGCONT, and no grant attempts
while watchexec is backgrounded.
