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
import secrets
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
ME_ENDPOINT = "/api/v1/me"


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
    `(method, path, query_dict, authed_bool)`, one per request received (any method) except the
    WebSocket upgrade of the realtime task (`GET /websocket`), which is answered 404 and only
    counted in `ws_attempts`. GET and
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
            if parsed.path == "/websocket":
                # The realtime connection (realtime-ddp): its upgrade request carries no
                # credential header by design (the token travels in the DDP login frame), and
                # this mock has no WebSocket, so it is refused and kept out of `requests`.
                self.server.ws_attempts += 1
                self.reply(404, {"success": False})
                return
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
            elif parsed.path == ME_ENDPOINT:
                self.reply(
                    200, {"_id": MOCK_USER_ID, "username": MOCK_USERNAME, "success": True}
                )
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
    server.ws_attempts = 0
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


def wait_confirmed(fd, buf, text, timeout=10, two_line=False):
    """Wait until the server's copy of the message `text` is drawn.

    Pending and failed rows show a marker where the time would be (plan 04-04), and since plan
    04-06 the pending author already equals the confirmed one, so only the time proves the swap:
    some screen row i holds `text` and row i (Compact) or row i-1 (TwoLine header) matches
    `\\d\\d:\\d\\d`. Every sending scenario calls this before its final `:q`, which warns
    while a message is undelivered (D-20).
    """
    clock = re.compile(r"\d\d:\d\d")

    def confirmed(b):
        rows = screen_of(b).rows()
        for i, row in enumerate(rows):
            if text not in row:
                continue
            head = rows[i - 1] if two_line and i > 0 else row
            if clock.search(head):
                return True
        return False

    if not read_until(fd, buf, confirmed, timeout):
        raise SmokeFail(f"the server's copy of {text!r} never showed its time")


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
    wait_confirmed(fd, buf, "hello smoke")
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
    wait_confirmed(fd, buf, text)
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
    wait_confirmed(fd, buf, "retry me")
    type_keys(fd, buf, b":q")
    type_keys(fd, buf, b"\r")
    code = wait_exit(pid, fd, buf, 10)
    if code != 0:
        raise SmokeFail(f"exit code {code}, expected 0")
    if MOCK_TOKEN.encode() in bytes(buf):
        raise SmokeFail("the token appeared in the output")


def scenario_send_author(binary):
    """D-17 in the real binary: the own username is learned once with GET /api/v1/me at startup, so
    a message the server never echoed (a Failed row) is still drawn under `smoke`, not under the
    `me` fallback. The /me requests are counted as a total since launch."""
    mock = start_mock()
    mock.fail_next = [400]
    pid, fd = spawn(binary, server_url=mock.base_url)
    buf = bytearray()
    wait_ready(fd, buf)
    type_keys(fd, buf, b"\r")  # opens general
    type_keys(fd, buf, b"i")
    type_keys(fd, buf, b"who am i")
    type_keys(fd, buf, b"\r")
    # The status line is read before ESC: any key press clears it.
    if not read_until(fd, buf, lambda b: screen_of(b).contains("message not sent"), 10):
        raise SmokeFail("a rejected send never showed `message not sent`")
    # A lone ESC followed by a pause, so crossterm does not read ESC plus the next byte as Alt+key.
    os.write(fd, b"\x1b")
    time.sleep(0.3)
    # The row was never confirmed by the server, so its author can only have come from /me.
    if not any("who am i" in row and MOCK_USERNAME in row for row in screen_of(buf).rows()):
        raise SmokeFail("the pending row is not drawn under the username from /me")

    def me_gets():
        return [r for r in mock.requests if r[0] == "GET" and r[1] == ME_ENDPOINT]

    if len(me_gets()) != 1:
        raise SmokeFail(f"expected exactly one GET /api/v1/me before :reconnect: {mock.requests}")
    if not me_gets()[0][3]:
        raise SmokeFail("the /me request was not authenticated")
    # :reconnect re-learns the username (the total becomes two) and never touches a send.
    type_keys(fd, buf, b":reconnect")
    type_keys(fd, buf, b"\r")
    if not read_until(fd, buf, lambda b: len(me_gets()) == 2, 10):
        raise SmokeFail(f"expected a second GET /api/v1/me after :reconnect: {mock.requests}")
    read_until(fd, buf, lambda b: False, 0.5)  # a stray /me or POST would be sent by now
    if len(me_gets()) != 2:
        raise SmokeFail(f"expected exactly two GET /api/v1/me in all: {mock.requests}")
    posts = [r for r in mock.requests if r[0] == "POST" and r[1] == SEND_ENDPOINT]
    if len(posts) != 1:
        raise SmokeFail(f"expected one chat.sendMessage POST in all, saw {len(posts)}")
    if not any("who am i" in row and MOCK_USERNAME in row for row in screen_of(buf).rows()):
        raise SmokeFail("the failed row lost its author or its place after :reconnect")
    # The Failed `who am i` row is intentional and D-20 makes a plain `:q` warn about it: `:q!`.
    type_keys(fd, buf, b":q!")
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
    # Since plan 04-06 the pending row already carries the username from /me, so the author cell can
    # no longer tell the local row from the server's copy. Wait on the observable events instead: the
    # reconcile GET reached the server, and the row has lost its unconfirmed `?` marker.
    def swapped(b):
        asked = any(r[0] == "GET" and r[1] == GET_MESSAGE_ENDPOINT for r in mock.requests)
        rows = [row for row in screen_of(b).rows() if "maybe sent" in row]
        return asked and bool(rows) and all("?" not in row for row in rows)

    if not read_until(fd, buf, swapped, 10):
        raise SmokeFail("`r` never turned the unconfirmed row into the server's copy")
    wait_confirmed(fd, buf, "maybe sent")
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


def failed_send_session(binary, text):
    """Start the binary against a mock that refuses the first send, send `text` in general and wait
    until the row is Failed. Returns (pid, fd, buf)."""
    mock = start_mock()
    mock.fail_next = [400]
    pid, fd = spawn(binary, server_url=mock.base_url)
    buf = bytearray()
    wait_ready(fd, buf)
    type_keys(fd, buf, b"\r")  # opens general
    type_keys(fd, buf, b"i")
    type_keys(fd, buf, text.encode())
    type_keys(fd, buf, b"\r")
    # The status line is read before ESC: any key press clears it.
    if not read_until(fd, buf, lambda b: screen_of(b).contains("message not sent"), 10):
        raise SmokeFail("a rejected send never showed `message not sent`")
    # A lone ESC followed by a pause, so crossterm does not read ESC plus the next byte as Alt+key.
    os.write(fd, b"\x1b")
    time.sleep(0.3)
    return pid, fd, buf


def scenario_quit_undelivered(binary):
    """D-20 in the real binary: `:q` with a Failed message warns with a count only and keeps
    running, a second `:q` quits; in a second run `:q!` quits at once."""
    text = "keep me"
    pid, fd, buf = failed_send_session(binary, text)
    type_keys(fd, buf, b":q")
    type_keys(fd, buf, b"\r")
    status = lambda b: screen_of(b).rows()[22]
    if not read_until(fd, buf, lambda b: "not delivered" in status(b), 10):
        raise SmokeFail("`:q` with a Failed message never showed `not delivered`")
    if "1 message not delivered" not in status(buf):
        raise SmokeFail(f"unexpected warning text: {status(buf).strip()!r}")
    read_until(fd, buf, lambda b: False, 1.0)
    done, _ = os.waitpid(pid, os.WNOHANG)
    if done:
        raise SmokeFail("the app quit on the first `:q` although a message is undelivered")
    if text in status(buf):
        raise SmokeFail("the quit warning holds the message text")
    type_keys(fd, buf, b":q")
    type_keys(fd, buf, b"\r")
    code = wait_exit(pid, fd, buf, 10)
    if code != 0:
        raise SmokeFail(f"exit code {code}, expected 0")
    if MOCK_TOKEN.encode() in bytes(buf):
        raise SmokeFail("the token appeared in the output")

    pid, fd, buf = failed_send_session(binary, "keep me too")
    type_keys(fd, buf, b":q!")
    type_keys(fd, buf, b"\r")
    code = wait_exit(pid, fd, buf, 10)
    if code != 0:
        raise SmokeFail(f"exit code {code}, expected 0 after :q!")
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
    wait_confirmed(fd, buf, "layout check", two_line=True)
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


def history_settled(b):
    """The open room's history has neither a Loading placeholder nor a load failure on screen."""
    screen = screen_of(b)
    return not screen.contains("Loading") and not screen.contains("Could not load")


def scenario_docker_search(binary):
    """`:search` against a real local Rocket.Chat: rt-public holds 130 seeded `seed message N`
    messages, so one search spans three server pages. The first page is the newest (129), `G`
    loads the next pages until the oldest result shows, `q` closes the list, a text with no hit
    says so on the status row, and an encrypted room refuses before any request.
    """
    project = os.environ.get("RT_PROJECT", "rockettui-dev")
    env = docker_env(project)
    token = env["ROCKETTUI_TOKEN"].encode()
    pid, fd = spawn(binary, extra_env=env)
    buf = bytearray()
    wait_ready(fd, buf, room=b"rt-public", timeout=20)
    open_room_named(fd, buf, "rt-public")
    if not read_until(fd, buf, history_settled, 10):
        raise SmokeFail("rt-public history never settled")

    type_keys(fd, buf, b":search seed message\r")
    if not read_until(fd, buf, lambda b: any("Search: seed message" in r for r in screen_of(b).rows()), 10):
        raise SmokeFail("the results list never opened")
    if not screen_of(buf).contains("seed message 129"):
        raise SmokeFail("the newest result (seed message 129) is not at the top")
    if "NORMAL" not in screen_of(buf).rows()[22]:
        raise SmokeFail("the mode badge changed while the list is open")

    oldest = re.compile(r"seed message 0[0-1][0-9]\b")
    for _ in range(6):
        type_keys(fd, buf, b"G")
        if read_until(fd, buf, lambda b: any(oldest.search(r) for r in screen_of(b).rows()), 3):
            break
    else:
        raise SmokeFail("paging never reached the oldest results: " + " / ".join(r.strip() for r in screen_of(buf).rows()[:6]) + " ... " + screen_of(buf).rows()[22].strip())

    type_keys(fd, buf, b"q")
    if not read_until(fd, buf, lambda b: not any("Search:" in r for r in screen_of(b).rows()), 5):
        raise SmokeFail("q did not close the list")

    def open_list_at_oldest():
        """:search, then G until the oldest result is shown and selected."""
        type_keys(fd, buf, b":search seed message\r")
        if not read_until(fd, buf, lambda b: any("Search: seed message" in r for r in screen_of(b).rows()), 10):
            raise SmokeFail("the results list did not open")
        for _ in range(6):
            type_keys(fd, buf, b"G")
            if read_until(fd, buf, lambda b: any(oldest.search(r) for r in screen_of(b).rows()), 3):
                return
        raise SmokeFail("paging never reached the oldest results")

    def jump_from_oldest():
        """Enter twice: the context view, then the room (detached)."""
        type_keys(fd, buf, b"\r")
        if not read_until(fd, buf, lambda b: any("Context" in r for r in screen_of(b).rows()), 10):
            raise SmokeFail("Enter on a result did not open its context view")
        type_keys(fd, buf, b"\r")
        if not read_until(fd, buf, lambda b: "older messages" in screen_of(b).rows()[22], 10):
            raise SmokeFail("Enter in the context view did not jump into the room")
        if any("Search:" in r or "Context" in r for r in screen_of(buf).rows()):
            raise SmokeFail("the list or the view is still open after the jump")

    # Opening a result: Enter shows the history around it, q returns to the list, Enter twice
    # jumps into the room, which is then detached; :reload returns to the newest.
    open_list_at_oldest()
    type_keys(fd, buf, b"\r")
    if not read_until(fd, buf, lambda b: any("Context" in r for r in screen_of(b).rows()), 10):
        raise SmokeFail("Enter on a result did not open its context view")
    if sum("seed message" in r for r in screen_of(buf).rows()) < 5:
        raise SmokeFail("the context view does not show the messages around the result")
    type_keys(fd, buf, b"q")
    if not read_until(fd, buf, lambda b: not any("Context" in r for r in screen_of(b).rows()) and any("Search: seed message" in r for r in screen_of(b).rows()), 5):
        raise SmokeFail("q in the context view did not return to the list")
    jump_from_oldest()
    type_keys(fd, buf, b":reload\r")
    if not read_until(fd, buf, lambda b: "older messages" not in screen_of(b).rows()[22], 10):
        raise SmokeFail(":reload did not leave the detached state")
    if not read_until(fd, buf, lambda b: screen_of(b).contains("seed message 129"), 10):
        raise SmokeFail("the room does not show its newest message after :reload")

    # Reading on from an old result: jump to the oldest result, move down onto the end of the
    # window and keep going; newer pages are appended until the newest message is reached.
    open_list_at_oldest()
    jump_from_oldest()
    for _ in range(8):
        type_keys(fd, buf, b"300j")
        if read_until(fd, buf, lambda b: "Reached the newest" in screen_of(b).rows()[22], 4):
            break
    else:
        raise SmokeFail("reading on never reached the newest messages: " + screen_of(buf).rows()[22].strip())
    # The page was appended below the cursor; one more motion brings the newest message into view.
    type_keys(fd, buf, b"300j")
    if not read_until(fd, buf, lambda b: screen_of(b).contains("seed message 129"), 5):
        raise SmokeFail("the newest message is not shown after reading on")

    # `N` toward newer in a detached room fetches newer pages until the match is found: search
    # for the newest message from near the oldest result (nothing older matches), then N.
    open_list_at_oldest()
    jump_from_oldest()
    type_keys(fd, buf, b"/seed message 129\r")
    if not read_until(fd, buf, lambda b: "Not found" in screen_of(b).rows()[22] or "No older" in screen_of(b).rows()[22], 15):
        raise SmokeFail("the search toward older did not settle: " + screen_of(buf).rows()[22].strip())
    for _ in range(4):
        type_keys(fd, buf, b"N")
        if read_until(fd, buf, lambda b: screen_of(b).contains("seed message 129"), 4):
            break
    else:
        raise SmokeFail("N never found the newest message from a detached room: " + screen_of(buf).rows()[22].strip())

    type_keys(fd, buf, b":search zzzqqqxxxnomatch\r")
    if not read_until(fd, buf, lambda b: "No results" in screen_of(b).rows()[22], 10):
        raise SmokeFail("a search with no hit did not say so")
    if any("Search:" in r for r in screen_of(buf).rows()):
        raise SmokeFail("a list opened for a search with no hit")

    open_room_named(fd, buf, "secrets")
    type_keys(fd, buf, b":search anything\r")
    if not read_until(fd, buf, lambda b: "cannot be searched" in screen_of(b).rows()[22], 5):
        raise SmokeFail("the encrypted room was not refused")

    os.write(fd, b"\x1b")
    time.sleep(0.3)
    type_keys(fd, buf, b":q\r")
    code = wait_exit(pid, fd, buf, 10)
    if code != 0:
        raise SmokeFail(f"exit code {code}, expected 0")
    if token in bytes(buf):
        raise SmokeFail("the token appears in the terminal output")


def scenario_docker_send(binary):
    """MSG-02 against a real local Rocket.Chat (T-04-22, T-04-23): the binary opens the throwaway
    rt-private room, sends a fresh text from Insert mode and the row turns into the server's copy
    (its time and the `dev` author); a second run of the binary finds the same text in rt-private's
    history, so the message persisted on the server.

    Sends go only to rt-private, never to rt-public, so docker_read's newest-page checks stay valid
    however often this runs. `docker_env` reads the state file in-process, refuses a non-loopback
    server and never prints a value; the scenario fails if the token reaches the terminal output.
    """
    project = os.environ.get("RT_PROJECT", "rockettui-dev")
    env = docker_env(project)
    token = env["ROCKETTUI_TOKEN"].encode()
    text = "ds-" + secrets.token_hex(4)  # fresh per run, short enough for one row of the pane
    clock = re.compile(r"\d\d:\d\d")

    # First run: send the text and wait for the server's copy.
    pid, fd = spawn(binary, extra_env=env)
    buf = bytearray()
    wait_ready(fd, buf, room=b"rt-public", timeout=20)
    open_room_named(fd, buf, "rt-private")
    if not read_until(fd, buf, history_settled, 10):
        raise SmokeFail("rt-private history never settled (Loading or Could not load)")
    type_keys(fd, buf, b"i")
    type_keys(fd, buf, text.encode())
    type_keys(fd, buf, b"\r")

    def confirmed(b):
        # The confirmed row: pending and failed rows show a marker instead of the time.
        return any(text in row and "dev" in row and clock.search(row) for row in screen_of(b).rows())

    if not read_until(fd, buf, confirmed, 15):
        raise SmokeFail("the sent message never turned into a confirmed row with its time and `dev`")
    status = screen_of(buf).rows()[22]
    if "message not sent" in status or "may not have been sent" in status:
        raise SmokeFail("the status row reports a failed send")
    # A lone ESC followed by a pause, so crossterm does not read ESC plus the next byte as Alt+key.
    os.write(fd, b"\x1b")
    time.sleep(0.3)
    type_keys(fd, buf, b":q")
    type_keys(fd, buf, b"\r")
    code = wait_exit(pid, fd, buf, 10)
    if code != 0:
        raise SmokeFail(f"first run: exit code {code}, expected 0")
    if token in bytes(buf):
        raise SmokeFail("the token appears in the terminal output (first run)")

    # Second run: the text must come back from the server's history.
    pid, fd = spawn(binary, extra_env=env)
    buf = bytearray()
    wait_ready(fd, buf, room=b"rt-public", timeout=20)
    open_room_named(fd, buf, "rt-private")
    persisted = lambda b: any(text in row for row in screen_of(b).rows())
    if not read_until(fd, buf, persisted, 15):
        raise SmokeFail("the sent message is not in rt-private's history after a restart")
    type_keys(fd, buf, b":q")
    type_keys(fd, buf, b"\r")
    code = wait_exit(pid, fd, buf, 10)
    if code != 0:
        raise SmokeFail(f"second run: exit code {code}, expected 0")
    if token in bytes(buf):
        raise SmokeFail("the token appears in the terminal output (second run)")


# ---------------------------------------------------------------- realtime (realtime-ddp)

import socket
import urllib.error
import urllib.request


def rc_call(base, method, path, body=None, auth=None):
    """One REST call to the local Rocket.Chat. Returns the decoded JSON body, or None on failure.
    Only loopback servers are ever passed in (see `docker_env`)."""
    headers = {"Content-Type": "application/json"}
    if auth is not None:
        headers["X-User-Id"], headers["X-Auth-Token"] = auth
    data = None if body is None else json.dumps(body).encode()
    request = urllib.request.Request(base + path, data=data, method=method, headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            raw = response.read()
            return json.loads(raw) if raw else {}
    except (urllib.error.URLError, OSError, ValueError):
        return None


def read_env_file(path):
    values = {}
    try:
        with open(path, encoding="utf-8") as handle:
            for line in handle.read().splitlines():
                key, sep, value = line.partition("=")
                if sep:
                    values[key.strip()] = value.strip().strip("\"'")
    except OSError:
        raise SmokeFail("docker stack not seeded: run dev/seed.py up and seed") from None
    return values


class Seeded:
    """The seeded stack seen from the test harness: the dev PAT (the app's account) and a REST
    login of `alice`, the other person who posts. Secrets are kept in memory and never printed."""

    def __init__(self, project, base_override=None):
        self.env = docker_env(project)
        self.base = self.env["ROCKETTUI_SERVER_URL"].rstrip("/")
        self.dev = (self.env["ROCKETTUI_USER_ID"], self.env["ROCKETTUI_TOKEN"])
        admin = read_env_file(os.path.join(REPO, "dev", "state", project, ".env.admin.local"))
        reply = rc_call(self.base, "POST", "/api/v1/login", {"user": "alice", "password": admin.get("ALICE_PASS", "")})
        data = (reply or {}).get("data") or {}
        if "authToken" not in data:
            raise SmokeFail("could not log in as alice")
        self.alice = (data["userId"], data["authToken"])
        self.secrets = [self.dev[1], self.alice[1], admin.get("ALICE_PASS", "")]

    def admin_login(self, project):
        """Log in as the local admin (password read in-process, kept in memory only)."""
        admin = read_env_file(os.path.join(REPO, "dev", "state", project, ".env.admin.local"))
        reply = rc_call(self.base, "POST", "/api/v1/login", {"user": "admin", "password": admin.get("ADMIN_PASS", "")})
        data = (reply or {}).get("data") or {}
        if "authToken" not in data:
            raise SmokeFail("could not log in as admin")
        self.secrets.extend([data["authToken"], admin.get("ADMIN_PASS", "")])
        return (data["userId"], data["authToken"])

    def setting(self, admin, name, value=None):
        """Read a server setting (value None) or set it; returns the value the server holds."""
        path = f"/api/v1/settings/{name}"
        if value is None:
            return (rc_call(self.base, "GET", path, None, admin) or {}).get("value")
        reply = rc_call(self.base, "POST", path, {"value": value}, admin)
        if not (reply or {}).get("success"):
            raise SmokeFail(f"could not set {name}")
        return value

    def room_id(self, name, kind):
        """The id of the seeded room: `kind` is channels, groups or dm."""
        if kind == "dm":
            reply = rc_call(self.base, "POST", "/api/v1/im.create", {"username": "alice"}, self.dev)
            rid = ((reply or {}).get("room") or {}).get("_id")
        else:
            key = "channel" if kind == "channels" else "group"
            reply = rc_call(self.base, "GET", f"/api/v1/{kind}.info?roomName={name}", None, self.dev)
            rid = ((reply or {}).get(key) or {}).get("_id")
        if not rid:
            raise SmokeFail(f"could not resolve the seeded room {name!r}")
        return rid

    def post_as_alice(self, rid, text):
        """Post as alice; returns the new message's id (for `edit_as_alice` / `delete_as_alice`)."""
        reply = rc_call(self.base, "POST", "/api/v1/chat.postMessage", {"roomId": rid, "text": text}, self.alice)
        if not (reply or {}).get("success"):
            raise SmokeFail("alice's post was refused")
        return ((reply or {}).get("message") or {}).get("_id")

    def create_member_room(self, admin, name):
        """A public channel made by admin with `dev` as a plain member (not an owner). `dev` owns the
        seeded rooms, and an owner may edit and delete whatever the settings say, so a refusal can
        only be seen in a room like this. Returns the room id."""
        reply = rc_call(
            self.base, "POST", "/api/v1/channels.create", {"name": name, "members": ["dev"]}, admin
        )
        rid = ((reply or {}).get("channel") or {}).get("_id")
        if not rid:
            raise SmokeFail("could not create the member-only room")
        return rid

    def delete_room(self, admin, rid):
        rc_call(self.base, "POST", "/api/v1/channels.delete", {"roomId": rid}, admin)

    def post_as_dev(self, rid, text):
        """Post as the app's own account (`dev`); returns the new message's id."""
        reply = rc_call(self.base, "POST", "/api/v1/chat.postMessage", {"roomId": rid, "text": text}, self.dev)
        if not (reply or {}).get("success"):
            raise SmokeFail("dev's post was refused")
        return ((reply or {}).get("message") or {}).get("_id")

    def message_text(self, msg_id):
        """The text the server holds for `msg_id` (read as dev), or None when it holds no such message."""
        reply = rc_call(self.base, "GET", f"/api/v1/chat.getMessage?msgId={msg_id}", None, self.dev)
        message = (reply or {}).get("message")
        return message.get("msg") if isinstance(message, dict) else None

    def edit_as_alice(self, rid, msg_id, text):
        reply = rc_call(
            self.base, "POST", "/api/v1/chat.update", {"roomId": rid, "msgId": msg_id, "text": text}, self.alice
        )
        if not (reply or {}).get("success"):
            raise SmokeFail("alice's edit was refused")

    def delete_as_alice(self, rid, msg_id):
        reply = rc_call(self.base, "POST", "/api/v1/chat.delete", {"roomId": rid, "msgId": msg_id}, self.alice)
        if not (reply or {}).get("success"):
            raise SmokeFail("alice's delete was refused")

    def mark_read(self, rid):
        rc_call(self.base, "POST", "/api/v1/subscriptions.read", {"rid": rid}, self.dev)


class Proxy:
    """A loopback TCP forwarder in front of the server, so a scenario can cut the app's connection
    (the WebSocket and every REST call) while the harness keeps posting straight to the server.
    `sever` closes every open pipe and the listening socket; `restore` listens again on the same port."""

    def __init__(self, target_port):
        self.target = ("127.0.0.1", target_port)
        probe = socket.socket()
        probe.bind(("127.0.0.1", 0))
        self.port = probe.getsockname()[1]
        probe.close()
        self.pipes = []
        self.lock = threading.Lock()
        self.listener = None
        self.restore()

    def restore(self):
        listener = socket.socket()
        listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        listener.bind(("127.0.0.1", self.port))
        listener.listen(16)
        self.listener = listener
        threading.Thread(target=self._accept, args=(listener,), daemon=True).start()

    def _accept(self, listener):
        while True:
            try:
                client, _ = listener.accept()
            except OSError:
                return
            try:
                upstream = socket.create_connection(self.target, timeout=10)
            except OSError:
                client.close()
                continue
            with self.lock:
                self.pipes.extend([client, upstream])
            for src, dst in ((client, upstream), (upstream, client)):
                threading.Thread(target=self._pump, args=(src, dst), daemon=True).start()

    @staticmethod
    def _pump(src, dst):
        try:
            while True:
                data = src.recv(65536)
                if not data:
                    break
                dst.sendall(data)
        except OSError:
            pass
        for sock in (src, dst):
            try:
                sock.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            sock.close()

    def sever(self):
        if self.listener is not None:
            self.listener.close()
            self.listener = None
        with self.lock:
            pipes, self.pipes = self.pipes, []
        for sock in pipes:
            try:
                sock.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            sock.close()


def rows_with(buf, text):
    return [row for row in screen_of(buf).rows() if text in row]


def row_of(buf, text):
    """The screen row index of the first row that shows `text`, or None."""
    for index, row in enumerate(screen_of(buf).rows()):
        if text in row:
            return index
    return None


def tombstone_at(buf, index):
    """Whether screen row `index` is a deleted line by alice: her name and `message deleted`."""
    row = screen_of(buf).rows()[index]
    return "alice" in row and "message deleted" in row


def finish_clean(pid, fd, buf, secrets_in_output, texts, label):
    """Quit with `:q` and check the run: exit code 0, no secret anywhere in the output, and none of
    the message `texts` after the app left the alternate screen (they are on the TUI screen by
    design, but must never reach the normal screen, a log or stderr)."""
    os.write(fd, b"\x1b")
    time.sleep(0.3)
    type_keys(fd, buf, b":q")
    type_keys(fd, buf, b"\r")
    code = wait_exit(pid, fd, buf, 10)
    if code != 0:
        raise SmokeFail(f"{label}: exit code {code}, expected 0")
    raw = bytes(buf)
    for secret in secrets_in_output:
        if secret and secret.encode() in raw:
            raise SmokeFail(f"{label}: a secret appears in the terminal output")
    tail = raw[raw.rfind(ALT_OFF) :] if ALT_OFF in raw else b""
    for text in texts:
        if text.encode() in tail:
            raise SmokeFail(f"{label}: message text appears outside the alternate screen")


def scenario_docker_realtime(binary):
    """Live messages and unread flags against the real local Rocket.Chat: a message alice posts in
    the open room appears without a command, a post in the DM raises its unread flag, and the echo
    of the app's own send shows once. Posts go to rt-private and the alice DM only; the DM and
    rt-private are marked read through the REST API first so the flags start clean."""
    project = os.environ.get("RT_PROJECT", "rockettui-dev")
    stack = Seeded(project)
    private = stack.room_id("rt-private", "groups")
    dm = stack.room_id("alice", "dm")
    stack.mark_read(private)
    stack.mark_read(dm)
    tag = secrets.token_hex(3)
    live_text, dm_text, own_text = f"rl-{tag}-a", f"rl-{tag}-b", f"rl-{tag}-c"

    pid, fd = spawn(binary, extra_env=stack.env)
    buf = bytearray()
    wait_ready(fd, buf, room=b"rt-public", timeout=20)
    open_room_named(fd, buf, "rt-private")
    if not read_until(fd, buf, history_settled, 10):
        raise SmokeFail("rt-private history never settled")
    # The connection is made after the rooms load; give the login and subscriptions a moment.
    time.sleep(2.0)

    stack.post_as_alice(private, live_text)
    if not read_until(fd, buf, lambda b: rows_with(b, live_text), 10):
        raise SmokeFail("alice's message never appeared live in the open room")

    stack.post_as_alice(dm, dm_text)

    def dm_flagged(b):
        rows = sidebar_rows(screen_of(b))
        return any(room_name_of(r) == "alice" and r.strip().startswith("*") for r in rows)

    if not read_until(fd, buf, dm_flagged, 10):
        raise SmokeFail("the DM did not get its unread flag live")
    # The server follows a message with a second subscription update that has no unread keys; the
    # flag must survive it.
    read_until(fd, buf, lambda b: False, 1.5)
    if not dm_flagged(buf):
        raise SmokeFail("the DM's unread flag was cleared again by a later update")

    type_keys(fd, buf, b"i")
    type_keys(fd, buf, own_text.encode())
    type_keys(fd, buf, b"\r")
    wait_confirmed(fd, buf, own_text, timeout=15)
    read_until(fd, buf, lambda b: False, 1.5)  # let the echo arrive
    if len(rows_with(buf, own_text)) != 1:
        raise SmokeFail("the echo of the app's own send was shown more than once")
    if len(rows_with(buf, live_text)) != 1:
        raise SmokeFail("the live message is shown more than once")
    finish_clean(pid, fd, buf, stack.secrets, [live_text, dm_text, own_text], "docker_realtime")


def scenario_docker_edits(binary):
    """Live edits and deletes against the real local Rocket.Chat. In rt-private, alice posts two
    messages; she edits the first (the new text and the edited marker show, in the same row) and
    deletes the second (a deleted line with her name and no text, in the same row). In the alice DM,
    which the app has not opened, she posts and deletes a message; opening the DM shows no trace.
    Posts go to rt-private and the alice DM only, never rt-public."""
    project = os.environ.get("RT_PROJECT", "rockettui-dev")
    stack = Seeded(project)
    private = stack.room_id("rt-private", "groups")
    dm = stack.room_id("alice", "dm")
    stack.mark_read(private)
    tag = secrets.token_hex(3)
    # Distinct names, none a substring of another, so "old text gone" is a real check.
    first, first_new, second, unseen = (f"ed-{tag}-{n}" for n in ("old", "new", "gone", "unseen"))

    pid, fd = spawn(binary, extra_env=stack.env)
    buf = bytearray()
    wait_ready(fd, buf, room=b"rt-public", timeout=20)
    open_room_named(fd, buf, "rt-private")
    if not read_until(fd, buf, history_settled, 10):
        raise SmokeFail("rt-private history never settled")
    time.sleep(2.0)  # login and subscriptions, including the per-room delete streams

    first_id = stack.post_as_alice(private, first)
    if not read_until(fd, buf, lambda b: rows_with(b, first), 10):
        raise SmokeFail("alice's first message never appeared live")
    second_id = stack.post_as_alice(private, second)
    if not read_until(fd, buf, lambda b: rows_with(b, second), 10):
        raise SmokeFail("alice's second message never appeared live")
    read_until(fd, buf, lambda b: False, 0.5)
    first_row, second_row = row_of(buf, first), row_of(buf, second)

    stack.edit_as_alice(private, first_id, first_new)
    if not read_until(fd, buf, lambda b: rows_with(b, first_new), 10):
        raise SmokeFail("the edited text never appeared")
    read_until(fd, buf, lambda b: False, 0.5)
    if "(edited)" not in screen_of(buf).rows()[row_of(buf, first_new)]:
        raise SmokeFail("the edited message has no edited marker")
    if rows_with(buf, first):
        raise SmokeFail("the old text is still shown after the edit")
    if row_of(buf, first_new) != first_row or row_of(buf, second) != second_row:
        raise SmokeFail("an edit moved a row")

    stack.delete_as_alice(private, second_id)
    if not read_until(fd, buf, lambda b: tombstone_at(b, second_row), 10):
        raise SmokeFail("the deleted message did not become a deleted line in its row")
    if rows_with(buf, second):
        raise SmokeFail("the deleted text is still shown")
    if row_of(buf, first_new) != first_row:
        raise SmokeFail("a delete moved a row")

    # A message created and deleted in a room the app has not opened: no trace after opening it.
    unseen_id = stack.post_as_alice(dm, unseen)
    stack.delete_as_alice(dm, unseen_id)
    read_until(fd, buf, lambda b: False, 1.5)
    open_room_named(fd, buf, "alice")
    if not read_until(fd, buf, history_settled, 10):
        raise SmokeFail("the alice DM history never settled")
    read_until(fd, buf, lambda b: False, 1.0)
    if rows_with(buf, unseen) or rows_with(buf, "message deleted"):
        raise SmokeFail("a message deleted before the room was opened left a trace")
    finish_clean(pid, fd, buf, stack.secrets, [first, first_new, second, unseen], "docker_edits")


def scenario_docker_deleted_status(binary):
    """Deleted messages when the server keeps them (setting Message_ShowDeletedStatus): the server
    then turns a delete into a message of type `rm` (empty text, `editedAt` set, original author)
    instead of dropping it. A live delete in rt-private must become the deleted line, and a deleted
    message in the alice DM, loaded from history, must show as one too: never `system message`,
    never an edited marker, never text. The setting is restored afterwards, even on failure."""
    project = os.environ.get("RT_PROJECT", "rockettui-dev")
    stack = Seeded(project)
    admin = stack.admin_login(project)
    private = stack.room_id("rt-private", "groups")
    dm = stack.room_id("alice", "dm")
    stack.mark_read(private)
    tag = secrets.token_hex(3)
    live, unseen = f"ds-{tag}-live", f"ds-{tag}-unseen"
    before = stack.setting(admin, "Message_ShowDeletedStatus")
    pid = None
    try:
        stack.setting(admin, "Message_ShowDeletedStatus", True)
        pid, fd = spawn(binary, extra_env=stack.env)
        buf = bytearray()
        wait_ready(fd, buf, room=b"rt-public", timeout=20)
        open_room_named(fd, buf, "rt-private")
        if not read_until(fd, buf, history_settled, 10):
            raise SmokeFail("rt-private history never settled")
        time.sleep(2.0)

        live_id = stack.post_as_alice(private, live)
        if not read_until(fd, buf, lambda b: rows_with(b, live), 10):
            raise SmokeFail("alice's message never appeared live")
        read_until(fd, buf, lambda b: False, 0.5)
        row = row_of(buf, live)
        stack.delete_as_alice(private, live_id)
        if not read_until(fd, buf, lambda b: tombstone_at(b, row), 10):
            raise SmokeFail("a live delete (kept as type rm) did not become a deleted line")
        screen = "\n".join(screen_of(buf).rows())
        if live in screen or "system message" in screen:
            raise SmokeFail("the deleted message still shows its text or as a system message")
        if "(edited)" in screen_of(buf).rows()[row]:
            raise SmokeFail("the deleted line carries an edited marker")

        unseen_id = stack.post_as_alice(dm, unseen)
        stack.delete_as_alice(dm, unseen_id)
        read_until(fd, buf, lambda b: False, 1.0)
        open_room_named(fd, buf, "alice")
        if not read_until(fd, buf, history_settled, 10):
            raise SmokeFail("the alice DM history never settled")
        read_until(fd, buf, lambda b: False, 1.0)
        screen = "\n".join(screen_of(buf).rows())
        if unseen in screen or "system message" in screen:
            raise SmokeFail("a deleted message loaded from history shows its text or as a system message")
        if not rows_with(buf, "message deleted"):
            raise SmokeFail("a deleted message loaded from history is not drawn as a deleted line")
        if any("(edited)" in r for r in rows_with(buf, "message deleted")):
            raise SmokeFail("a deleted line loaded from history carries an edited marker")
        finish_clean(pid, fd, buf, stack.secrets, [live, unseen], "docker_deleted_status")
        pid = None
    finally:
        stack.setting(admin, "Message_ShowDeletedStatus", bool(before))
        if pid is not None:
            try:
                os.kill(pid, signal.SIGKILL)
                os.waitpid(pid, 0)
            except OSError:
                pass


def scenario_docker_reconnect(binary):
    """Reconnect and gap recovery against the real local Rocket.Chat. The app talks to the server
    through a `Proxy`. With rt-private open and the alice DM loaded, the proxy cuts every
    connection and refuses new ones; alice posts straight to the server (3 messages in rt-private,
    more than a page in the DM); the proxy comes back. The open room must catch up without
    duplicates, and the DM, stale, must refetch when opened and show its newest message once."""
    project = os.environ.get("RT_PROJECT", "rockettui-dev")
    stack = Seeded(project)
    private = stack.room_id("rt-private", "groups")
    dm = stack.room_id("alice", "dm")
    port = urllib.parse.urlsplit(stack.base).port
    proxy = Proxy(port)
    env = dict(stack.env)
    env["ROCKETTUI_SERVER_URL"] = f"http://127.0.0.1:{proxy.port}"
    tag = secrets.token_hex(3)
    private_texts = [f"rg-{tag}-p{i}" for i in range(3)]
    kept, kept_new, doomed = f"rg-{tag}-kept", f"rg-{tag}-edit", f"rg-{tag}-doomed"
    dm_count = 55  # more than the default history_size of 50
    dm_texts = [f"rg-{tag}-d{i:02d}" for i in range(dm_count)]

    pid, fd = spawn(binary, extra_env=env)
    buf = bytearray()
    try:
        wait_ready(fd, buf, room=b"rt-public", timeout=20)
        open_room_named(fd, buf, "alice")
        if not read_until(fd, buf, history_settled, 10):
            raise SmokeFail("the alice DM history never settled")
        open_room_named(fd, buf, "rt-private")
        if not read_until(fd, buf, history_settled, 10):
            raise SmokeFail("rt-private history never settled")
        time.sleep(2.0)  # connection live

        # Two messages the app holds, which alice edits and deletes while it is cut off. Each is
        # older than the newest second of the refetched page, so the reconcile judges it.
        kept_id = stack.post_as_alice(private, kept)
        doomed_id = stack.post_as_alice(private, doomed)
        if not read_until(fd, buf, lambda b: rows_with(b, kept) and rows_with(b, doomed), 10):
            raise SmokeFail("the two held messages never appeared live")
        time.sleep(1.2)

        proxy.sever()
        if not read_until(fd, buf, lambda b: "Reconnecting" in screen_of(b).rows()[22], 15):
            raise SmokeFail("the status line never showed that the app is reconnecting")
        stack.edit_as_alice(private, kept_id, kept_new)
        stack.delete_as_alice(private, doomed_id)
        time.sleep(1.2)
        for text in private_texts:
            stack.post_as_alice(private, text)
        for text in dm_texts:
            stack.post_as_alice(dm, text)
        proxy.restore()

        gone = lambda b: "Reconnecting" not in screen_of(b).rows()[22]
        if not read_until(fd, buf, gone, 60):
            raise SmokeFail("the reconnecting status never cleared")
        if not read_until(fd, buf, lambda b: all(rows_with(b, t) for t in private_texts), 15):
            raise SmokeFail("the open room did not catch up after the reconnect")
        read_until(fd, buf, lambda b: False, 1.0)
        for text in private_texts:
            if len(rows_with(buf, text)) != 1:
                raise SmokeFail("a caught-up message is shown more than once")
        # The edit and the delete that happened during the outage show after the refetch.
        # The reconnect can wait out a backoff of up to 30 s after a longer outage.
        if not read_until(fd, buf, lambda b: rows_with(b, kept_new), 60):
            raise SmokeFail("the edit made during the outage did not show after the reconnect")
        if "(edited)" not in screen_of(buf).rows()[row_of(buf, kept_new)] or rows_with(buf, kept):
            raise SmokeFail("the edited message shows no marker or still shows its old text")
        if rows_with(buf, doomed) or not rows_with(buf, "message deleted"):
            raise SmokeFail("the delete made during the outage did not become a deleted line")

        open_room_named(fd, buf, "alice")
        newest = dm_texts[-1]
        if not read_until(fd, buf, lambda b: rows_with(b, newest), 15):
            raise SmokeFail("the stale DM did not show its newest message after opening")
        read_until(fd, buf, lambda b: False, 1.0)
        if len(rows_with(buf, newest)) != 1:
            raise SmokeFail("the newest message of the stale DM is shown more than once")
        finish_clean(
            pid, fd, buf, stack.secrets, private_texts + dm_texts + [kept, kept_new, doomed], "docker_reconnect"
        )
    finally:
        proxy.sever()
        try:
            os.kill(pid, signal.SIGKILL)
            os.waitpid(pid, 0)
        except OSError:
            pass


def esc(fd):
    """A lone ESC followed by a pause, so crossterm does not read ESC plus the next byte as Alt+key."""
    os.write(fd, b"\x1b")
    time.sleep(0.3)


def status_row(buf):
    return screen_of(buf).rows()[22]


def open_private_with_own_ready(binary, stack):
    """Start the app on the seeded stack, open rt-private and give the own username time to load
    (`e` and `dd` do nothing until it is known)."""
    pid, fd = spawn(binary, extra_env=stack.env)
    buf = bytearray()
    wait_ready(fd, buf, room=b"rt-public", timeout=20)
    open_room_named(fd, buf, "rt-private")
    if not read_until(fd, buf, history_settled, 10):
        raise SmokeFail("rt-private history never settled")
    time.sleep(2.0)  # the own username (GET /me) and the realtime login
    return pid, fd, buf


def scenario_docker_edit_own(binary):
    """Editing the user's own message against the real local Rocket.Chat. `dev` posts a message in
    rt-private; in the app `e` shows its text in the input (Insert mode, the title says Enter saves
    and Esc cancels), Esc restores a half-typed draft, and a changed text saves: the row shows the
    new text and the edited marker in the same row. With Message_AllowEditing off (set through
    admin, restored in `finally`) a save shows the fixed line and keeps the typed text. Posts go to
    rt-private only."""
    project = os.environ.get("RT_PROJECT", "rockettui-dev")
    stack = Seeded(project)
    admin = stack.admin_login(project)
    private = stack.room_id("rt-private", "groups")
    tag = secrets.token_hex(3)
    old, new, refused, draft = (f"eo-{tag}-{n}" for n in ("old", "new", "nope", "draft"))
    original = stack.setting(admin, "Message_AllowEditing")
    msg_id = stack.post_as_dev(private, old)
    # `dev` owns rt-private, and an owner may edit whatever the setting says: the refusal is
    # checked in a room where `dev` is a plain member.
    member_name = f"rt-eo-{tag}"
    member_room = stack.create_member_room(admin, member_name)
    member_msg = stack.post_as_dev(member_room, refused)
    try:
        pid, fd, buf = open_private_with_own_ready(binary, stack)
        if not read_until(fd, buf, lambda b: rows_with(b, old), 10):
            raise SmokeFail("the own message is not shown in rt-private")
        row = row_of(buf, old)

        # A half-typed draft, then `e`: the message text is in the input and the title says so.
        type_keys(fd, buf, b"i")
        type_keys(fd, buf, draft.encode())
        esc(fd)
        type_keys(fd, buf, b"e")
        if not read_until(fd, buf, lambda b: screen_of(b).contains("Editing: Enter saves, Esc cancels"), 5):
            raise SmokeFail("`e` on the own message did not start editing")
        if len(rows_with(buf, old)) != 2:
            raise SmokeFail("the message text is not in the input bar")
        if not read_until(fd, buf, lambda b: "INSERT" in status_row(b), 5):
            raise SmokeFail("editing is not in Insert mode")
        # Esc cancels and the draft is back.
        esc(fd)
        if not read_until(fd, buf, lambda b: not screen_of(b).contains("Editing: Enter"), 5):
            raise SmokeFail("Esc did not leave the editing state")
        if not rows_with(buf, draft) or len(rows_with(buf, old)) != 1:
            raise SmokeFail("the earlier draft was not restored after cancelling")

        # Edit and save: the new text and the marker, in the same row; the draft is back after.
        type_keys(fd, buf, b"e")
        read_until(fd, buf, lambda b: screen_of(b).contains("Editing: Enter saves"), 5)
        type_keys(fd, buf, b"\x15")  # Ctrl-u: clear the line
        type_keys(fd, buf, new.encode())
        type_keys(fd, buf, b"\r")
        saved = lambda b: any(new in r and "(edited)" in r for r in screen_of(b).rows()[1:19])
        if not read_until(fd, buf, saved, 10):
            raise SmokeFail("the edited text and its marker never appeared in the message list")
        read_until(fd, buf, lambda b: False, 0.8)
        if row_of(buf, new) != row:
            raise SmokeFail("the edit moved the row")
        if len([r for r in rows_with(buf, new) if "(edited)" in r]) != 1:
            raise SmokeFail("the edited message is shown more than once")
        if rows_with(buf, old):
            raise SmokeFail("the old text is still shown after the edit")
        if stack.message_text(msg_id) != new:
            raise SmokeFail("the server does not hold the edited text")
        if screen_of(buf).contains("Editing: Enter") or not rows_with(buf, draft):
            raise SmokeFail("saving did not restore the earlier draft")

        # A refused edit, in the member-only room: the fixed line, the typed text kept, the message
        # unchanged.
        open_room_named(fd, buf, member_name)
        if not read_until(fd, buf, lambda b: history_settled(b) and rows_with(b, refused), 10):
            raise SmokeFail("the member-only room's message is not shown")
        time.sleep(0.5)
        stack.setting(admin, "Message_AllowEditing", False)
        typed = refused + "-x"
        type_keys(fd, buf, b"e")
        read_until(fd, buf, lambda b: screen_of(b).contains("Editing: Enter saves"), 5)
        type_keys(fd, buf, b"\x15")
        type_keys(fd, buf, typed.encode())
        type_keys(fd, buf, b"\r")
        if not read_until(fd, buf, lambda b: "edit not saved" in status_row(b), 10):
            raise SmokeFail("a refused edit did not show the fixed status line")
        if not screen_of(buf).contains("Editing: Enter") or len(rows_with(buf, typed)) != 1:
            raise SmokeFail("a refused edit did not keep the typed text in the input")
        if len(rows_with(buf, refused)) != 2 or stack.message_text(member_msg) != refused:
            raise SmokeFail("the message changed after a refused edit")
        esc(fd)
        finish_clean(pid, fd, buf, stack.secrets, [old, new, refused, typed, draft], "docker_edit_own")
    finally:
        stack.setting(admin, "Message_AllowEditing", original)
        stack.delete_room(admin, member_room)


def scenario_docker_delete_own(binary):
    """Deleting the user's own message against the real local Rocket.Chat. In rt-private the app
    user has two messages around one by alice. `dd` then another key deletes nothing; `dd` then `y`
    turns the newest own message into the deleted line in the same row; `e` and `dd` on alice's
    message change nothing and send nothing (the server still holds her text); with
    Message_AllowDeleting off (set through admin, restored in `finally`) a confirmed delete shows the
    fixed line and the message stays. Posts go to rt-private only."""
    project = os.environ.get("RT_PROJECT", "rockettui-dev")
    stack = Seeded(project)
    admin = stack.admin_login(project)
    private = stack.room_id("rt-private", "groups")
    tag = secrets.token_hex(3)
    kept, theirs, doomed = (f"do-{tag}-{n}" for n in ("kept", "alice", "gone"))
    original = stack.setting(admin, "Message_AllowDeleting")
    theirs_id = stack.post_as_alice(private, theirs)
    doomed_id = stack.post_as_dev(private, doomed)
    # `dev` owns rt-private, and an owner may delete whatever the setting says: the refusal is
    # checked in a room where `dev` is a plain member.
    member_name = f"rt-do-{tag}"
    member_room = stack.create_member_room(admin, member_name)
    kept_id = stack.post_as_dev(member_room, kept)
    try:
        pid, fd, buf = open_private_with_own_ready(binary, stack)
        if not read_until(fd, buf, lambda b: rows_with(b, doomed) and rows_with(b, theirs), 10):
            raise SmokeFail("the seeded messages are not shown in rt-private")
        doomed_row, theirs_row = row_of(buf, doomed), row_of(buf, theirs)

        # `dd` asks first; any other key cancels, the cursor does not move, nothing is deleted.
        type_keys(fd, buf, b"dd")
        if not read_until(fd, buf, lambda b: "Delete this message?" in status_row(b), 5):
            raise SmokeFail("`dd` on the own message did not open the prompt")
        type_keys(fd, buf, b"j")
        read_until(fd, buf, lambda b: "Delete this message?" not in status_row(b), 5)
        if "Delete this message?" in status_row(buf):
            raise SmokeFail("another key did not close the prompt")
        read_until(fd, buf, lambda b: False, 0.5)
        if stack.message_text(doomed_id) != doomed or row_of(buf, doomed) != doomed_row:
            raise SmokeFail("cancelling the prompt changed the message")

        # `dd` then `y`: the deleted line, in the same row, and the server no longer holds it.
        type_keys(fd, buf, b"dd")
        read_until(fd, buf, lambda b: "Delete this message?" in status_row(b), 5)
        type_keys(fd, buf, b"y")
        gone = lambda b: "dev" in screen_of(b).rows()[doomed_row] and "message deleted" in screen_of(b).rows()[doomed_row]
        if not read_until(fd, buf, gone, 10):
            raise SmokeFail("the deleted message did not become a deleted line in its row")
        if rows_with(buf, doomed):
            raise SmokeFail("the deleted text is still shown")
        if stack.message_text(doomed_id) is not None:
            raise SmokeFail("the server still holds the deleted message")

        # Alice's message: nothing happens, nothing is sent.
        type_keys(fd, buf, b"k")
        read_until(fd, buf, lambda b: False, 0.4)
        before = screen_of(buf).rows()
        type_keys(fd, buf, b"e")
        type_keys(fd, buf, b"dd")
        read_until(fd, buf, lambda b: False, 0.8)
        if screen_of(buf).contains("Editing: Enter") or "Delete this message?" in status_row(buf):
            raise SmokeFail("`e` or `dd` acted on someone else's message")
        if "NORMAL" not in status_row(buf):
            raise SmokeFail("`e` on someone else's message changed the mode")
        if screen_of(buf).rows()[1:19] != before[1:19]:
            raise SmokeFail("`e` or `dd` on someone else's message changed the screen")
        type_keys(fd, buf, b"y")  # not a prompt answer here: `y` is unbound and does nothing
        if stack.message_text(theirs_id) != theirs or row_of(buf, theirs) != theirs_row:
            raise SmokeFail("alice's message changed")

        # A refused delete, in the member-only room: the fixed line, the message stays.
        open_room_named(fd, buf, member_name)
        if not read_until(fd, buf, lambda b: history_settled(b) and rows_with(b, kept), 10):
            raise SmokeFail("the member-only room's message is not shown")
        time.sleep(0.5)
        kept_row = row_of(buf, kept)
        stack.setting(admin, "Message_AllowDeleting", False)
        type_keys(fd, buf, b"dd")
        read_until(fd, buf, lambda b: "Delete this message?" in status_row(b), 5)
        type_keys(fd, buf, b"y")
        if not read_until(fd, buf, lambda b: "message not deleted" in status_row(b), 10):
            raise SmokeFail("a refused delete did not show the fixed status line")
        if row_of(buf, kept) != kept_row or stack.message_text(kept_id) != kept:
            raise SmokeFail("a refused delete changed the message")
        finish_clean(pid, fd, buf, stack.secrets, [kept, theirs, doomed], "docker_delete_own")
    finally:
        stack.setting(admin, "Message_AllowDeleting", original)
        stack.delete_room(admin, member_room)


SCENARIOS = {
    "quit": scenario_quit,
    "panic": scenario_panic,
    "insert": scenario_insert,
    "send": scenario_send,
    "send_retry": scenario_send_retry,
    "send_author": scenario_send_author,
    "send_unconfirmed": scenario_send_unconfirmed,
    "quit_undelivered": scenario_quit_undelivered,
    "config_layout": scenario_config_layout,
    "config_keymap": scenario_config_keymap,
    "config_error": scenario_config_error,
    "connect_read": scenario_connect_read,
    "missing_creds": scenario_missing_creds,
    "config_creds": scenario_config_creds,
    "perm_warning": scenario_perm_warning,
    "unauthorized": scenario_unauthorized,
    "docker_read": scenario_docker_read,
    "docker_send": scenario_docker_send,
    "docker_search": scenario_docker_search,
    "docker_realtime": scenario_docker_realtime,
    "docker_reconnect": scenario_docker_reconnect,
    "docker_edits": scenario_docker_edits,
    "docker_deleted_status": scenario_docker_deleted_status,
    "docker_edit_own": scenario_docker_edit_own,
    "docker_delete_own": scenario_docker_delete_own,
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
