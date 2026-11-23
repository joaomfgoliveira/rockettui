#!/usr/bin/env python3
"""Live probe of the Rocket.Chat edit and delete contract for rockettui (edit-delete-own-messages).

Usage: python3 -I dev/probe_edit.py        (from the repository root)

Standard library only. Runs against the throwaway local server that dev/seed.py started
(RT_PROJECT, default rockettui-dev). It asks how `chat.update` and `chat.delete` answer for the
app user on its own message and on someone else's, with the settings that limit them switched off
or tightened, and whether the own edit and delete come back on the realtime stream.

Output rule: every line goes through out(). A line holds only a probe id, an endpoint, a status
code, top-level key names, yes/no flags, a server `errorType` code (matched against a strict
pattern) and closed error classes. It never holds a credential, a message text, a response body, a
header or the server's error text. out() refuses any line that contains a secret value. The server
must be on loopback: this script changes settings and creates and deletes rooms as admin and must
never touch the company server (CLAUDE.md Policy).

Settings it changes (Message_AllowEditing, Message_AllowDeleting and the two Block*InMinutes
limits) are read first and restored in a `finally`; the last lines say whether they are back.

The last line is `PROBE OK` only when the own edit and delete answered 200, the settings are back
to their original values and every room this run created was deleted.
"""

import base64
import json
import os
import re
import secrets
import socket
import struct
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

PROJECT = os.environ.get("RT_PROJECT", "rockettui-dev")
REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
STATE_DIR = os.path.join(REPO, "dev", "state", PROJECT)
APP_ENV = os.path.join(STATE_DIR, ".env.local")
ADMIN_ENV = os.path.join(STATE_DIR, ".env.admin.local")
LOOPBACK = ("127.0.0.1", "localhost", "::1")

SETTINGS = (
    "Message_AllowEditing",
    "Message_AllowDeleting",
    "Message_AllowEditing_BlockEditInMinutes",
    "Message_AllowDeleting_BlockDeleteInMinutes",
)
ERROR_TYPE = re.compile(r"^[A-Za-z0-9_.-]{1,64}$")

SECRETS = set()
BASE = ""


def out(line):
    """The only way anything is printed. Refuses a line holding a secret value."""
    for secret in SECRETS:
        if secret and secret in line:
            print("PROBE FAIL: refusing to print a secret value")
            sys.exit(1)
    print(line)
    sys.stdout.flush()


def fail(reason):
    out("PROBE FAIL: %s" % reason)
    sys.exit(1)


def read_env(path):
    values = {}
    with open(path) as handle:
        for line in handle:
            line = line.strip()
            if line and not line.startswith("#") and "=" in line:
                key, _, value = line.partition("=")
                values[key] = value
    return values


def api(method, path, body=None, auth=None, query=None):
    """JSON request. Returns (status, parsed_json_or_None); returns on 4xx/5xx instead of raising."""
    headers = {"Content-Type": "application/json"}
    if auth is not None:
        headers["X-User-Id"] = auth[0]
        headers["X-Auth-Token"] = auth[1]
    url = BASE + path
    if query:
        url += "?" + urllib.parse.urlencode(query)
    data = None if body is None else json.dumps(body).encode()
    request = urllib.request.Request(url, data=data, method=method, headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            status, raw = response.status, response.read()
    except urllib.error.HTTPError as error:
        status, raw = error.code, error.read()
    except (urllib.error.URLError, OSError):
        return 0, None
    try:
        parsed = json.loads(raw) if raw else None
    except ValueError:
        parsed = None
    return status, parsed


def login(user, password):
    status, reply = api("POST", "/api/v1/login", {"user": user, "password": password})
    data = (reply or {}).get("data") or {}
    if status != 200 or "authToken" not in data:
        fail("admin login did not succeed")
    SECRETS.add(data["authToken"])
    SECRETS.add(data["userId"])
    return data["userId"], data["authToken"]


def keys(parsed):
    if isinstance(parsed, dict):
        return ",".join(sorted(parsed.keys())) or "none"
    return "none"


def error_type(parsed):
    value = parsed.get("errorType") if isinstance(parsed, dict) else None
    if value is None:
        return "absent"
    if isinstance(value, str) and ERROR_TYPE.match(value):
        return value
    return "other"


def error_class(parsed):
    """Closed set derived by case-insensitive substring from the error and errorType strings."""
    if not isinstance(parsed, dict):
        return "absent"
    text = " ".join(str(parsed.get(k, "")) for k in ("error", "errorType")).lower()
    if not text.strip():
        return "absent"
    rules = (
        ("time_limit", ("blocked", "time", "minutes")),
        ("invalid_params", ("invalid-params", "must not have additional", "match failed")),
        ("not_found", ("invalid-message", "not-found", "not found", "no message")),
        ("not_allowed", ("not-allowed", "not allowed", "unauthorized", "not authorized")),
        ("type_error", ("typeerror", "cannot read properties", "undefined")),
    )
    for label, needles in rules:
        if any(needle in text for needle in needles):
            return label
    return "other"


def line(probe, endpoint, status, parsed, extra=""):
    out(
        "%s %s status=%d keys=%s error_type=%s error_class=%s%s"
        % (probe, endpoint, status, keys(parsed), error_type(parsed), error_class(parsed), extra)
    )


def post_message(auth, room, text):
    status, parsed = api("POST", "/api/v1/chat.sendMessage", {"message": {"rid": room, "msg": text}}, auth)
    time.sleep(0.3)  # chat.sendMessage has its own five-per-second limiter
    message = (parsed or {}).get("message") if isinstance(parsed, dict) else None
    mid = message.get("_id") if isinstance(message, dict) else None
    if status != 200 or not mid:
        fail("setup could not post a probe message")
    return mid


def update(auth, room, mid, text):
    status, parsed = api("POST", "/api/v1/chat.update", {"roomId": room, "msgId": mid, "text": text}, auth)
    time.sleep(0.3)
    return status, parsed


def delete(auth, room, mid):
    status, parsed = api("POST", "/api/v1/chat.delete", {"roomId": room, "msgId": mid}, auth)
    time.sleep(0.3)
    return status, parsed


def set_setting(admin, name, value):
    status, _ = api("POST", "/api/v1/settings/" + name, {"value": value}, admin)
    time.sleep(0.2)
    return status == 200


def get_setting(admin, name):
    status, parsed = api("GET", "/api/v1/settings/" + name, auth=admin)
    if status != 200 or not isinstance(parsed, dict) or "value" not in parsed:
        return None
    return parsed["value"]


# ---------------------------------------------------------------- minimal WebSocket client


class WebSocket:
    def __init__(self, host, port, path):
        self.sock = socket.create_connection((host, port), timeout=10)
        key = base64.b64encode(os.urandom(16)).decode()
        request = (
            "GET %s HTTP/1.1\r\nHost: %s:%d\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
            "Sec-WebSocket-Key: %s\r\nSec-WebSocket-Version: 13\r\n\r\n" % (path, host, port, key)
        )
        self.sock.sendall(request.encode())
        head = b""
        while b"\r\n\r\n" not in head:
            chunk = self.sock.recv(4096)
            if not chunk:
                fail("websocket upgrade: connection closed")
            head += chunk
        _, _, rest = head.partition(b"\r\n\r\n")
        self.buf = rest

    def _need(self, n, deadline):
        while len(self.buf) < n:
            left = deadline - time.time()
            if left <= 0:
                return False
            self.sock.settimeout(left)
            try:
                chunk = self.sock.recv(65536)
            except (socket.timeout, TimeoutError):
                return False
            if not chunk:
                raise ConnectionError("closed")
            self.buf += chunk
        return True

    def send_frame(self, opcode, payload):
        mask = os.urandom(4)
        n = len(payload)
        head = bytes([0x80 | opcode])
        if n < 126:
            head += bytes([0x80 | n])
        elif n < 65536:
            head += bytes([0x80 | 126]) + struct.pack(">H", n)
        else:
            head += bytes([0x80 | 127]) + struct.pack(">Q", n)
        masked = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
        self.sock.sendall(head + mask + masked)

    def send_json(self, obj):
        self.send_frame(1, json.dumps(obj).encode())

    def recv_json(self, timeout):
        """Next JSON text message, or None on timeout. Answers pings itself."""
        deadline = time.time() + timeout
        message = b""
        while True:
            if not self._need(2, deadline):
                return None
            b0, b1 = self.buf[0], self.buf[1]
            n, off = b1 & 0x7F, 2
            if n == 126:
                if not self._need(4, deadline):
                    return None
                n, off = struct.unpack(">H", self.buf[2:4])[0], 4
            elif n == 127:
                if not self._need(10, deadline):
                    return None
                n, off = struct.unpack(">Q", self.buf[2:10])[0], 10
            if not self._need(off + n, deadline):
                return None
            payload = self.buf[off : off + n]
            self.buf = self.buf[off + n :]
            opcode = b0 & 0x0F
            if opcode == 9:
                self.send_frame(10, payload)
                continue
            if opcode == 8:
                raise ConnectionError("server closed")
            if opcode in (1, 0):
                message += payload
                if b0 & 0x80:
                    decoded = json.loads(message)
                    if decoded.get("msg") == "ping":
                        self.send_json({"msg": "pong"})
                        message = b""
                        continue
                    return decoded

    def drain(self, seconds):
        found, end = [], time.time() + seconds
        while time.time() < end:
            msg = self.recv_json(max(0.05, end - time.time()))
            if msg is not None:
                found.append(msg)
        return found

    def wait_for(self, msg_name, key, value, timeout=10):
        end = time.time() + timeout
        while time.time() < end:
            msg = self.recv_json(max(0.05, end - time.time()))
            if msg is not None and msg.get("msg") == msg_name and (
                msg.get(key) == value or value in (msg.get(key) or [])
            ):
                return msg
        return None


def open_stream(dev, user_id, rooms):
    """A DDP connection logged in with the PAT, subscribed to the room-message and delete streams."""
    parsed = urllib.parse.urlparse(BASE)
    ws = WebSocket(parsed.hostname, parsed.port or 80, "/websocket")
    ws.recv_json(5)
    ws.send_json({"msg": "connect", "version": "1", "support": ["1", "pre2", "pre1"]})
    reply = ws.recv_json(5)
    if (reply or {}).get("msg") != "connected":
        fail("DDP connect did not answer `connected`")
    ws.send_json({"msg": "method", "method": "login", "params": [{"resume": dev[1]}], "id": "1"})
    if ws.wait_for("result", "id", "1") is None:
        fail("DDP login did not answer")
    ws.send_json({"msg": "sub", "id": "s-msgs", "name": "stream-room-messages", "params": ["__my_messages__", False]})
    for index, room in enumerate(rooms):
        ws.send_json(
            {
                "msg": "sub",
                "id": "s-del-%d" % index,
                "name": "stream-notify-room",
                "params": ["%s/deleteMessage" % room, False],
            }
        )
    ws.drain(1.5)
    return ws


def stream_summary(events, mid):
    """Count of stream `changed` events that name `mid`, split by collection. Never a value."""
    counts = {}
    for event in events:
        if event.get("msg") != "changed":
            continue
        fields = event.get("fields") or {}
        args = fields.get("args") or []
        first = args[0] if args else None
        if isinstance(first, dict) and (first.get("_id") == mid or first.get("msgId") == mid):
            name = str(event.get("collection"))
            counts[name] = counts.get(name, 0) + 1
    return ",".join("%s=%d" % (k, counts[k]) for k in sorted(counts)) or "none"


def main():
    global BASE
    try:
        app = read_env(APP_ENV)
        admin_env = read_env(ADMIN_ENV)
        BASE = app["ROCKETTUI_SERVER_URL"].rstrip("/")
        dev = (app["ROCKETTUI_USER_ID"], app["ROCKETTUI_TOKEN"])
        admin_pass = admin_env["ADMIN_PASS"]
    except (OSError, KeyError):
        print("PROBE FAIL: no dev state files; run dev/seed.py up and seed first")
        return 1
    SECRETS.update(value for value in (dev[0], dev[1], admin_pass) if value)
    if urllib.parse.urlparse(BASE).hostname not in LOOPBACK:
        out("PROBE FAIL: not a loopback server")
        return 1

    created = []
    originals = {}
    admin = None
    own_ok = None
    try:
        admin = login("admin", admin_pass)
        for name in SETTINGS:
            originals[name] = get_setting(admin, name)
        out(
            "settings read=%d/%d"
            % (sum(1 for value in originals.values() if value is not None), len(SETTINGS))
        )
        if any(value is None for value in originals.values()):
            fail("a setting could not be read, so it could not be restored")

        status, me = api("GET", "/api/v1/me", auth=dev)
        username = me.get("username") if isinstance(me, dict) else None
        if status != 200 or not username:
            fail("no own username, cannot create the probe room")
        name = "rt-probe-edit-%s" % secrets.token_hex(3)
        status, reply = api("POST", "/api/v1/channels.create", {"name": name, "members": [username]}, admin)
        time.sleep(0.3)
        room = ((reply or {}).get("channel") or {}).get("_id")
        if status != 200 or not room:
            fail("setup could not create a probe room")
        created.append(room)

        # The message the time-limit probe will use is posted first so it ages while the rest runs.
        aged = post_message(dev, room, "probe aged text")
        aged_at = time.time()

        ws = open_stream(dev, dev[0], [room])

        # E1: edit an own message. Does it answer 200, with which keys, and does the stream carry it?
        own = post_message(dev, room, "probe own text")
        ws.drain(1.0)
        status, parsed = update(dev, room, own, "probe own text edited")
        message = (parsed or {}).get("message") if isinstance(parsed, dict) else None
        message = message if isinstance(message, dict) else {}
        own_ok = status == 200
        line(
            "E1",
            "POST chat.update",
            status,
            parsed,
            " message_keys=%s id_match=%s edited_at=%s"
            % (
                keys(message),
                "yes" if message.get("_id") == own else "no",
                "yes" if message.get("editedAt") else "no",
            ),
        )
        events = ws.drain(2.0)
        out("E1s stream events for the edit: %s" % stream_summary(events, own))

        # E2: delete the same own message.
        status, parsed = delete(dev, room, own)
        own_ok = own_ok and status == 200
        line("E2", "POST chat.delete", status, parsed, " id_match=%s" % ("yes" if isinstance(parsed, dict) and parsed.get("_id") == own else "no"))
        events = ws.drain(2.0)
        out("E2s stream events for the delete: %s" % stream_summary(events, own))
        ws.sock.close()

        # E3: a message id the room does not hold.
        ghost = "rt" + secrets.token_hex(16) + "0"
        status, parsed = update(dev, room, ghost, "probe ghost")
        line("E3a", "POST chat.update (unknown id)", status, parsed)
        status, parsed = delete(dev, room, ghost)
        line("E3b", "POST chat.delete (unknown id)", status, parsed)

        # E4: someone else's message (posted by admin, the room owner).
        other = post_message(admin, room, "probe other text")
        status, parsed = update(dev, room, other, "probe other changed")
        line("E4a", "POST chat.update (other user)", status, parsed)
        status, parsed = delete(dev, room, other)
        line("E4b", "POST chat.delete (other user)", status, parsed)
        status, parsed = api("GET", "/api/v1/chat.getMessage", auth=dev, query={"msgId": other})
        got = (parsed or {}).get("message") if isinstance(parsed, dict) else None
        out(
            "E4c other user's message still on the server, text unchanged: %s"
            % ("yes" if isinstance(got, dict) and got.get("msg") == "probe other text" else "no")
        )

        # E5: editing switched off, then deleting switched off, on fresh own messages.
        fresh = post_message(dev, room, "probe fresh text")
        if set_setting(admin, "Message_AllowEditing", False):
            status, parsed = update(dev, room, fresh, "probe fresh changed")
            line("E5a", "POST chat.update (editing off)", status, parsed)
            set_setting(admin, "Message_AllowEditing", originals["Message_AllowEditing"])
        else:
            out("E5a SKIPPED setting not changed")
        if set_setting(admin, "Message_AllowDeleting", False):
            status, parsed = delete(dev, room, fresh)
            line("E5b", "POST chat.delete (deleting off)", status, parsed)
            set_setting(admin, "Message_AllowDeleting", originals["Message_AllowDeleting"])
        else:
            out("E5b SKIPPED setting not changed")
        status, parsed = api("GET", "/api/v1/chat.getMessage", auth=dev, query={"msgId": fresh})
        got = (parsed or {}).get("message") if isinstance(parsed, dict) else None
        out(
            "E5c message untouched after both refusals: %s"
            % ("yes" if isinstance(got, dict) and got.get("msg") == "probe fresh text" else "no")
        )

        # E6: the time limits, on the message posted at the start, once it is older than a minute.
        wait = aged_at + 65 - time.time()
        if wait > 0:
            time.sleep(wait)
        if set_setting(admin, "Message_AllowEditing_BlockEditInMinutes", 1):
            status, parsed = update(dev, room, aged, "probe aged changed")
            line("E6a", "POST chat.update (past the edit limit)", status, parsed)
            set_setting(
                admin,
                "Message_AllowEditing_BlockEditInMinutes",
                originals["Message_AllowEditing_BlockEditInMinutes"],
            )
        else:
            out("E6a SKIPPED setting not changed")
        if set_setting(admin, "Message_AllowDeleting_BlockDeleteInMinutes", 1):
            status, parsed = delete(dev, room, aged)
            line("E6b", "POST chat.delete (past the delete limit)", status, parsed)
            set_setting(
                admin,
                "Message_AllowDeleting_BlockDeleteInMinutes",
                originals["Message_AllowDeleting_BlockDeleteInMinutes"],
            )
        else:
            out("E6b SKIPPED setting not changed")
    finally:
        restored = 0
        if admin is not None:
            for name, value in originals.items():
                if value is not None:
                    set_setting(admin, name, value)
            for name, value in originals.items():
                if value is not None and get_setting(admin, name) == value:
                    restored += 1
        out("settings restored=%d/%d" % (restored, len(originals) or len(SETTINGS)))
        deleted = 0
        total = len(created)
        if admin is not None:
            for rid in created:
                status, _ = api("POST", "/api/v1/channels.delete", {"roomId": rid}, admin)
                time.sleep(0.1)
                if status == 200:
                    deleted += 1
        out("cleanup deleted=%d/%d" % (deleted, total))
        cleaned = deleted == total
        settings_back = restored == len(SETTINGS)

    if own_ok is None:
        out("PROBE FAIL: probe did not complete")
        return 1
    if not own_ok:
        out("PROBE FAIL: the own edit or delete did not answer 200")
        return 1
    if not settings_back:
        out("PROBE FAIL: a setting is not back to its original value")
        return 1
    if not cleaned:
        out("PROBE FAIL: cleanup did not delete every created room")
        return 1
    out("PROBE OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
