#!/usr/bin/env python3
"""Offline tmux/PTY acceptance with synthetic harnesses, never provider accounts.

Uses a dedicated temporary tmux server; no user settings or existing sessions.
"""
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import signal
import shutil
import struct
import subprocess
import sys
import tempfile
import termios
import time


def claim_controlling_terminal():
    # Linux tmux attach requires /dev/tty, not merely an isatty() stdin fd.
    # Run only in the single-threaded fixture's freshly forked child.
    os.setsid()
    fcntl.ioctl(0, termios.TIOCSCTTY, 0)


def check(binary, provider, interrupt=False, survive_interrupt=False):
    # /tmp keeps Unix socket paths below macOS's length limit.
    with tempfile.TemporaryDirectory(prefix="mmh-test-", dir="/tmp") as temporary:
        root = Path(temporary)
        bin_dir = root / "bin"
        bin_dir.mkdir()
        fake = bin_dir / provider
        fake.write_text("""#!/bin/sh
trap 'exit 130' INT
if [ "$SPONSOR_TEST_SURVIVE_INTERRUPT" = "1" ]; then
  trap 'printf "INTERRUPT_SURVIVED\\n"' INT
fi
printf 'HARNESS_ARGS:'
printf '<%s>' "$@"
printf '\\n'
attempt=0
while [ ! -S "$SPONSOR_SHELL_HOOK_SOCKET" ] && [ "$attempt" -lt 50 ]; do
  sleep 0.05
  attempt=$((attempt + 1))
done
printf '%s' '{"hook_event_name":"PermissionRequest","prompt":"PRIVATE_FIXTURE","transcript_path":"/do-not-read"}' | "$SPONSOR_TEST_BINARY" harness-event "$SPONSOR_SHELL_HARNESS"
printf 'PERMISSION_PROMPT_VISIBLE\\n'
while ! read -r reply; do :; done
exit 37
""")
        fake.chmod(0o700)
        env = os.environ.copy()
        for key in list(env):
            if key.startswith("SPONSOR_SHELL_") or key == "TMUX":
                del env[key]
        env.update({
            "PATH": str(bin_dir) + os.pathsep + env.get("PATH", ""),
            "TMUX_TMPDIR": temporary, "TMPDIR": temporary,
            "TERM": "xterm-256color", "CI": "1",
            "SPONSOR_TEST_BINARY": str(binary),
            "SPONSOR_TEST_SURVIVE_INTERRUPT": "1" if survive_interrupt else "0",
            "SPONSOR_SHELL_CONFIG": str(root / "absent-config.json"),
            "SPONSOR_SHELL_AD_FILE": str(root / "absent-ad.json"),
            "SPONSOR_SHELL_IDLE_FULLSCREEN_SECONDS": "1",
            "SPONSOR_SHELL_AD_WHILE_WORKING": "1",
        })

        def tmux(*args, required=True):
            result = subprocess.run(["tmux", *args], env=env, cwd=root,
                                    capture_output=True, text=True, timeout=5)
            if required:
                assert result.returncode == 0, result.stderr
            return result.stdout.strip()

        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 36, 100, 0, 0))
        child = None
        try:
            tmux("-f", "/dev/null", "new-session", "-d", "-s", "test-bootstrap")
            tmux("set-option", "-g", "default-shell", "/bin/sh")
            # A stale server environment must never override the new wrapper channel.
            tmux("set-environment", "-g", "SPONSOR_SHELL_HARNESS", "wrong")
            tmux("set-environment", "-g", "SPONSOR_SHELL_HOOK_SOCKET", "/missing/stale")
            child = subprocess.Popen([str(binary), "harness", provider, "--", "a b", "literal;$(no-exec)'"],
                                     env=env, cwd=root, stdin=slave, stdout=slave, stderr=slave,
                                     preexec_fn=claim_controlling_terminal)
            os.close(slave)
            slave = None
            session = f"sponsor-shell-{child.pid}"
            terminal_tail = bytearray()

            def drain_terminal(timeout=0):
                if select.select([master], [], [], timeout)[0]:
                    try:
                        terminal_tail.extend(os.read(master, 65536))
                        del terminal_tail[:-4096]
                    except OSError:
                        pass

            def wait_for(condition, timeout=8):
                deadline = time.monotonic() + timeout
                while time.monotonic() < deadline:
                    drain_terminal()
                    assert child.poll() is None, f"wrapper exited early: {child.returncode}; fixture output: {bytes(terminal_tail)!r}"
                    if condition():
                        return
                    time.sleep(0.05)
                raise AssertionError("timed out waiting for fixture terminal state")

            def capture(pane):
                return tmux("capture-pane", "-p", "-t", f"{session}:0.{pane}", required=False)

            wait_for(lambda: "recent hook: permission requested" in capture(0))
            preview = capture(0)
            assert "Sponsored preview: RAILWAY" in preview, preview
            assert "https://railway.app" in preview, preview
            assert "[Sponsored terminal time]" in preview, preview
            app = capture(1)
            assert "PERMISSION_PROMPT_VISIBLE" in app, app
            assert "<a b><literal;$(no-exec)'>" in app, app
            assert "PRIVATE_FIXTURE" not in capture(0) + app
            # Past the 1s idle expansion threshold and 10s hint lease: still split.
            # The hook label expires, but the permission prompt is still on
            # screen, so the phase must keep saying so. Reporting "activity
            # unavailable" here was wrong: the harness is demonstrably waiting.
            wait_for(
                lambda: "waiting on you" in capture(0)
                and "recent hook" not in capture(0),
                timeout=12,
            )
            assert tmux("display-message", "-p", "-t", session, "#{window_zoomed_flag}") == "0"
            assert tmux("display-message", "-p", "-t", f"{session}:0.1", "#{pane_active}") == "1"
            fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", 18, 48, 0, 0))
            # Explicitly notify the attached client on both supported platforms.
            client_pid = int(tmux("list-clients", "-t", session, "-F", "#{client_pid}"))
            os.kill(client_pid, signal.SIGWINCH)
            wait_for(lambda: tmux("display-message", "-p", "-t", session, "#{window_width}") == "48")
            assert "PERMISSION_PROMPT_VISIBLE" in capture(1)
            assert tmux("display-message", "-p", "-t", session, "#{window_zoomed_flag}") == "0"
            # Four ad rows cannot hold all preview essentials: no cropped sponsor.
            wait_for(lambda: "Sponsor hidden:" in capture(0))
            assert "railway.app" not in capture(0)
            if interrupt:
                # Clicking the ad pane must not strand Ctrl-C away from the app.
                tmux("select-pane", "-t", f"{session}:0.0")
                tmux("send-keys", "-t", f"{session}:0.0", "C-c")
                if survive_interrupt:
                    wait_for(lambda: "INTERRUPT_SURVIVED" in capture(1))
                    assert tmux("display-message", "-p", "-t", f"{session}:0.1", "#{pane_active}") == "1"
                    tmux("send-keys", "-t", f"{session}:0.1", "q", "Enter")
            else:
                tmux("send-keys", "-t", f"{session}:0.1", "q", "Enter")
            deadline = time.monotonic() + 5
            while child.poll() is None and time.monotonic() < deadline:
                drain_terminal(0.05)
            expected_exit = 130 if interrupt and not survive_interrupt else 37
            assert child.poll() == expected_exit, f"unexpected exit: {child.poll()}, app: {capture(1)!r}, ad: {capture(0)!r}, panes: {tmux('list-panes', '-t', session, '-F', '#{pane_index}:#{pane_active}:#{pane_current_command}', required=False)!r}"
            assert not list(root.glob("mmh-*")), "wrapper did not clean its local channel"
            print(json.dumps({"provider": provider, "pty": "passed", "exit": expected_exit,
                              "survive_interrupt": survive_interrupt,
                              "hook_privacy": "passed", "stale_hint_no_takeover": "passed"}))
        finally:
            if child and child.poll() is None:
                child.terminate()
                child.wait(timeout=5)
            # This server was created above beneath our private temporary directory.
            tmux("kill-server", required=False)
            os.close(master)
            if slave is not None:
                os.close(slave)


if __name__ == "__main__":
    if not shutil.which("tmux"):
        raise SystemExit("tmux is required for offline PTY acceptance")
    executable = Path(sys.argv[1] if len(sys.argv) > 1 else "target/debug/sponsor-shell").resolve(strict=True)
    for selected in ["claude", "codex"]:
        check(executable, selected)
    check(executable, "codex", interrupt=True)
    check(executable, "codex", interrupt=True, survive_interrupt=True)
