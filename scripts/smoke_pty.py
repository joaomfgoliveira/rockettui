#!/usr/bin/env python3
"""End-to-end pty smoke harness for the real rockettui binary.

Usage: python3 -I scripts/smoke_pty.py <binary> <scenario>

Standard library only. Every scenario starts with `wait_ready`, which also proves that the
seeded rooms were applied before the first draw (general precedes NORMAL in the byte stream).
Prints `SMOKE OK: <scenario>` and exits 0, or `SMOKE FAIL: <reason>` and exits 1.
"""

import atexit
import codecs
import fcntl
import os
import pty
import re
import select
import shutil
import signal
import struct
import sys
import tempfile
import termios
import time
import unicodedata

ALT_ON = b"\x1b[?1049h"
ALT_OFF = b"\x1b[?1049l"
PASTE_ON = b"\x1b[?2004h"
PASTE_OFF = b"\x1b[?2004l"


class SmokeFail(Exception):
    pass


class Screen:
    """A minimal 24x80 screen model.

    ratatui only emits the cells that changed, so a typed word arrives split by cursor moves. This
    applies CUP moves (CSI row;col H), ignores every other CSI and OSC sequence, writes printable
    characters at the cursor (a wide character takes two columns) and exposes the grid as text.
    """

    def __init__(self, rows=24, cols=80):
        self.nrows = rows
        self.ncols = cols
        self.grid = [[" "] * cols for _ in range(rows)]
        self.row = 0
        self.col = 0
        self.state = "text"
        self.params = ""
        self.decoder = codecs.getincrementaldecoder("utf-8")(errors="replace")

    def feed(self, data):
        for ch in self.decoder.decode(bytes(data)):
            self.step(ch)

    def step(self, ch):
        if self.state == "text":
            if ch == "\x1b":
                self.state = "esc"
            elif ch == "\r":
                self.col = 0
            elif ch == "\n":
                self.row = min(self.row + 1, self.nrows - 1)
            elif ch == "\b":
                self.col = max(self.col - 1, 0)
            elif ch >= " " and ch != "\x7f":
                self.put(ch)
        elif self.state == "esc":
            if ch == "[":
                self.state = "csi"
                self.params = ""
            elif ch == "]":
                self.state = "osc"
            else:
                self.state = "text"  # a two-byte escape such as ESC 7 or ESC =
        elif self.state == "csi":
            if "\x40" <= ch <= "\x7e":
                if ch in ("H", "f"):
                    self.move(self.params)
                self.state = "text"
            else:
                self.params += ch
        elif self.state == "osc":
            if ch == "\x07":
                self.state = "text"
            elif ch == "\x1b":
                self.state = "esc"  # ESC \ ends the string; the backslash ends the escape

    def move(self, params):
        parts = params.split(";")
        try:
            row = int(parts[0]) if parts[0] else 1
            col = int(parts[1]) if len(parts) > 1 and parts[1] else 1
        except ValueError:
            return
        self.row = min(max(row - 1, 0), self.nrows - 1)
        self.col = min(max(col - 1, 0), self.ncols - 1)

    def put(self, ch):
        if unicodedata.combining(ch):
            return
        width = 2 if unicodedata.east_asian_width(ch) in ("W", "F") else 1
        if self.col + width <= self.ncols:
            self.grid[self.row][self.col] = ch
            if width == 2:
                self.grid[self.row][self.col + 1] = " "
        self.col = min(self.col + width, self.ncols)

    def rows(self):
        return ["".join(r) for r in self.grid]

    def contains(self, text):
        return any(text in r for r in self.rows())


def make_config_dir(files):
    """Create a temp XDG_CONFIG_HOME holding `rockettui/<name>` for each `name -> text` of `files`.

    Removed at exit. Returns the base directory (the value for XDG_CONFIG_HOME).
    """
    base = tempfile.mkdtemp(prefix="rockettui-smoke-")
    atexit.register(shutil.rmtree, base, ignore_errors=True)
    config = os.path.join(base, "rockettui")
    os.mkdir(config)
    for name, text in files.items():
        with open(os.path.join(config, name), "w", encoding="utf-8") as handle:
            handle.write(text)
    return base


def spawn(binary, extra_env=None, config_files=None):
    """Fork the binary onto a fresh 80x24 pty. Returns (pid, master_fd).

    XDG_CONFIG_HOME always points at a fresh temp dir (empty unless `config_files` is given), so
    the developer's real config never changes a scenario.
    """
    config_home = make_config_dir(config_files or {})
    pid, fd = pty.fork()
    if pid == 0:
        try:
            fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
            os.environ["TERM"] = "xterm-256color"
            os.environ["XDG_CONFIG_HOME"] = config_home
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


def type_keys(fd, buf, data):
    """Like `send`, but keeps draining output during each 20 ms gap."""
    for i in range(len(data)):
        os.write(fd, data[i : i + 1])
        read_some(fd, buf, 0.02)


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


def scenario_panic(binary):
    pid, fd = spawn(binary, {"ROCKETTUI_DEBUG_PANIC": "1"})
    buf = bytearray()
    wait_ready(fd, buf)
    code = wait_exit(pid, fd, buf, 10)
    if code != 101:
        raise SmokeFail(f"exit code {code}, expected 101")
    restored = bytes(buf).find(ALT_OFF)
    message = bytes(buf).find(b"panicked at")
    if restored == -1:
        raise SmokeFail("alternate screen never left")
    if message == -1:
        raise SmokeFail("no panic message printed")
    if restored > message:
        raise SmokeFail("panic message printed before the terminal was restored")


def scenario_insert(binary):
    pid, fd = spawn(binary)
    buf = bytearray()
    wait_ready(fd, buf)
    # Enter opens the selected sidebar room (general), i enters Insert, Enter sends the message.
    type_keys(fd, buf, b"\r")
    type_keys(fd, buf, b"i")
    type_keys(fd, buf, b"hello smoke")
    type_keys(fd, buf, b"\r")

    def sent(b):
        screen = Screen()
        screen.feed(b)
        return screen.contains("hello smoke")

    if not read_until(fd, buf, sent, 5):
        raise SmokeFail("typed message never appeared on the screen")
    if b"INSERT" not in bytes(buf):
        raise SmokeFail("INSERT mode never shown")
    # A lone ESC followed by a pause, so crossterm does not read ESC plus the next byte as Alt+key.
    os.write(fd, b"\x1b")
    time.sleep(0.3)
    type_keys(fd, buf, b":q")
    type_keys(fd, buf, b"\r")
    code = wait_exit(pid, fd, buf, 10)
    if code != 0:
        raise SmokeFail(f"exit code {code}, expected 0")
    if ALT_OFF not in bytes(buf):
        raise SmokeFail("alternate screen not left")


def scenario_config_layout(binary):
    pid, fd = spawn(binary, config_files={"config.toml": 'message_layout = "two_line"\n'})
    buf = bytearray()
    wait_ready(fd, buf)
    type_keys(fd, buf, b"\r")  # opens general

    def screen_of(b):
        screen = Screen()
        screen.feed(b)
        return screen

    two_line = lambda b: any(
        re.search(r"[┃│][a-z]+  \d\d:\d\d", row) for row in screen_of(b).rows()
    )
    if not read_until(fd, buf, two_line, 5):
        raise SmokeFail("two-line message header never appeared")
    if any(re.search(r"[┃│]\d\d:\d\d [a-z]", row) for row in screen_of(buf).rows()):
        raise SmokeFail("compact message row shown with message_layout = two_line")
    type_keys(fd, buf, b":q")
    type_keys(fd, buf, b"\r")
    code = wait_exit(pid, fd, buf, 10)
    if code != 0:
        raise SmokeFail(f"exit code {code}, expected 0")
    if ALT_OFF not in bytes(buf):
        raise SmokeFail("alternate screen not left")


SCENARIOS = {
    "quit": scenario_quit,
    "panic": scenario_panic,
    "insert": scenario_insert,
    "config_layout": scenario_config_layout,
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
