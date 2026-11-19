#!/usr/bin/env python3
"""Wave 0 live probe of the Rocket.Chat send contract for rockettui (Phase 4, MSG-02).

Usage: python3 -I dev/probe_send.py        (from the repository root)

Standard library only. Runs against the throwaway local server that dev/seed.py started
(RT_PROJECT, default rockettui-dev) and records how it answers chat.sendMessage with a
client-chosen _id (D-15), a repeated _id, chat.getMessage, the own-username call (D-17), a
read-only room, a muted user, an archived room, an oversize text and the endpoint D-15 replaced.

Output rule: every line goes through out(). A line holds only a probe id, an endpoint, a status
code, top-level key names, yes/no flags and closed error classes. It never holds a credential,
a message text, a response body, a header or the server's error text. out() refuses any line
that contains a secret value. The server must be on loopback: this script creates and deletes
rooms as admin and must never touch the company server (CLAUDE.md Policy).

The last line is `PROBE OK` only when P1 answered 200 with the client's _id and every room this
run created was deleted. It creates, mutes, archives and deletes only its own rt-probe-* rooms.
"""

import json
import os
import re
import secrets
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

SECRETS = set()
ID_COUNTER = [0]
DEFAULT_MAX_SIZE = 5000
ISO_MS = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$")
ISO = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})$")
ERROR_TYPE = re.compile(r"^[A-Za-z0-9_.-]{1,64}$")


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


def new_id():
    """The app's id shape (assumption A4): `rt` + 32 hex digits + a counter digit."""
    ID_COUNTER[0] += 1
    return "rt" + secrets.token_hex(16) + str(ID_COUNTER[0] % 10)


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
        ("readonly", ("readonly", "read-only", "read only")),
        ("muted", ("mute",)),
        ("archived", ("archived",)),
        ("size_exceeded", ("size-exceeded", "maxallowedsize", "size exceeded")),
        ("invalid_params", ("invalid-params", "must not have additional")),
        ("type_error", ("typeerror", "cannot read properties", "undefined")),
        ("not_allowed", ("not-allowed", "not allowed")),
        ("duplicate", ("already", "duplicate", "exists")),
    )
    for label, needles in rules:
        if any(needle in text for needle in needles):
            return label
    return "other"


def ts_shape(value):
    if value is None:
        return "absent"
    if isinstance(value, dict) and "$date" in value:
        return "ejson"
    if isinstance(value, str):
        if ISO_MS.match(value):
            return "iso_ms"
        if ISO.match(value):
            return "iso"
    return "other"


def send(auth, room, msg_id, text):
    body = {"message": {"_id": msg_id, "rid": room, "msg": text}}
    status, parsed = api("POST", "/api/v1/chat.sendMessage", body, auth)
    time.sleep(0.3)  # chat.sendMessage has its own five-per-second limiter
    return status, parsed


def line_send(probe, status, parsed, extra=""):
    out(
        "%s POST chat.sendMessage status=%d keys=%s error_type=%s error_class=%s%s"
        % (probe, status, keys(parsed), error_type(parsed), error_class(parsed), extra)
    )


def main():
    global BASE
    try:
        app = read_env(APP_ENV)
        admin = read_env(ADMIN_ENV)
        BASE = app["ROCKETTUI_SERVER_URL"].rstrip("/")
        dev = (app["ROCKETTUI_USER_ID"], app["ROCKETTUI_TOKEN"])
        admin_pass = admin["ADMIN_PASS"]
    except (OSError, KeyError):
        print("PROBE FAIL: no dev state files; run dev/seed.py up and seed first")
        return 1
    SECRETS.update(value for value in (dev[0], dev[1], admin_pass) if value)
    host = urllib.parse.urlparse(BASE).hostname
    if host not in ("127.0.0.1", "localhost"):
        out("PROBE FAIL: not a loopback server")
        return 1

    created = []
    admin_auth = None
    verdict = None
    try:
        admin_auth = login("admin", admin_pass)

        # P0: the own-username call (D-17, A6).
        status, me = api("GET", "/api/v1/me", auth=dev)
        username = me.get("username") if isinstance(me, dict) else None
        out(
            "P0 GET me status=%d keys=%s has_username=%s"
            % (status, keys(me), "yes" if isinstance(username, str) and username else "no")
        )
        if not isinstance(username, str) or not username:
            fail("P0 no username, cannot create the probe rooms")

        # Setup as admin: three probe rooms, all with the dev user as a member.
        def make_room(label, **extra):
            name = "rt-probe-%s%s" % (label, secrets.token_hex(3))
            body = {"name": name, "members": [username]}
            body.update(extra)
            st, reply = api("POST", "/api/v1/channels.create", body, admin_auth)
            time.sleep(0.3)
            rid = ((reply or {}).get("channel") or {}).get("_id")
            if st != 200 or not rid:
                fail("setup could not create a probe room")
            created.append(rid)
            return rid

        room = make_room("")
        ro_room = make_room("ro-", readOnly=True)
        ar_room = make_room("ar-")
        st, _ = api("POST", "/api/v1/channels.archive", {"roomId": ar_room}, admin_auth)
        if st != 200:
            fail("setup could not archive the probe room")
        out("setup rooms=%d archived=yes" % len(created))

        # P1: a send with a fresh client _id.
        id1 = new_id()
        status, parsed = send(dev, room, id1, "probe text one")
        message = parsed.get("message") if isinstance(parsed, dict) else None
        message = message if isinstance(message, dict) else {}
        user = message.get("u") if isinstance(message.get("u"), dict) else {}
        p1_ok = status == 200 and message.get("_id") == id1
        out(
            "P1 POST chat.sendMessage status=%d keys=%s message_keys=%s user_keys=%s "
            "id_match=%s ts=%s error_class=%s"
            % (
                status,
                keys(parsed),
                keys(message),
                keys(user),
                "yes" if message.get("_id") == id1 else "no",
                ts_shape(message.get("ts")),
                error_class(parsed),
            )
        )

        # P2: the identical body again (A1).
        status, parsed = send(dev, room, id1, "probe text one")
        line_send("P2", status, parsed)

        # P3a / P3b: lookup by id.
        status, parsed = api("GET", "/api/v1/chat.getMessage", auth=dev, query={"msgId": id1})
        got = parsed.get("message") if isinstance(parsed, dict) else None
        got = got if isinstance(got, dict) else {}
        out(
            "P3a GET chat.getMessage status=%d keys=%s id_match=%s"
            % (status, keys(parsed), "yes" if got.get("_id") == id1 else "no")
        )
        status, parsed = api("GET", "/api/v1/chat.getMessage", auth=dev, query={"msgId": new_id()})
        out(
            "P3b GET chat.getMessage status=%d keys=%s error_class=%s"
            % (status, keys(parsed), error_class(parsed))
        )

        # P4: read-only room.
        status, parsed = send(dev, ro_room, new_id(), "probe readonly")
        line_send("P4", status, parsed)

        # P5a: muted user (the mute call may differ per release, A7).
        muted = False
        mute_status, _ = api(
            "POST", "/api/v1/rooms.muteUser", {"roomId": room, "userId": dev[0]}, admin_auth
        )
        if mute_status != 200:
            call = {
                "message": json.dumps(
                    {
                        "msg": "method",
                        "id": "1",
                        "method": "muteUserInRoom",
                        "params": [{"rid": room, "username": username}],
                    }
                )
            }
            mute_status, _ = api("POST", "/api/v1/method.call/muteUserInRoom", call, admin_auth)
        if mute_status == 200:
            muted = True
            status, parsed = send(dev, room, new_id(), "probe muted")
            line_send("P5a", status, parsed)
        else:
            out("P5a SKIPPED mute_status=%d" % mute_status)

        # P5b: archived room.
        status, parsed = send(dev, ar_room, new_id(), "probe archived")
        line_send("P5b", status, parsed)

        # P6: oversize text.
        status, parsed = api(
            "GET", "/api/v1/settings/Message_MaxAllowedSize", auth=admin_auth
        )
        limit = (parsed or {}).get("value") if isinstance(parsed, dict) else None
        if isinstance(limit, int) and not isinstance(limit, bool) and limit > 0:
            out("P6 setting max_allowed_size=%d" % limit)
        else:
            limit = DEFAULT_MAX_SIZE
            out("P6 setting max_allowed_size=unknown")
        target = room
        if muted:
            target = make_room("big-")
        status, parsed = send(dev, target, new_id(), "x" * (limit + 1))
        line_send("P6", status, parsed, " size=limit_plus_one")

        # P7: the endpoint D-15 replaced, with a client _id.
        status, parsed = api(
            "POST",
            "/api/v1/chat.postMessage",
            {"roomId": room if not muted else target, "text": "probe replaced", "_id": new_id()},
            dev,
        )
        time.sleep(0.3)
        out(
            "P7 POST chat.postMessage status=%d keys=%s error_type=%s error_class=%s"
            % (status, keys(parsed), error_type(parsed), error_class(parsed))
        )
        verdict = p1_ok
    finally:
        deleted = 0
        total = len(created)
        if admin_auth is not None:
            for rid in created:
                st, _ = api("POST", "/api/v1/channels.delete", {"roomId": rid}, admin_auth)
                time.sleep(0.1)
                if st == 200:
                    deleted += 1
        out("cleanup deleted=%d/%d" % (deleted, total))
        cleaned = deleted == total

    if verdict is None:
        out("PROBE FAIL: probe did not complete")
        return 1
    if not verdict:
        out("PROBE FAIL: P1 did not return 200 with the client id")
        return 1
    if not cleaned:
        out("PROBE FAIL: cleanup did not delete every created room")
        return 1
    out("PROBE OK")
    return 0


BASE = ""

if __name__ == "__main__":
    sys.exit(main())
