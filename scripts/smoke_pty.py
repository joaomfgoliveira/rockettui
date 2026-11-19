#!/usr/bin/env python3
"""End-to-end pty smoke harness for the real rockettui binary.

Usage: python3 -I scripts/smoke_pty.py <binary> <scenario>

Standard library only. The binary has no fake data: every scenario that starts the app runs it
against `start_mock`, a stdlib mock Rocket.Chat on an ephemeral 127.0.0.1 port that serves the
JSON fixtures under src/api/fixtures and answers 401 to a wrong X-User-Id or X-Auth-Token. It
also serves POST /api/v1/chat.sendMessage: it records each body, rejects any key but `_id`, `rid`
and `msg` (the exact wire contract), rejects a repeated `_id` and answers with the stored message
authored by MOCK_USERNAME. The first frame is an empty sidebar (D-05), so `wait_ready` waits until the rooms have arrived and
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
MOCK_USERNAME = "smoke"
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


def make_config_dir(files, modes=None):
    """Create a temp XDG_CONFIG_HOME holding `rockettui/<name>` for each `name -> text` of `files`.

    `modes` maps a file name to the permission bits to chmod it to after writing (the default is
    the umask's). Removed at exit. Returns the base directory (the value for XDG_CONFIG_HOME).
    """
    base = tempfile.mkdtemp(prefix="rockettui-smoke-")
    atexit.register(shutil.rmtree, base, ignore_errors=True)
    config = os.path.join(base, "rockettui")
    os.mkdir(config)
    for name, text in files.items():
        with open(os.path.join(config, name), "w", encoding="utf-8") as handle:
            handle.write(text)
    for name, mode in (modes or {}).items():
        os.chmod(os.path.join(config, name), mode)
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
HISTORY_BASE = datetime.datetime(2026, 11, 16, 8, 55, 0, tzinfo=datetime.timezone.utc)
E2E_SENTINEL = "U0VOVElORUwtQ0lQSEVS"  # fake ciphertext of an E2E message; must never be drawn


def _subscriptions():
    with open(os.path.join(FIXTURES, "subscriptions.json"), encoding="utf-8") as handle:
        return {sub["rid"]: sub for sub in json.load(handle)["update"]}


def _iso(base, index):
    when = base + datetime.timedelta(seconds=30 * index)
    return when.strftime("%Y-%m-%dT%H:%M:%S.") + f"{when.microsecond // 1000:03d}Z"


def history_messages(rid, name):
    """All messages of a room, oldest first. r-general has 130 (`message 000`..`message 129`,
    authors alternating alice and bob, except the last four: g-126 is an E2E leftover, g-127 a
    `uj` system message by carol, g-128 carries an attachment, g-129 stays `message 129`), every
    other room 5 (`<name> note N`)."""
    base = HISTORY_BASE
    out = []
    count = 130 if rid == "r-general" else 5
    for i in range(count):
        if rid == "r-general":
            ident, text = f"g-{i:03d}", f"message {i:03d}"
        else:
            ident, text = f"{rid}-{i}", f"{name} note {i}"
        user = "alice" if i % 2 == 0 else "bob"
        message = {
            "_id": ident,
            "rid": rid,
            "msg": text,
            "ts": _iso(base, i),
            "u": {"_id": f"u-{user}", "username": user},
        }
        if rid == "r-general" and i == 126:
            message.update(t="e2e", msg=E2E_SENTINEL, content={"ciphertext": E2E_SENTINEL})
        elif rid == "r-general" and i == 127:
            message.update(
                t="uj", msg="carol", u={"_id": "u-carol", "username": "carol"}
            )
        elif rid == "r-general" and i == 128:
            message.update(msg="see attached", attachments=[{"title": "report.pdf"}])
        out.append(message)
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


SEND_ENDPOINT = "/api/v1/chat.sendMessage"
GET_MESSAGE_ENDPOINT = "/api/v1/chat.getMessage"


def _now_iso():
    now = datetime.datetime.now(datetime.timezone.utc)
    return now.strftime("%Y-%m-%dT%H:%M:%S.") + f"{now.microsecond // 1000:03d}Z"


def send_reply(server, body):
    """(status, body) for one chat.sendMessage: 400 invalid-params unless the body is exactly
    {"message": {"_id", "rid", "msg"}}, 400 duplicate for an `_id` already stored, else the stored
    message authored by MOCK_USERNAME."""
    invalid = (400, {"success": False, "error": "invalid-params"})
    failure = server.fail_next.pop(0) if server.fail_next else None
    if isinstance(failure, int):
        # A scripted failure: an integer status answers that status and stores nothing.
        return failure, {"success": False, "error": "error-action-not-allowed"}
    if not isinstance(body, dict) or list(body.keys()) != ["message"]:
        return invalid
    message = body["message"]
    if not isinstance(message, dict) or set(message.keys()) != {"_id", "rid", "msg"}:
        return invalid
    if not all(isinstance(message[key], str) for key in ("_id", "rid", "msg")):
        return invalid
    if message["_id"] in server.stored:
        return 400, {"success": False, "error": "duplicate"}
    now = _now_iso()
    stored = {
        "_id": message["_id"],
        "rid": message["rid"],
        "msg": message["msg"],
        "ts": now,
        "_updatedAt": now,
        "u": {"_id": MOCK_USER_ID, "username": MOCK_USERNAME},
    }
    server.stored[message["_id"]] = stored
    if failure == "store_then_500":
        # The server keeps the message, then fails the answer: the outcome is unknown to the client.
        return 500, {"success": False}
    return 200, {"message": stored, "success": True}


def get_message_reply(server, query):
    """(status, body) for one chat.getMessage: the stored message for a known `msgId`, else 400."""
    stored = server.stored.get(query.get("msgId"))
    if stored is None:
        return 400, {"success": False, "error": "error-not-found"}
    return 200, {"message": stored, "success": True}


def start_mock(mode="ok"):
    """Start a mock Rocket.Chat on 127.0.0.1 with an ephemeral port, in a daemon thread.

    Returns the server: `base_url` is `http://127.0.0.1:<port>` and `requests` is a list of
    `(method, path, query_dict, authed_bool)`, one per request received (any method). GET and
    POST chat.sendMessage are served; every other method gets 405 (and is recorded). A request
    without the right X-User-Id and X-Auth-Token gets 401, whatever the method. The server is shut
    down at exit.

    `bodies` is a list of `(path, parsed_json)`, one per POST that carried a JSON body, and
    `stored` maps the `_id` of each accepted chat.sendMessage to the message the server stored.
    `fail_next` is a list a scenario may fill: each chat.sendMessage pops its first entry. An
    integer entry answers that status with {"success": false} without storing the message;
    "store_then_500" stores the message and then answers 500. GET chat.getMessage answers 200 with
    the stored message for a known `msgId` and 400 otherwise.

    `mode` is settable at any time: "ok" serves normally, "unauthorized" answers every request with
    the 9.0.0-style 401 body whatever the credentials (a rejected or revoked token).
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
            body = None
            if self.command == "POST":
                length = int(self.headers.get("Content-Length") or 0)
                raw = self.rfile.read(length) if length else b""
                try:
                    body = json.loads(raw) if raw else None
                except ValueError:
                    body = None
                if body is not None:
                    self.server.bodies.append((parsed.path, body))
            if self.server.mode == "unauthorized" or not authed:
                self.reply(
                    401, {"success": False, "error": "You must be logged in to do this."}
                )
            elif self.command == "POST" and parsed.path == SEND_ENDPOINT:
                self.reply(*send_reply(self.server, body))
            elif self.command != "GET":
                self.reply(405, {"success": False})
            elif parsed.path == GET_MESSAGE_ENDPOINT:
                self.reply(*get_message_reply(self.server, query))
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
    server.bodies = []
    server.stored = {}
    server.fail_next = []
    server.mode = mode
    server.base_url = f"http://127.0.0.1:{server.server_address[1]}"
    threading.Thread(target=server.serve_forever, daemon=True).start()
    atexit.register(server.shutdown)
    return server


def spawn(binary, extra_env=None, config_files=None, server_url=None, config_modes=None):
    """Fork the binary onto a fresh 80x24 pty. Returns (pid, master_fd).

    XDG_CONFIG_HOME always points at a fresh temp dir (empty unless `config_files` is given), so
    the developer's real config never changes a scenario. The three ROCKETTUI_* credential
    variables are removed first, so the developer's real credentials never reach a scenario; with
    `server_url` they are then set to that URL and the mock's user id and token. `config_modes`
    chmods config files (see `make_config_dir`).
    """
    config_home = make_config_dir(config_files or {}, config_modes)
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


def wait_ready(fd, buf, room=b"general", timeout=10):
    """The shared readiness check: `room` is listed (it arrived from the server) and NORMAL is drawn.

    D-05 draws an empty sidebar first, so there is no ordering to check.
    """
    text = room.decode()
    ready = lambda b: screen_of(b).contains(text) and screen_of(b).contains("NORMAL")
    if not read_until(fd, buf, ready, timeout):
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


def scenario_send(binary):
    """MSG-02 in the real binary: Insert-mode Enter shows the draft at once, POSTs it exactly once
    as {"message": {"_id", "rid", "msg"}} with a client _id of the run's `rt` prefix, and the same
    row turns into the server's copy (author `smoke`)."""
    mock = start_mock()
    pid, fd = spawn(binary, server_url=mock.base_url)
    buf = bytearray()
    wait_ready(fd, buf)
    type_keys(fd, buf, b"\r")  # opens general
    type_keys(fd, buf, b"i")
    text = 'hello "send"'
    type_keys(fd, buf, text.encode())
    type_keys(fd, buf, b"\r")

    swapped = lambda b: any(
        text in row and MOCK_USERNAME in row for row in screen_of(b).rows()
    )
    if not read_until(fd, buf, swapped, 10):
        raise SmokeFail("the sent message never turned into the server's copy")
    posts = [r for r in mock.requests if r[0] == "POST" and r[1] == SEND_ENDPOINT]
    if len(posts) != 1:
        raise SmokeFail(f"expected one chat.sendMessage POST, saw {len(posts)}")
    if not posts[0][3]:
        raise SmokeFail("the send request was not authenticated")
    sent = [body for path, body in mock.bodies if path == SEND_ENDPOINT]
    if len(sent) != 1 or list(sent[0].keys()) != ["message"]:
        raise SmokeFail("the send body is not exactly {message: ...}")
    message = sent[0]["message"]
    if set(message.keys()) != {"_id", "rid", "msg"}:
        raise SmokeFail(f"send body keys {sorted(message.keys())}, expected _id, rid, msg")
    if not message["_id"].startswith("rt") or len(message["_id"]) < 35:
        raise SmokeFail("the client _id does not carry the run's rt prefix")
    if message["rid"] != "r-general" or message["msg"] != text:
        raise SmokeFail("the send body names the wrong room or text")
    # A lone ESC followed by a pause, so crossterm does not read ESC plus the next byte as Alt+key.
    os.write(fd, b"\x1b")
    time.sleep(0.3)
    type_keys(fd, buf, b":q")
    type_keys(fd, buf, b"\r")
    code = wait_exit(pid, fd, buf, 10)
    if code != 0:
        raise SmokeFail(f"exit code {code}, expected 0")
    if MOCK_TOKEN.encode() in bytes(buf):
        raise SmokeFail("the token appeared in the output")


def scenario_send_retry(binary):
    """MSG-02, D-06 in the real binary: a failed send is retried with `r` under the same client _id
    and turns into the server's copy; a failed one is discarded with `dd`. Three POSTs in all."""
    mock = start_mock()
    mock.fail_next = [400]
    pid, fd = spawn(binary, server_url=mock.base_url)
    buf = bytearray()
    wait_ready(fd, buf)
    type_keys(fd, buf, b"\r")  # opens general
    type_keys(fd, buf, b"i")
    type_keys(fd, buf, b"retry me")
    type_keys(fd, buf, b"\r")
    # The status line is read before ESC: any key press clears it.
    if not read_until(fd, buf, lambda b: screen_of(b).contains("message not sent"), 10):
        raise SmokeFail("a rejected send never showed `message not sent`")
    # A lone ESC followed by a pause, so crossterm does not read ESC plus the next byte as Alt+key.
    os.write(fd, b"\x1b")
    time.sleep(0.3)
    type_keys(fd, buf, b"r")

    def posts_of():
        return [body for path, body in mock.bodies if path == SEND_ENDPOINT]

    def retried(b):
        rows = screen_of(b).rows()
        return len(posts_of()) == 2 and any(
            "retry me" in row and MOCK_USERNAME in row for row in rows
        )

    if not read_until(fd, buf, retried, 10):
        raise SmokeFail("`r` never re-sent the failed message")
    first, second = posts_of()
    if first["message"]["_id"] != second["message"]["_id"]:
        raise SmokeFail("the retry used another client _id")

    mock.fail_next = [400]
    type_keys(fd, buf, b"i")
    type_keys(fd, buf, b"drop me")
    type_keys(fd, buf, b"\r")
    if not read_until(fd, buf, lambda b: screen_of(b).contains("message not sent"), 10):
        raise SmokeFail("the second rejected send never showed `message not sent`")
    os.write(fd, b"\x1b")
    time.sleep(0.3)
    if not any("drop me" in row for row in screen_of(buf).rows()):
        raise SmokeFail("the failed message is not on the screen before dd")
    type_keys(fd, buf, b"dd")
    gone = lambda b: not any("drop me" in row for row in screen_of(b).rows())
    if not read_until(fd, buf, gone, 5):
        raise SmokeFail("`dd` did not remove the failed message")
    posts = [r for r in mock.requests if r[0] == "POST" and r[1] == SEND_ENDPOINT]
    if len(posts) != 3:
        raise SmokeFail(f"expected three chat.sendMessage POSTs, saw {len(posts)}")
    type_keys(fd, buf, b":q")
    type_keys(fd, buf, b"\r")
    code = wait_exit(pid, fd, buf, 10)
    if code != 0:
        raise SmokeFail(f"exit code {code}, expected 0")
    if MOCK_TOKEN.encode() in bytes(buf):
        raise SmokeFail("the token appeared in the output")


def scenario_send_unconfirmed(binary):
    """D-07, D-15, D-18 in the real binary: the server stores a message and then answers 500, so the
    row is Unconfirmed; `r` asks chat.getMessage for that _id, finds it and turns the row into the
    server's copy without a second POST."""
    mock = start_mock()
    mock.fail_next = ["store_then_500"]
    pid, fd = spawn(binary, server_url=mock.base_url)
    buf = bytearray()
    wait_ready(fd, buf)
    type_keys(fd, buf, b"\r")  # opens general
    type_keys(fd, buf, b"i")
    type_keys(fd, buf, b"maybe sent")
    type_keys(fd, buf, b"\r")
    # The status line is read before ESC: any key press clears it.
    if not read_until(fd, buf, lambda b: screen_of(b).contains("may not have been sent"), 10):
        raise SmokeFail("a 500 answer never showed `may not have been sent`")
    # A lone ESC followed by a pause, so crossterm does not read ESC plus the next byte as Alt+key.
    os.write(fd, b"\x1b")
    time.sleep(0.3)
    type_keys(fd, buf, b"r")
    swapped = lambda b: any(
        "maybe sent" in row and MOCK_USERNAME in row for row in screen_of(b).rows()
    )
    if not read_until(fd, buf, swapped, 10):
        raise SmokeFail("`r` never turned the unconfirmed row into the server's copy")
    posts = [r for r in mock.requests if r[0] == "POST" and r[1] == SEND_ENDPOINT]
    if len(posts) != 1:
        raise SmokeFail(f"expected exactly one chat.sendMessage POST, saw {len(posts)}")
    sent_id = [body for path, body in mock.bodies if path == SEND_ENDPOINT][0]["message"]["_id"]
    gets = [r for r in mock.requests if r[0] == "GET" and r[1] == GET_MESSAGE_ENDPOINT]
    if len(gets) != 1:
        raise SmokeFail(f"expected exactly one chat.getMessage GET, saw {len(gets)}")
    if gets[0][2] != {"msgId": sent_id}:
        raise SmokeFail("the reconcile GET did not ask for the POST's _id")
    if not gets[0][3]:
        raise SmokeFail("the reconcile GET was not authenticated")
    type_keys(fd, buf, b":q")
    type_keys(fd, buf, b"\r")
    code = wait_exit(pid, fd, buf, 10)
    if code != 0:
        raise SmokeFail(f"exit code {code}, expected 0")
    if MOCK_TOKEN.encode() in bytes(buf):
        raise SmokeFail("the token appeared in the output")


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
    # D-15: system line, attachment tag and the E2E leftover read as text, never as ciphertext.
    kinds = ("* joined the room", "[attachment: report.pdf]", "[encrypted message]")
    if not read_until(fd, buf, lambda b: all(screen_of(b).contains(t) for t in kinds), 5):
        missing = [t for t in kinds if not screen_of(buf).contains(t)]
        raise SmokeFail(f"history kinds not drawn as text, missing: {missing}")
    if E2E_SENTINEL.encode() in bytes(buf):
        raise SmokeFail("E2E ciphertext appears in the terminal output")
    history_requests = [r for r in mock.requests if r[1] in HISTORY_ENDPOINTS]
    general = [r for r in history_requests if r[2].get("roomId") == "r-general"]
    if len(general) != 1 or general[0][1] != "/api/v1/channels.history":
        raise SmokeFail(f"expected one channels.history request for r-general: {general}")
    if "latest" in general[0][2]:
        raise SmokeFail("the first page was requested with `latest`")
    # gg moves onto the oldest loaded message (g-080): the next older page is requested with
    # `latest` equal to its timestamp, and after it arrives k lands on message 079 (D-14).
    type_keys(fd, buf, b"gg")
    oldest = _iso(HISTORY_BASE, 80)

    def older_request(_b):
        return any(
            r[1] == "/api/v1/channels.history"
            and r[2].get("roomId") == "r-general"
            and r[2].get("latest") == oldest
            for r in mock.requests
        )

    if not read_until(fd, buf, older_request, 5):
        raise SmokeFail(f"no older-page request with latest={oldest}: {mock.requests}")
    # k does nothing until the page is in (the cursor sits on the oldest message), so press it
    # until message 079 is drawn.
    reached = False
    for _ in range(25):
        type_keys(fd, buf, b"k")
        if read_until(fd, buf, lambda b: screen_of(b).contains("message 079"), 0.2):
            reached = True
            break
    if not reached:
        raise SmokeFail("message 079 never appeared after gg and k")
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
    if E2E_SENTINEL.encode() in bytes(buf):
        raise SmokeFail("E2E ciphertext appears in the terminal output")


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


def scenario_config_creds(binary):
    # The credentials live in config.toml only (mode 0600): no ROCKETTUI_* variable is set.
    mock = start_mock()
    files = {
        "config.toml": (
            f'server_url = "{mock.base_url}"\n'
            f'user_id = "{MOCK_USER_ID}"\n'
            f'token = "{MOCK_TOKEN}"\n'
        )
    }
    pid, fd = spawn(
        binary, config_files=files, config_modes={"config.toml": 0o600}
    )
    buf = bytearray()
    wait_ready(fd, buf)
    if not mock.requests:
        raise SmokeFail("the mock received no request")
    if not all(authed for _method, _path, _query, authed in mock.requests):
        raise SmokeFail("a request reached the server without valid credentials")
    if MOCK_TOKEN.encode() in bytes(buf):
        raise SmokeFail("the token appears in the terminal output")
    type_keys(fd, buf, b":q")
    type_keys(fd, buf, b"\r")
    code = wait_exit(pid, fd, buf, 10)
    if code != 0:
        raise SmokeFail(f"exit code {code}, expected 0")
    if MOCK_TOKEN.encode() in bytes(buf):
        raise SmokeFail("the token appears in the terminal output")


def scenario_perm_warning(binary):
    mock = start_mock()
    files = {
        "config.toml": (
            f'server_url = "{mock.base_url}"\n'
            f'user_id = "{MOCK_USER_ID}"\n'
            f'token = "{MOCK_TOKEN}"\n'
        )
    }
    # A 0644 config.toml holding the token: the app still starts, warns on the status row (kept
    # across a key press) and repeats the warning on stderr after the terminal is restored.
    pid, fd = spawn(binary, config_files=files, config_modes={"config.toml": 0o644})
    buf = bytearray()
    wait_ready(fd, buf)
    if not read_until(fd, buf, lambda b: screen_of(b).contains("chmod 600"), 5):
        raise SmokeFail("the permission warning never appeared on the status bar")
    type_keys(fd, buf, b"j")
    read_until(fd, buf, lambda b: False, 0.3)
    if not screen_of(buf).contains("chmod 600"):
        raise SmokeFail("the warning did not survive a key press")
    type_keys(fd, buf, b":q")
    type_keys(fd, buf, b"\r")
    code = wait_exit(pid, fd, buf, 10)
    if code != 0:
        raise SmokeFail(f"exit code {code}, expected 0")
    data = bytes(buf)
    after = data[data.rfind(ALT_OFF) :]
    if b"rockettui: warning:" not in after or b"chmod 600" not in after:
        raise SmokeFail("the warning was not printed to stderr after the terminal was restored")
    if MOCK_TOKEN.encode() in data:
        raise SmokeFail("the token appears in the terminal output")
    if not all(authed for _method, _path, _query, authed in mock.requests):
        raise SmokeFail("a request reached the server without valid credentials")

    # The same files at 0600: no warning anywhere.
    pid, fd = spawn(binary, config_files=files, config_modes={"config.toml": 0o600})
    buf = bytearray()
    wait_ready(fd, buf)
    type_keys(fd, buf, b":q")
    type_keys(fd, buf, b"\r")
    code = wait_exit(pid, fd, buf, 10)
    if code != 0:
        raise SmokeFail(f"exit code {code}, expected 0")
    if b"chmod 600" in bytes(buf):
        raise SmokeFail("a 0600 config.toml produced a permission warning")
    if MOCK_TOKEN.encode() in bytes(buf):
        raise SmokeFail("the token appears in the terminal output")


def scenario_unauthorized(binary):
    # The server rejects the token: one clear status line, the app stays usable, nothing retries
    # on its own, and :reconnect loads the rooms once the server accepts again (D-06).
    mock = start_mock("unauthorized")
    pid, fd = spawn(binary, server_url=mock.base_url)
    buf = bytearray()
    wait_drawn(fd, buf)
    shows_unauthorized = lambda b: screen_of(b).contains("unauthorized")
    if not read_until(fd, buf, shows_unauthorized, 10):
        raise SmokeFail("the unauthorized line never appeared on the status bar")
    if screen_of(buf).contains("general"):
        raise SmokeFail("a room is listed although the server rejected the token")
    # Give an automatic retry time to show itself, then count the room-list requests.
    read_until(fd, buf, lambda b: False, 1.0)
    subs = lambda: [r for r in mock.requests if r[1] == "/api/v1/subscriptions.get"]
    if len(subs()) != 1:
        raise SmokeFail(f"expected exactly one subscriptions.get before :reconnect: {mock.requests}")
    # The app stays usable: a key press is handled and the line is still the unauthorized one.
    type_keys(fd, buf, b"j")
    if not screen_of(buf).contains("NORMAL"):
        raise SmokeFail("the app stopped drawing after the failure")

    mock.mode = "ok"
    type_keys(fd, buf, b":reconnect")
    type_keys(fd, buf, b"\r")
    if not read_until(fd, buf, lambda b: screen_of(b).contains("general"), 10):
        raise SmokeFail("the rooms never loaded after :reconnect")
    status = lambda b: screen_of(b).rows()[22]
    if not read_until(fd, buf, lambda b: "unauthorized" not in status(b), 5):
        raise SmokeFail("the unauthorized line is still on the status bar after :reconnect")
    if len(subs()) < 2:
        raise SmokeFail(f"expected a second subscriptions.get after :reconnect: {mock.requests}")
    if any(method != "GET" for method, _path, _query, _authed in mock.requests):
        raise SmokeFail(f"a non-GET request was sent: {mock.requests}")
    if MOCK_TOKEN.encode() in bytes(buf):
        raise SmokeFail("the token appears in the terminal output")
    type_keys(fd, buf, b":q")
    type_keys(fd, buf, b"\r")
    code = wait_exit(pid, fd, buf, 10)
    if code != 0:
        raise SmokeFail(f"exit code {code}, expected 0")
    if MOCK_TOKEN.encode() in bytes(buf):
        raise SmokeFail("the token appears in the terminal output")


def docker_env(project):
    """The ROCKETTUI_* values `dev/seed.py` wrote for `project`, read in-process.

    The values go only into the child environment: nothing here prints them (T-03-29). A server
    URL that is not loopback is refused, so the scenario can never reach a real server (T-03-30).
    """
    path = os.path.join(REPO, "dev", "state", project, ".env.local")
    try:
        with open(path, encoding="utf-8") as handle:
            lines = handle.read().splitlines()
    except OSError:
        raise SmokeFail("docker stack not seeded: run dev/seed.py up and seed") from None
    values = {}
    for line in lines:
        key, sep, value = line.partition("=")
        if sep and key.strip() in CREDENTIAL_VARS:
            values[key.strip()] = value.strip().strip("\"'")
    if sorted(values) != sorted(CREDENTIAL_VARS) or not all(values.values()):
        raise SmokeFail("docker stack not seeded: run dev/seed.py up and seed")
    host = urllib.parse.urlsplit(values["ROCKETTUI_SERVER_URL"]).hostname
    if host not in ("127.0.0.1", "localhost"):
        raise SmokeFail("the docker state file does not point at a loopback server")
    return values


SIDEBAR_HEADERS = ("Unread", "Folders", "Recent")
ROOM_GLYPHS = "#&~+@"
FOLD_GLYPHS = "\u25be\u25b8"
SIDEBAR_CURSOR_WIDTH = 2  # the "> " highlight symbol column


def sidebar_rows(screen, width=28):
    """The sidebar text of screen rows 1..nrows-3: between the borders, cursor column dropped,
    trailing spaces removed."""
    rows = []
    for row in screen.rows()[1 : screen.nrows - 2]:
        inner = row[1 : width - 1][SIDEBAR_CURSOR_WIDTH:]
        rows.append(inner.rstrip())
    return rows


def room_name_of(row):
    """The room name drawn on a sidebar room row, or None when the row is not a room row: an
    optional `* ` unread marker, a kind glyph, the name and an optional ` [e2e]` tag."""
    text = row.strip()
    if text.startswith("* "):
        text = text[2:].lstrip()
    if not text or text[0] not in ROOM_GLYPHS:
        return None
    text = text[1:]
    if text.endswith(" [e2e]"):
        text = text[: -len(" [e2e]")]
    return text


def selectable_index(rows, name):
    """The selectable index (headers and blank rows skipped, folders and rooms counted) of the
    first room row named `name`; None when absent."""
    index = 0
    for row in rows:
        text = row.strip()
        if not text or text in SIDEBAR_HEADERS:
            continue
        if room_name_of(row) == name:
            return index
        index += 1
    return None


def open_room_named(fd, buf, name):
    """Focus the sidebar, walk to the room named `name` and open it with Enter."""
    type_keys(fd, buf, b"\x17")
    type_keys(fd, buf, b"h")
    type_keys(fd, buf, b"gg")
    read_until(fd, buf, lambda b: False, 0.3)
    index = selectable_index(sidebar_rows(screen_of(buf)), name)
    if index is None:
        raise SmokeFail(f"room {name!r} is not listed in the sidebar")
    for _ in range(index):
        type_keys(fd, buf, b"j")
    type_keys(fd, buf, b"\r")
    on_status = lambda b: name in screen_of(b).rows()[22]
    if not read_until(fd, buf, on_status, 10):
        raise SmokeFail(f"room {name!r} never opened")


def scenario_docker_read(binary):
    project = os.environ.get("RT_PROJECT", "rockettui-dev")
    env = docker_env(project)
    token = env["ROCKETTUI_TOKEN"].encode()
    pid, fd = spawn(binary, extra_env=env)
    buf = bytearray()
    wait_ready(fd, buf, room=b"rt-public", timeout=20)
    wanted = ["rt-public", "rt-private", "alice", "secrets", "rt-team", "rt-discussion", "Favorites"]
    with open(os.path.join(FIXTURES, "docker", "preferences.json"), encoding="utf-8") as handle:
        categories = json.load(handle).get("preferences", {}).get("sidebarCategories", [])
    wanted += [c["name"] for c in categories if c.get("name")]
    listed = lambda b: all(screen_of(b).contains(text) for text in wanted)
    if not read_until(fd, buf, listed, 20):
        missing = [t for t in wanted if not screen_of(buf).contains(t)]
        raise SmokeFail(f"rooms never listed, missing: {missing}")
    if selectable_index(sidebar_rows(screen_of(buf)), "bob") is not None:
        raise SmokeFail("the closed bob DM is listed")

    open_room_named(fd, buf, "rt-public")
    # The 50-column message pane wraps the attachment tag onto two rows, so check both halves.
    newest = ("seed message 129", "[attachment:", "report.pdf]")
    if not read_until(fd, buf, lambda b: all(screen_of(b).contains(t) for t in newest), 10):
        missing = [t for t in newest if not screen_of(buf).contains(t)]
        raise SmokeFail(f"the newest history page was not drawn, missing: {missing}")
    # gg lands on the oldest loaded message, which requests the next older page.
    for _ in range(6):
        type_keys(fd, buf, b"gg")
        if read_until(fd, buf, lambda b: screen_of(b).contains("* joined the room"), 1.5):
            break
    else:
        raise SmokeFail("the join system line never appeared while paging back")
    type_keys(fd, buf, b"gg")
    status = lambda b: screen_of(b).rows()[22]
    if not read_until(fd, buf, lambda b: "Beginning of history" in status(b), 10):
        raise SmokeFail("the start of history was never reported")

    open_room_named(fd, buf, "secrets")
    if not read_until(fd, buf, lambda b: screen_of(b).contains("encrypted, not supported"), 10):
        raise SmokeFail("the encrypted placeholder was never shown")

    open_room_named(fd, buf, "alice")

    def alice_drawn(b):
        screen = screen_of(b)
        return not screen.contains("Loading") and not screen.contains("No messages yet")

    if not read_until(fd, buf, alice_drawn, 10):
        raise SmokeFail("the alice DM never showed messages")
    type_keys(fd, buf, b":q")
    type_keys(fd, buf, b"\r")
    code = wait_exit(pid, fd, buf, 10)
    if code != 0:
        raise SmokeFail(f"exit code {code}, expected 0")
    if token in bytes(buf):
        raise SmokeFail("the token appears in the terminal output")


SCENARIOS = {
    "quit": scenario_quit,
    "panic": scenario_panic,
    "insert": scenario_insert,
    "send": scenario_send,
    "send_retry": scenario_send_retry,
    "send_unconfirmed": scenario_send_unconfirmed,
    "config_layout": scenario_config_layout,
    "config_keymap": scenario_config_keymap,
    "config_error": scenario_config_error,
    "connect_read": scenario_connect_read,
    "missing_creds": scenario_missing_creds,
    "config_creds": scenario_config_creds,
    "perm_warning": scenario_perm_warning,
    "unauthorized": scenario_unauthorized,
    "docker_read": scenario_docker_read,
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
