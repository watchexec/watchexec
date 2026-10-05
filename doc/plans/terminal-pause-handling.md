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
- D5: Delivery is out of scope by design, not deferred: pausing the command
  must not pause the supervisor, exactly as ^Z on a shell's foreground job
  does not pause the shell. The supervisor's own lifecycle is already handled
  by existing CLI behaviour: a catchable suspension of watchexec (SIGTSTP) is
  forwarded to the command group before watchexec suspends itself
  (`job.signal(TerminalSuspend)` then `suspend_self()`), and watchexec's own
  continuation is passed on as SIGCONT (`Signal::Continue` events flow
  through the signal map to `job.signal(Continue)`), which the pause watcher
  observes to re-grant the terminal. Uncatchable suspensions (SIGSTOP to
  watchexec) cannot be forwarded; the command keeps running while watchexec
  is frozen.
- D6: The supervisor records the command's pause state (the signal that
  paused it) and manages it during normal operations: stopping or restarting
  a paused command continues it first, because signals sent to a stopped
  process only take effect once it continues — graceful termination of a
  paused command would otherwise pend until the force-kill timeout. The
  recorded state is cleared on observed continuations, on our own successful
  continuations, and on every new run.

## Implementation (supervisor only; CLI unaffected)

1. `foreground.rs`: a `foreground_owner()` helper returning the controlling
   terminal's foreground process group, if any.
2. `pause_watch_loop`: add WCONTINUED to the peek and consume `waitid` flags;
   forward `WaitStatus::Continued` as the SIGCONT signal number.
3. `handle_pause_event`: a SIGCONT branch implementing D2; the D3 ownership
   guard in the SIGTTIN/SIGTTOU branch; D4 log wording; pause state recorded
   and cleared per D6.
4. `resume_paused_command()`: continue a paused command, called at the top of
   every stop or restart control path.
5. `SpawnOptions::grant_foreground` doc: one factual sentence on the
   suspend/reclaim and resume/re-grant behaviour.

## Verification notes

Unit tests cannot reach the watcher (test builds stub the child); behaviour
is verified manually under a pty with supervisor debug logging: grant on tty
touch, reclaim on SIGSTOP/SIGTSTP, re-grant on SIGCONT, and no grant attempts
while watchexec is backgrounded.
