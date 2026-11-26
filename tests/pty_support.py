"""Read PTY output and wait for observable test conditions with deadlines."""
import errno
import os
import re
import select
import time


def read_output(master, timeout=2, *, until=None):
    deadline = time.monotonic() + timeout
    output = bytearray()
    while True:
        if until is not None and until(output):
            break
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            break
        # Without a predicate, collect one burst and return as soon as it ends.
        wait = min(remaining, 0.035) if output and until is None else min(remaining, 0.05)
        readable, _, _ = select.select([master], [], [], wait)
        if readable:
            try:
                data = os.read(master, 65536)
            except OSError as error:
                if error.errno == errno.EIO:  # Linux PTY hangup.
                    break
                raise
            if not data:
                break
            output.extend(data)
        elif output and until is None:
            break
    return bytes(output)


def wait_output(master, marker, timeout=2, *, visible=False):
    ansi = re.compile(rb'\x1b\[[0-?]*[ -/]*[@-~]')
    def matches(data):
        return marker in (ansi.sub(b'', data) if visible else data)
    output = read_output(master, timeout, until=matches)
    assert matches(output), f"missing {marker!r} in PTY output: {output[-1000:]!r}"
    return output


def wait_for(master, predicate, timeout=2):
    output = read_output(master, timeout, until=lambda _: predicate())
    assert predicate(), f"condition not met in {timeout}s: {output[-1000:]!r}"
    return output

def terminate(master, proc):
    if proc.poll() is not None:
        return
    proc.terminate()
    read_output(master, timeout=2, until=lambda _: proc.poll() is not None)
    if proc.poll() is None:
        proc.kill()
    proc.wait(timeout=2)
