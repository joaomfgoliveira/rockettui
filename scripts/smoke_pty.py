#!/usr/bin/env python3
"""End-to-end pty smoke harness for the real rockettui binary.

Usage: python3 -I scripts/smoke_pty.py <binary> <scenario>

Standard library only. Every scenario starts with `wait_ready`, which also proves that the
seeded rooms were applied before the first draw (general precedes NORMAL in the byte stream).
Prints `SMOKE OK: <scenario>` and exits 0, or `SMOKE FAIL: <reason>` and exits 1.
"""

import fcntl
import os
import pty
import select
import signal
import struct
import sys
import termios
import time

ALT_ON = b"\x1b[?1049h"
ALT_OFF = b"\x1b[?1049l"
PASTE_ON = b"\x1b[?2004h"
PASTE_OFF = b"\x1b[?2004l"


class SmokeFail(Exception):
    pass


def spawn(binary, extra_env=None):
    """Fork the binary onto a fresh 80x24 pty. Returns (pid, master_fd)."""
    pid, fd = pty.fork()
    if pid == 0:
        try:
            fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
            os.environ["TERM"] = "xterm-256color"
            for key, value in (extra_env or {}).items():
                os.environ[key] = value
            os.execv(binary, [binary])
        except BaseException:
            os._exit(127)
    return pid, fd


def read_some(fd, buf, wait):
    """Append whatever the master has within `wait` seconds. Returns False at end of output."""
    ready, _, _ = select.select([fd], [], [], wait)
    if not ready:
        return True
    try:
        chunk = os.read(fd, 65536)
    except OSError:  # EIO once the child has exited
        return False
    if not chunk:
        return False
    buf.extend(chunk)
    return True


def read_until(fd, buf, pred, timeout):
    """Read until pred(buf) holds. Returns True on success, False on timeout or end of output."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if pred(buf):
            return True
        if not read_some(fd, buf, 0.1):
            return pred(buf)
    return pred(buf)


def wait_ready(fd, buf):
    """The shared readiness check used by every scenario."""
    both = lambda b: b"general" in b and b"NORMAL" in b
    if not read_until(fd, buf, both, 10):
        raise SmokeFail("not ready")
    if bytes(buf).find(b"general") > bytes(buf).find(b"NORMAL"):
        raise SmokeFail("seed not applied before first draw")


def send(fd, data):
    """Write one byte at a time with 20 ms gaps, like a typist."""
    for i in range(len(data)):
        os.write(fd, data[i : i + 1])
        time.sleep(0.02)


def wait_exit(pid, fd, buf, timeout):
    """Keep draining output while waiting for exit. Returns the exit code."""
    deadline = time.monotonic() + timeout
    alive = True
    while time.monotonic() < deadline:
        if alive:
            alive = read_some(fd, buf, 0.1)
        else:
            time.sleep(0.05)
        done, status = os.waitpid(pid, os.WNOHANG)
        if done:
            while read_some(fd, buf, 0.05):
                pass
            return os.waitstatus_to_exitcode(status)
    os.kill(pid, signal.SIGKILL)
    os.waitpid(pid, 0)
    raise SmokeFail("process did not exit in time")


def first_before(buf, first, second):
    a = bytes(buf).find(first)
    b = bytes(buf).find(second)
    return a != -1 and b != -1 and a < b


def scenario_quit(binary):
    pid, fd = spawn(binary)
    buf = bytearray()
    wait_ready(fd, buf)
    send(fd, b"j")
    send(fd, b"k")
    send(fd, b":q")
    send(fd, b"\r")
    code = wait_exit(pid, fd, buf, 10)
    if code != 0:
        raise SmokeFail(f"exit code {code}, expected 0")
    if not first_before(buf, ALT_ON, ALT_OFF):
        raise SmokeFail("alternate screen not entered then left")
    if not first_before(buf, PASTE_ON, PASTE_OFF):
        raise SmokeFail("bracketed paste not enabled then disabled")


SCENARIOS = {
    "quit": scenario_quit,
}


def main():
    if len(sys.argv) != 3 or sys.argv[2] not in SCENARIOS:
        print(f"usage: smoke_pty.py <binary> <{'|'.join(SCENARIOS)}>", file=sys.stderr)
        return 2
    binary = os.path.abspath(sys.argv[1])
    name = sys.argv[2]
    try:
        SCENARIOS[name](binary)
    except SmokeFail as exc:
        print(f"SMOKE FAIL: {exc}")
        return 1
    print(f"SMOKE OK: {name}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
