#!/bin/bash

set -euo pipefail

export WATCHEXEC_BIN=${WATCHEXEC_BIN:-watchexec}
python3 - <<'PY'
import errno
import os
import pty
import re
import selectors
import signal
import subprocess
import tempfile
import termios
import time
from pathlib import Path

clear_sequence = re.compile(rb"\x1b\[(?:2|3)J|\x1bc")


def check(clear_mode, startup_signal=False, interactive_start=False):
    with tempfile.TemporaryDirectory() as fixture:
        watched = Path(fixture)
        existing = watched / "existing"
        changed = watched / "changed"
        existing.write_text("existing")
        changed.write_text("changed")
        master, slave = pty.openpty()
        extra = ["--delay-run=2s", "--map-signal=USR1:"] if startup_signal else []
        if interactive_start:
            ready_marker = watched / "ready-marker"
            ready_marker.write_text("")
            extra += ["--postpone", "--interactive", "--workdir", str(watched),
                      "--ignore", ready_marker.name]
        environment = {**os.environ, "TERM": "xterm-256color", "NO_COLOR": "1"}
        if startup_signal:
            environment["WATCHEXEC_LOG"] = (
                "watchexec_cli::config=trace,watchexec_supervisor::job::task=trace"
            )
        if interactive_start:
            environment["WATCHEXEC_LOG"] = (
                "watchexec_cli::filterer=trace,watchexec_cli::config=debug,"
                "watchexec_filterer_globset=trace"
            )
        try:
            process = subprocess.Popen(
                [os.environ["WATCHEXEC_BIN"], "--watch", str(watched),
                 "--initial-events=create", "--print-events", clear_mode,
                 "--color=never", *extra, "-n", "--", "sh", "-c", "sleep 2"],
                stdin=slave, stdout=slave, stderr=slave,
                env=environment,
            )
        except BaseException:
            os.close(master)
            raise
        finally:
            os.close(slave)
        output = bytearray()
        selector = selectors.DefaultSelector()
        selector.register(master, selectors.EVENT_READ)

        def read_until(condition, stage, probe_marker=False):
            deadline = time.monotonic() + 10
            while not condition():
                remaining = deadline - time.monotonic()
                assert remaining > 0, f"No progress at {stage}: {output!r}"
                if probe_marker:
                    ready_marker.write_text("ready")
                if not selector.select(min(remaining, .05) if probe_marker else remaining):
                    continue
                try:
                    chunk = os.read(master, 65536)
                except OSError as error:
                    if error.errno != errno.EIO:
                        raise
                    chunk = b""
                assert chunk, f"PTY closed at {stage}: {output!r}"
                output.extend(chunk)

        def wait_for_run(count):
            read_until(lambda: output.count(b"[Command was successful]") >= count,
                       f"command {count} completion")
            assert output.count(b"[Running:") == count, output
            clears = list(clear_sequence.finditer(output))
            assert clears, f"Expected terminal clearing: {output!r}"
            return bytes(output[clears[-1].end():])

        try:
            if interactive_start:
                # A rejected marker event proves ordinary FS delivery passed the ready fence.
                # Overwrite an existing file to avoid parent-directory creation events.
                read_until(lambda: any(b"ignored by globset ignore" in line
                                       and str(ready_marker).encode() in line
                                       for line in output.splitlines()),
                           "filesystem ready", probe_marker=True)
                assert not termios.tcgetattr(master)[3] & termios.ICANON
                assert b"[Running:" not in output, "Postpone ran before the restart key"
                os.write(master, b"r")
                read_until(lambda: b"interactive: restart" in output, "restart key handled")
            if startup_signal:
                read_until(lambda: b"delaying run by sleeping inside the job" in output
                           and b"got control message" in output and b"AsyncFunc" in output,
                           "startup delay")
                assert b"[Running:" not in output
                offset = len(output)
                os.kill(process.pid, signal.SIGUSR1)
                read_until(lambda: b"discarding signal" in output[offset:]
                           and b"no filesystem or synthetic events, skip without doing more" in output[offset:],
                           "signal-only action")
                assert b"[Running:" not in output, "Signal arrived after the first spawn"
            first = wait_for_run(1)
            assert str(existing).encode() in first, "Initial path was erased by clearing"
            changed.write_text("modified")
            second = wait_for_run(2)
            assert any(b"[EVENT " in line and b"kind=Modify(" in line
                       and str(changed).encode() in line for line in second.splitlines()), \
                "Current modify event was not displayed"
            assert str(existing).encode() not in second, "Initial path was replayed"
        finally:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
            selector.close()
            os.close(master)
        assert process.returncode == 0, process.returncode
        case = f"{clear_mode} with delayed startup and signal" if startup_signal else clear_mode
        if interactive_start:
            case = f"{clear_mode} with postponed interactive first start"
        print(f"{case}: initial path survives clearing; next run does not replay it")


for mode in ("--clear", "--clear=reset"):
    check(mode)
check("--clear", startup_signal=True)
check("--clear", interactive_start=True)
PY
