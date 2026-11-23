#!/usr/bin/env python3
"""Live probe of the Rocket.Chat realtime (DDP over WebSocket) contract for rockettui (realtime-ddp).

Usage: python3 -I dev/probe_ddp.py        (from the repository root)

Standard library only (a minimal hand-rolled WebSocket client, RFC 6455). Runs against the
throwaway local server that dev/seed.py started (RT_PROJECT, default rockettui-dev) and records
how it behaves, so the Rust client is built on measured answers and not on memory.

Safety rules, same spirit as dev/probe_send.py:
- The server MUST be on loopback (127.0.0.1 / localhost / ::1). The script exits otherwise. It
  reads credentials only from dev/state/<RT_PROJECT>/, never from ~/.config/rockettui or from
  the environment, so it cannot be pointed at a company server.
- Every line goes through out(). A line holds only probe ids, DDP message names, collection and
  event names, top-level key names, yes/no flags and counts. Never a credential, a message text,
  a room name or a response body. out() refuses any line that contains a secret value.
- It creates only messages in the seeded rooms (as alice), and edits/deletes only its own.

Last line is `PROBE OK` when the handshake and login worked and every probe ran.
"""

import base64
import hashlib
import json
import os
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

SECRETS = set()
BASE = ""


def out(line):
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


def api(method, path, body=None, auth=None):
    headers = {"Content-Type": "application/json"}
    if auth is not None:
        headers["X-User-Id"], headers["X-Auth-Token"] = auth
    data = None if body is None else json.dumps(body).encode()
    request = urllib.request.Request(BASE + path, data=data, method=method, headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            raw = response.read()
            return response.status, (json.loads(raw) if raw else None)
    except urllib.error.HTTPError as error:
        try:
            return error.code, json.loads(error.read())
        except ValueError:
            return error.code, None
    except (urllib.error.URLError, OSError):
        return 0, None


def rest_login(user, password):
    status, reply = api("POST", "/api/v1/login", {"user": user, "password": password})
    data = (reply or {}).get("data") or {}
    if status != 200 or "authToken" not in data:
        fail("REST login did not succeed")
    SECRETS.add(data["authToken"])
    return data["userId"], data["authToken"]


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
        header, _, rest = head.partition(b"\r\n\r\n")
        self.status = int(header.split(b" ", 2)[1])
        self.buf = rest
        self.pings_seen = 0
        self.ddp_pings = []

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
        """Next JSON text message, or None on timeout. Answers WebSocket pings itself."""
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
                self.pings_seen += 1
                self.send_frame(10, payload)
                continue
            if opcode == 8:
                raise ConnectionError("server closed")
            if opcode in (1, 0):
                message += payload
                if b0 & 0x80:
                    decoded = json.loads(message)
                    if decoded.get("msg") == "ping":
                        self.ddp_pings.append(time.time())
                        reply = {"msg": "pong"}
                        if "id" in decoded:
                            reply["id"] = decoded["id"]
                        self.send_json(reply)
                        message = b""
                        continue
                    return decoded

    def drain(self, seconds):
        """Collect every JSON message that arrives within `seconds`."""
        found, end = [], time.time() + seconds
        while time.time() < end:
            msg = self.recv_json(max(0.05, end - time.time()))
            if msg is not None:
                found.append(msg)
        return found


# ---------------------------------------------------------------- helpers


def shape(value):
    """Top-level key names only, never values."""
    if isinstance(value, dict):
        return ",".join(sorted(value.keys())) or "none"
    return type(value).__name__


def ddp_call(ws, method, params, call_id, timeout=10):
    """Send a DDP method call; return (result_or_None, error_or_None, other_messages)."""
    ws.send_json({"msg": "method", "method": method, "params": params, "id": call_id})
    others, end = [], time.time() + timeout
    while time.time() < end:
        msg = ws.recv_json(max(0.05, end - time.time()))
        if msg is None:
            continue
        if msg.get("msg") == "result" and msg.get("id") == call_id:
            return msg.get("result"), msg.get("error"), others
        others.append(msg)
    return None, {"error": "timeout"}, others


def sub(ws, name, params, sub_id, timeout=10):
    ws.send_json({"msg": "sub", "id": sub_id, "name": name, "params": params})
    seen, end = [], time.time() + timeout
    while time.time() < end:
        msg = ws.recv_json(max(0.05, end - time.time()))
        if msg is None:
            continue
        if msg.get("msg") in ("ready", "nosub") and (
            sub_id in msg.get("subs", []) or msg.get("id") == sub_id
        ):
            return msg["msg"], seen
        seen.append(msg)
    return "timeout", seen


def describe_event(msg):
    """One safe line for a `changed` message: collection, eventName, arg shapes. No values."""
    fields = msg.get("fields") or {}
    args = fields.get("args") or []
    first = args[0] if args else None
    detail = shape(first) if isinstance(first, dict) else (
        first if isinstance(first, str) and len(first) < 24 and first.isalpha() else type(first).__name__
    )
    return "%s event=%s nargs=%d arg0=[%s]" % (
        msg.get("collection"),
        "ok" if fields.get("eventName") else "none",
        len(args),
        detail,
    )


def events_of(msgs):
    return [m for m in msgs if m.get("msg") == "changed"]


def main():
    global BASE
    if not os.path.isdir(STATE_DIR):
        fail("no local stack; run `python3 -I dev/seed.py up` and `seed` first")
    app = read_env(APP_ENV)
    admin = read_env(ADMIN_ENV)
    BASE = app["ROCKETTUI_SERVER_URL"].rstrip("/")
    parsed = urllib.parse.urlparse(BASE)
    if parsed.hostname not in LOOPBACK:
        fail("server is not loopback; this probe never talks to anything else")
    dev = (app["ROCKETTUI_USER_ID"], app["ROCKETTUI_TOKEN"])
    SECRETS.update(dev)
    SECRETS.update(v for k, v in admin.items() if k.endswith("PASS"))
    alice = rest_login("alice", admin["ALICE_PASS"])
    uid = dev[0]

    status, info = api("GET", "/api/info")
    out("D-info: GET /api/info status=%d version_key=%s" % (status, "yes" if (info or {}).get("version") else "no"))

    # Room ids (REST, dev's PAT).
    rooms = {}
    for label, path, query in (
        ("public", "/api/v1/channels.info", "roomName=rt-public"),
        ("private", "/api/v1/groups.info", "roomName=rt-private"),
    ):
        st, body = api("GET", path + "?" + query, auth=dev)
        rooms[label] = ((body or {}).get("channel") or (body or {}).get("group") or {}).get("_id")
    st, body = api("POST", "/api/v1/im.create", {"username": "alice"}, auth=dev)
    rooms["dm"] = ((body or {}).get("room") or {}).get("_id")
    if not all(rooms.values()):
        fail("could not resolve seeded room ids")

    # D0: upgrade and DDP handshake.
    ws = WebSocket(parsed.hostname, parsed.port or 80, "/websocket")
    out("D0a: websocket upgrade status=%d" % ws.status)
    first = ws.recv_json(5)
    out("D0b: first server frame keys=[%s]" % (shape(first) if first else "none"))
    ws.send_json({"msg": "connect", "version": "1", "support": ["1", "pre2", "pre1"]})
    reply = ws.recv_json(5)
    out("D0c: connect reply msg=%s keys=[%s]" % ((reply or {}).get("msg"), shape(reply)))
    if (reply or {}).get("msg") != "connected":
        fail("DDP connect did not answer `connected`")

    # D1: login with the PAT as `resume`.
    result, error, _ = ddp_call(ws, "login", [{"resume": dev[1]}], "1")
    pat_ok = result is not None and error is None
    out("D1a: login {resume: PAT} ok=%s result_keys=[%s] error_keys=[%s]" % (
        "yes" if pat_ok else "no", shape(result) if result else "none", shape(error) if error else "none"))
    if pat_ok:
        out("D1b: login result id matches own user id=%s" % ("yes" if result.get("id") == uid else "no"))
    if not pat_ok:
        # Fallback: a resume token from the REST password login.
        tok = rest_login("dev", admin["DEV_PASS"])
        ws2 = ws
        result, error, _ = ddp_call(ws2, "login", [{"resume": tok[1]}], "2")
        out("D1c: login {resume: REST login token} ok=%s" % ("yes" if result and not error else "no"))
        if not (result and not error):
            fail("DDP login failed with both the PAT and a REST login token")
        uid = tok[0]

    # D2/D3: subscriptions.
    state, early = sub(ws, "stream-room-messages", ["__my_messages__", False], "s-msgs")
    out("D2a: sub stream-room-messages __my_messages__ -> %s" % state)
    state, _ = sub(ws, "stream-notify-user", ["%s/subscriptions-changed" % uid, False], "s-subs")
    out("D3a: sub subscriptions-changed -> %s" % state)
    state, _ = sub(ws, "stream-notify-user", ["%s/rooms-changed" % uid, False], "s-rooms")
    out("D3b: sub rooms-changed -> %s" % state)
    state, _ = sub(ws, "stream-notify-room", ["%s/deleteMessage" % rooms["public"], False], "s-del")
    out("D4a: sub stream-notify-room <rid>/deleteMessage (public) -> %s" % state)
    for label in ("private", "dm"):
        state, _ = sub(ws, "stream-notify-room", ["%s/deleteMessage" % rooms[label], False], "s-del-" + label)
        out("D4a: sub stream-notify-room <rid>/deleteMessage (%s) -> %s" % (label, state))
    ws.drain(1)

    # Alice posts in the public channel, the private group and the DM: do they all arrive?
    for label in ("public", "private", "dm"):
        marker = "probe-%s" % secrets.token_hex(4)
        api("POST", "/api/v1/chat.postMessage", {"roomId": rooms[label], "text": marker}, alice)
        got = ws.drain(2.0)
        evs = events_of(got)
        out("D2b[%s]: events=%d [%s]" % (label, len(evs), " | ".join(sorted({describe_event(e) for e in evs}))))
        for e in evs:
            args = (e.get("fields") or {}).get("args") or []
            if e.get("collection") == "stream-room-messages" and args and isinstance(args[0], dict):
                m = args[0]
                out("D2c[%s]: message keys=[%s] rid_matches=%s" % (label, shape(m), "yes" if m.get("rid") == rooms[label] else "no"))
                break
        for e in evs:
            args = (e.get("fields") or {}).get("args") or []
            if e.get("collection") == "stream-notify-user" and len(args) >= 2 and isinstance(args[1], dict):
                s = args[1]
                out("D3c[%s]: notify-user action=%s doc_has alert=%s unread=%s" % (
                    label, args[0] if isinstance(args[0], str) and args[0].isalpha() else "?",
                    "alert" in s, "unread" in s))

    # D4: edit then delete one of alice's messages, in each room type (a public channel counts only
    # mentions as unread, a private group and a DM carry alert/unread on every message).
    for label in ("public", "private", "dm"):
        marker = "probe-edit-%s" % secrets.token_hex(4)
        st, body = api("POST", "/api/v1/chat.postMessage", {"roomId": rooms[label], "text": marker}, alice)
        mid = ((body or {}).get("message") or {}).get("_id")
        ws.drain(1.5)
        api("POST", "/api/v1/chat.update", {"roomId": rooms[label], "msgId": mid, "text": marker + "-edited"}, alice)
        evs = events_of(ws.drain(2.0))
        out("D4b[%s]: after edit events=%d [%s]" % (label, len(evs), " | ".join(sorted({describe_event(e) for e in evs}))))
        reach_edit = False
        for e in evs:
            args = (e.get("fields") or {}).get("args") or []
            if args and isinstance(args[0], dict) and args[0].get("_id") == mid:
                reach_edit = True
                out("D4c[%s]: edit event same _id=yes editedAt_present=%s keys=[%s]" % (label, "editedAt" in args[0], shape(args[0])))
                break
        # D4f: does the subscriptions-changed update that follows an edit carry alert/unread (key presence only)?
        subs_after_edit = [
            e for e in evs if e.get("collection") == "stream-notify-user"
            and len((e.get("fields") or {}).get("args") or []) >= 2
            and isinstance(e["fields"]["args"][1], dict)
        ]
        out("D4f[%s]: edit reaches own account=%s subscriptions-changed updates=%d carrying alert=%s unread=%s" % (
            label, "yes" if reach_edit else "no", len(subs_after_edit),
            any("alert" in e["fields"]["args"][1] for e in subs_after_edit),
            any("unread" in e["fields"]["args"][1] for e in subs_after_edit)))
        api("POST", "/api/v1/chat.delete", {"roomId": rooms[label], "msgId": mid}, alice)
        evs = events_of(ws.drain(2.0))
        out("D4d[%s]: after delete events=%d [%s]" % (label, len(evs), " | ".join(sorted({describe_event(e) for e in evs}))))
        out("D4g[%s]: delete reaches own account=%s" % (
            label, "yes" if any(e.get("collection") == "stream-notify-room" for e in evs) else "no"))
        for e in evs:
            args = (e.get("fields") or {}).get("args") or []
            if e.get("collection") == "stream-notify-room" and args and isinstance(args[0], dict):
                out("D4e[%s]: deleteMessage arg keys=[%s] has_same_id=%s" % (label, shape(args[0]), args[0].get("_id") == mid))

    # D5: heartbeat. Client ping must be answered with pong; note server pings seen so far.
    ws.send_json({"msg": "ping", "id": "p1"})
    pong = None
    end = time.time() + 5
    while time.time() < end:
        m = ws.recv_json(1)
        if m and m.get("msg") == "pong":
            pong = m
            break
    out("D5a: client ping answered pong=%s echoed_id=%s" % ("yes" if pong else "no", "yes" if pong and pong.get("id") == "p1" else "no"))
    ws.send_json({"msg": "pong"})
    out("D5b: websocket-level pings seen so far=%d" % ws.pings_seen)
    # Wait for a server DDP ping (interval is not documented; the drain is bounded).
    waited = 90
    start = time.time()
    try:
        ws.drain(waited)
        alive = "yes"
    except ConnectionError:
        alive = "no"
    gaps = [round(b - a) for a, b in zip([start] + ws.ddp_pings, ws.ddp_pings)]
    out("D5c: idle %ds, answered server DDP pings=%d gaps_s=%s connection_alive=%s" % (
        waited, len(ws.ddp_pings), gaps, alive))

    out("PROBE OK")


if __name__ == "__main__":
    main()
