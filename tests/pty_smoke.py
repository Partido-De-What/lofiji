#!/usr/bin/env python3
"""Linux PTY integration checks. Run after cargo build; no Python packages needed."""
import fcntl
import os
from pathlib import Path
import pty
import signal
import struct
import subprocess
import tempfile
import termios

from pty_support import read_output, wait_for, wait_output

BIN = Path(__file__).resolve().parents[1] / "target/debug/lofiji"

def size(fd, cols, rows):
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))

with tempfile.TemporaryDirectory(prefix="lofiji-pty-") as tmp:
    for exit_mode in ("q", "ctrl-c", "sigterm", "sighup"):
        master, slave = pty.openpty()
        size(slave, 120, 36)
        baseline = termios.tcgetattr(slave)
        proc = subprocess.Popen([str(BIN), "--config", tmp + "/config.toml"], stdin=slave, stdout=slave, stderr=slave)
        try:
            output = wait_output(master, b"38;2;")
            assert b"\x1b[?1049h" in output, "alternate screen not entered"
            assert b"\x1b[?1003h" not in output, "mouse capture must remain disabled"
            assert b"38;2;" in output, "true color missing"
            assert not termios.tcgetattr(slave)[3] & termios.ICANON, "raw mode missing"
            os.write(master,b"w")
            path = Path(tmp, "config.toml")
            wait_for(master, lambda: path.exists() and 'weather = "rain"' in path.read_text())
            for cols, rows in [(80,24), (40,12), (20,6), (1,1), (120,36)]:
                size(slave, cols, rows)
                os.kill(proc.pid, signal.SIGWINCH)
                os.write(master, b"p ")
                read_output(master, .5)
                assert proc.poll() is None, "crashed while resizing"
            os.write(master, b"s")
            wait_output(master, b"SETTINGS")
            # Toggle 24h -> 12h and save, then exit from the main screen.
            os.write(master, b"\r \x13")
            wait_for(master, lambda: 'format_24h = false' in path.read_text())
            if exit_mode == "q": os.write(master, b"q")
            elif exit_mode == "ctrl-c": os.write(master, b"\x03")
            else: os.kill(proc.pid, signal.SIGTERM if exit_mode == "sigterm" else signal.SIGHUP)
            output = wait_output(master, b"\x1b[?1049l")
            assert proc.wait(timeout=2) == 0
            assert b"\x1b[?1049l" in output, "alternate screen not restored"

            assert termios.tcgetattr(slave) == baseline, "terminal flags not restored"
            print("PASS: resize, settings persistence, terminal restoration via", exit_mode)
        finally:
            if proc.poll() is None:
                proc.kill()
                proc.wait()
            os.close(master)
            os.close(slave)
        # Ensure every iteration starts with the same defaults.
        Path(tmp, "config.toml").unlink(missing_ok=True)

# A closed PTY must not leave an orphan spinning inside the event reader.
master, slave = pty.openpty()
size(slave, 80, 24)
proc = subprocess.Popen([str(BIN), "--config", "/tmp/lofiji-hangup-unused.toml"], stdin=slave, stdout=slave, stderr=slave)
os.close(slave)
wait_output(master, b"\x1b[?1049h")
os.close(master)
try:
    returncode = proc.wait(timeout=2)
    assert returncode in (0, 1), returncode
    print("PASS: closing the PTY exits without an orphan")
finally:
    if proc.poll() is None:
        proc.kill()
        proc.wait()

print("All PTY integration checks passed")
