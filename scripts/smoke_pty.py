#!/usr/bin/env python3
"""End-to-end pty smoke harness for the real rockettui binary.

Usage: python3 -I scripts/smoke_pty.py <binary> <scenario>

Standard library only. The binary has no fake data: every scenario that starts the app runs it
against `start_mock`, a stdlib mock Rocket.Chat on an ephemeral 127.0.0.1 port that serves the
JSON fixtures under src/api/fixtures and answers 401 to a wrong X-User-Id or X-Auth-Token. The
first frame is an empty sidebar (D-05), so `wait_ready` waits until the rooms have arrived and
the mode badge is drawn; `wait_drawn` waits for the badge only.
Prints `SMOKE OK: <scenario>` and exits 0, or `SMOKE FAIL: <reason>` and exits 1.
"""

import atexit
import codecs
import datetime
import fcntl
import http.server
import json
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
import threading
import time
import unicodedata
import urllib.parse

ALT_ON = b"\x1b[?1049h"
ALT_OFF = b"\x1b[?1049l"
PASTE_ON = b"\x1b[?2004h"
PASTE_OFF = b"\x1b[?2004l"

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FIXTURES = os.path.join(REPO, "src", "api", "fixtures")
MOCK_USER_ID = "smoke-user"
MOCK_TOKEN = "SENTINEL-TOKEN-9c41"
CREDENTIAL_VARS = ("ROCKETTUI_SERVER_URL", "ROCKETTUI_USER_ID", "ROCKETTUI_TOKEN")


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


# Mock Rocket.Chat endpoint -> fixture file.
MOCK_ROUTES = {
    "/api/v1/subscriptions.get": "subscriptions.json",
    "/api/v1/rooms.get": "rooms.json",
    "/api/v1/users.getPreferences": "preferences.json",
}


# history endpoint -> the subscription type it serves, like the real server's room-type check.
HISTORY_ENDPOINTS = {
    "/api/v1/channels.history": "c",
    "/api/v1/groups.history": "p",
    "/api/v1/im.history": "d",
}
HISTORY_COUNT_CAP = 100  # the server's API_Upper_Count_Limit


def _subscriptions():
    with open(os.path.join(FIXTURES, "subscriptions.json"), encoding="utf-8") as handle:
        return {sub["rid"]: sub for sub in json.load(handle)["update"]}


def _iso(base, index):
    when = base + datetime.timedelta(seconds=30 * index)
    return when.strftime("%Y-%m-%dT%H:%M:%S.") + f"{when.microsecond // 1000:03d}Z"


def history_messages(rid, name):
    """All messages of a room, oldest first. r-general has 130 (`message 000`..`message 129`,
    authors alternating alice and bob), every other room 5 (`<name> note N`)."""
    base = datetime.datetime(2026, 11, 16, 8, 55, 0, tzinfo=datetime.timezone.utc)
    out = []
    count = 130 if rid == "r-general" else 5
    for i in range(count):
        if rid == "r-general":
            ident, text = f"g-{i:03d}", f"message {i:03d}"
        else:
            ident, text = f"{rid}-{i}", f"{name} note {i}"
        user = "alice" if i % 2 == 0 else "bob"
        out.append(
            {
                "_id": ident,
                "rid": rid,
                "msg": text,
                "ts": _iso(base, i),
                "u": {"_id": f"u-{user}", "username": user},
            }
        )
    return out


def history_reply(path, query):
    """(status, body) for one history request: 400 error-room-not-found when the endpoint does
    not match the room type, else the newest `count` (capped) messages strictly older than
    `latest`, newest first."""
    sub = _subscriptions().get(query.get("roomId", ""))
    if sub is None or sub.get("t") != HISTORY_ENDPOINTS[path]:
        return 400, {"success": False, "error": "error-room-not-found"}
    msgs = history_messages(sub["rid"], sub.get("name") or sub["rid"])
    if "latest" in query:
        msgs = [m for m in msgs if m["ts"] < query["latest"]]
    try:
        count = int(query.get("count", "20"))
    except ValueError:
        count = 20
    count = max(0, min(count, HISTORY_COUNT_CAP))
    page = list(reversed(msgs))[:count]
    return 200, {"messages": page, "success": True}


def start_mock(mode="ok"):
    """Start a mock Rocket.Chat on 127.0.0.1 with an ephemeral port, in a daemon thread.

    Returns the server: `base_url` is `http://127.0.0.1:<port>` and `requests` is a list of
    `(method, path, query_dict, authed_bool)`, one per request received (any method). Only GET is
    served; every other method gets 405 (and is recorded). A request without the right
    X-User-Id and X-Auth-Token gets 401. The server is shut down at exit.
    """

    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def reply(self, status, body):
            data = json.dumps(body).encode() if not isinstance(body, bytes) else body
            try:
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)
            except (BrokenPipeError, ConnectionResetError):
                pass  # the app quit or panicked while a request was in flight

        def handle_any(self):
            parsed = urllib.parse.urlsplit(self.path)
            query = dict(urllib.parse.parse_qsl(parsed.query))
            authed = (
                self.headers.get("X-User-Id") == MOCK_USER_ID
                and self.headers.get("X-Auth-Token") == MOCK_TOKEN
            )
            self.server.requests.append((self.command, parsed.path, query, authed))
            if self.command != "GET":
                self.reply(405, {"success": False})
            elif not authed:
                self.reply(
                    401, {"success": False, "error": "You must be logged in to do this."}
                )
            elif parsed.path in HISTORY_ENDPOINTS:
                self.reply(*history_reply(parsed.path, query))
            elif parsed.path in MOCK_ROUTES:
                with open(os.path.join(FIXTURES, MOCK_ROUTES[parsed.path]), "rb") as handle:
                    self.reply(200, handle.read())
            else:
                self.reply(404, {"success": False})

        do_GET = do_POST = do_PUT = do_DELETE = do_PATCH = do_HEAD = handle_any

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    server.requests = []
    server.mode = mode
    server.base_url = f"http://127.0.0.1:{server.server_address[1]}"
    threading.Thread(target=server.serve_forever, daemon=True).start()
    atexit.register(server.shutdown)
    return server


def spawn(binary, extra_env=None, config_files=None, server_url=None):
    """Fork the binary onto a fresh 80x24 pty. Returns (pid, master_fd).

    XDG_CONFIG_HOME always points at a fresh temp dir (empty unless `config_files` is given), so
    the developer's real config never changes a scenario. The three ROCKETTUI_* credential
    variables are removed first, so the developer's real credentials never reach a scenario; with
    `server_url` they are then set to that URL and the mock's user id and token.
    """
    config_home = make_config_dir(config_files or {})
    pid, fd = pty.fork()
    if pid == 0:
        try:
            fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
            os.environ["TERM"] = "xterm-256color"
            os.environ["XDG_CONFIG_HOME"] = config_home
            os.environ["NO_PROXY"] = os.environ["no_proxy"] = "127.0.0.1,localhost"
            for key in CREDENTIAL_VARS:
                os.environ.pop(key, None)
            if server_url is not None:
                os.environ["ROCKETTUI_SERVER_URL"] = server_url
                os.environ["ROCKETTUI_USER_ID"] = MOCK_USER_ID
                os.environ["ROCKETTUI_TOKEN"] = MOCK_TOKEN
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


def screen_of(buf):
    """The screen model of everything read so far."""
    screen = Screen()
    screen.feed(buf)
    return screen


def wait_ready(fd, buf, room=b"general"):
    """The shared readiness check: `room` is listed (it arrived from the mock) and NORMAL is drawn.

    D-05 draws an empty sidebar first, so there is no ordering to check.
    """
    text = room.decode()
    ready = lambda b: screen_of(b).contains(text) and screen_of(b).contains("NORMAL")
    if not read_until(fd, buf, ready, 10):
        raise SmokeFail("not ready")


def wait_drawn(fd, buf):
    """Wait only for the first draw (the NORMAL badge), before any room has arrived."""
    if not read_until(fd, buf, lambda b: b"NORMAL" in b, 10):
        raise SmokeFail("first frame never drawn")


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
    mock = start_mock()
    pid, fd = spawn(binary, server_url=mock.base_url)
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
    mock = start_mock()
    # The debug panic fires after the first draw, before the rooms arrive: wait for the draw only.
    pid, fd = spawn(binary, {"ROCKETTUI_DEBUG_PANIC": "1"}, server_url=mock.base_url)
    buf = bytearray()
    wait_drawn(fd, buf)
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
    mock = start_mock()
    pid, fd = spawn(binary, server_url=mock.base_url)
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
    mock = start_mock()
    pid, fd = spawn(
        binary,
        config_files={"config.toml": 'message_layout = "two_line"\n'},
        server_url=mock.base_url,
    )
    buf = bytearray()
    wait_ready(fd, buf)
    type_keys(fd, buf, b"\r")  # opens general
    # The room has no history yet, so the header is checked on a local message from `me`.
    type_keys(fd, buf, b"i")
    type_keys(fd, buf, b"layout check")
    type_keys(fd, buf, b"\r")
    os.write(fd, b"\x1b")  # a lone ESC; the pause keeps crossterm from reading Alt+key
    time.sleep(0.3)

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


def scenario_config_keymap(binary):
    # keymap.toml rebinds ";" to enter_command, so ";q<CR>" must quit through Command mode.
    mock = start_mock()
    pid, fd = spawn(
        binary,
        config_files={"keymap.toml": '[normal]\n";" = "enter_command"\n'},
        server_url=mock.base_url,
    )
    buf = bytearray()
    wait_ready(fd, buf)
    type_keys(fd, buf, b";")

    def command_mode(b):
        screen = Screen()
        screen.feed(b)
        return screen.contains("COMMAND")

    # ratatui redraws only changed cells, so the badge is read from the screen model.
    if not read_until(fd, buf, command_mode, 5):
        raise SmokeFail("Command mode was never entered through the rebound key")
    type_keys(fd, buf, b"q")
    type_keys(fd, buf, b"\r")
    code = wait_exit(pid, fd, buf, 10)
    if code != 0:
        raise SmokeFail(f"exit code {code}, expected 0")
    if ALT_OFF not in bytes(buf):
        raise SmokeFail("alternate screen not left")


def scenario_config_error(binary):
    files = {
        "config.toml": 'sidebar_width = 99\ntimestamp_format = "%Q"\n',
        "keymap.toml": "[normal\n",
    }
    # Valid credentials pointing nowhere, so only the intended config errors show.
    pid, fd = spawn(binary, config_files=files, server_url="http://127.0.0.1:9")
    buf = bytearray()

    def shown(b):
        screen = Screen()
        screen.feed(b)
        return all(
            screen.contains(text)
            for text in ("invalid configuration", "sidebar_width", "timestamp_format", "keymap.toml")
        )

    if not read_until(fd, buf, shown, 10):
        raise SmokeFail("error screen not shown")
    type_keys(fd, buf, b"x")
    code = wait_exit(pid, fd, buf, 10)
    if code != 1:
        raise SmokeFail(f"exit code {code}, expected 1")
    data = bytes(buf)
    if not first_before(buf, ALT_ON, ALT_OFF):
        raise SmokeFail("alternate screen not entered then left")
    if b"sidebar_width" not in data[data.rfind(ALT_OFF) :]:
        raise SmokeFail("report not printed to stderr after the terminal was restored")
    if b"NORMAL" in data:
        raise SmokeFail("the app started although the config is invalid")


def scenario_connect_read(binary):
    mock = start_mock()
    pid, fd = spawn(binary, server_url=mock.base_url)
    buf = bytearray()
    wait_ready(fd, buf)
    wanted = (
        "general",
        "dev-team",
        "alice",
        "random",
        "platform",
        "Design review",
        "secrets",
        "Favorites",
        "Work",
    )
    listed = lambda b: all(screen_of(b).contains(text) for text in wanted)
    if not read_until(fd, buf, listed, 10):
        missing = [t for t in wanted if not screen_of(buf).contains(t)]
        raise SmokeFail(f"rooms never listed, missing: {missing}")
    screen = screen_of(buf)
    for hidden in ("carol", "visitor", "invited-room", "orphan"):
        if screen.contains(hidden):
            raise SmokeFail(f"skipped subscription {hidden!r} is listed")
    if not mock.requests:
        raise SmokeFail("the mock received no request")
    if any(method != "GET" for method, _path, _query, _authed in mock.requests):
        raise SmokeFail(f"a non-GET request was sent: {mock.requests}")
    if not all(authed for _method, _path, _query, authed in mock.requests):
        raise SmokeFail("a request reached the server without valid credentials")
    data = bytes(buf)
    if MOCK_TOKEN.encode() in data:
        raise SmokeFail("the token appears in the terminal output")
    if b"SENTINEL-LASTMSG" in data:
        raise SmokeFail("a lastMessage body appears in the terminal output")
    # Enter on the first row (general) opens it and fetches its newest page, once.
    type_keys(fd, buf, b"\r")
    if not read_until(fd, buf, lambda b: screen_of(b).contains("message 129"), 10):
        raise SmokeFail("the newest history message never appeared")
    history_requests = [r for r in mock.requests if r[1] in HISTORY_ENDPOINTS]
    general = [r for r in history_requests if r[2].get("roomId") == "r-general"]
    if len(general) != 1 or general[0][1] != "/api/v1/channels.history":
        raise SmokeFail(f"expected one channels.history request for r-general: {general}")
    if "latest" in general[0][2]:
        raise SmokeFail("the first page was requested with `latest`")
    # Ctrl-w h back to the sidebar, G to the last row (secrets), Enter opens it.
    type_keys(fd, buf, b"\x17")
    type_keys(fd, buf, b"h")
    type_keys(fd, buf, b"G")
    type_keys(fd, buf, b"\r")
    status_has_secrets = lambda b: "secrets" in screen_of(b).rows()[22]
    if not read_until(fd, buf, status_has_secrets, 10):
        raise SmokeFail("the encrypted room never opened")
    shows_marker = lambda b: screen_of(b).contains("encrypted, not supported")
    if not read_until(fd, buf, shows_marker, 5):
        raise SmokeFail("the encrypted placeholder was never shown")
    read_until(fd, buf, lambda b: False, 0.5)  # a stray request would be sent by now
    if any(r[2].get("roomId") == "r-secrets" for r in mock.requests):
        raise SmokeFail("a request was sent for the encrypted room")
    if any(method != "GET" for method, _path, _query, _authed in mock.requests):
        raise SmokeFail(f"a non-GET request was sent: {mock.requests}")
    type_keys(fd, buf, b":q")
    type_keys(fd, buf, b"\r")
    code = wait_exit(pid, fd, buf, 10)
    if code != 0:
        raise SmokeFail(f"exit code {code}, expected 0")
    if MOCK_TOKEN.encode() in bytes(buf):
        raise SmokeFail("the token appears in the terminal output")


def scenario_missing_creds(binary):
    # No server_url, so none of the three variables is set and there is no config file.
    pid, fd = spawn(binary)
    buf = bytearray()

    def shown(b):
        screen = screen_of(b)
        return all(
            screen.contains(text)
            for text in ("invalid configuration", "server_url", "user_id", "token")
        )

    if not read_until(fd, buf, shown, 10):
        raise SmokeFail("error screen not shown")
    type_keys(fd, buf, b"x")
    code = wait_exit(pid, fd, buf, 10)
    if code != 1:
        raise SmokeFail(f"exit code {code}, expected 1")
    data = bytes(buf)
    if b"ROCKETTUI_TOKEN" not in data[data.rfind(ALT_OFF) :]:
        raise SmokeFail("report not printed to stderr after the terminal was restored")
    if b"NORMAL" in data:
        raise SmokeFail("the app started although credentials are missing")


SCENARIOS = {
    "quit": scenario_quit,
    "panic": scenario_panic,
    "insert": scenario_insert,
    "config_layout": scenario_config_layout,
    "config_keymap": scenario_config_keymap,
    "config_error": scenario_config_error,
    "connect_read": scenario_connect_read,
    "missing_creds": scenario_missing_creds,
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
