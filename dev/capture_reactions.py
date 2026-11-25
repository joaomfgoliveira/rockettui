#!/usr/bin/env python3
"""Capture real reaction payloads from the local dev server (change show-message-reactions).

Usage: python3 -I dev/capture_reactions.py        (from the repository root)

Standard library only. Runs against the throwaway local server that dev/seed.py started. It
reuses the helpers of dev/probe_edit.py (REST, login, a minimal websocket, the DDP stream).

What it does, in the `rt-public` room as the seeded users `dev`, `alice` and `bob`:
  * posts a message and reacts to it with `:+1:` (three users, in a known order), `:tada:`,
    a skin-tone shortcode, a custom emoji created for the run, and a name the server may reject;
  * posts a second message, reacts once and takes the reaction back (the "last reaction removed"
    shape);
  * records the stream frames for both messages and the room history that holds them.

With the argument `custom-emoji` it only writes `emoji_custom_list.json` (see `capture_custom_emoji`).

Output files (written in server order, NOT key-sorted: the order of `reactions` is the point):
  src/api/fixtures/docker/history_reactions.json   channels.history of the two messages
  src/api/fixtures/docker/stream_reactions.json    the stream `fields` of every event, in order

Output rule: stdout carries only probe ids, status codes and counts. Fixtures hold the seeded
usernames and synthetic text, and are written only after a check that no credential occurs in
them. The server must be on loopback. The custom emoji it creates is deleted in a `finally`.
"""

import importlib.util
import json
import os
import struct
import sys
import time
import urllib.parse
import uuid
import zlib

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FIXTURES = os.path.join(REPO, "src", "api", "fixtures", "docker")
CUSTOM = "rtpartyparrot"
UNKNOWN = "rt_no_such_emoji_zz"

_spec = importlib.util.spec_from_file_location("probe_edit", os.path.join(REPO, "dev", "probe_edit.py"))
pe = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(pe)


def png_1x1():
    """A valid 1x1 PNG, built by hand so no file or library is needed."""

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", 1, 1, 8, 6, 0, 0, 0)
    pixel = zlib.compress(b"\x00\xff\x00\x00\xff")
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", pixel) + chunk(b"IEND", b"")


def create_custom_emoji(admin, name=None, aliases=""):
    """POST emoji-custom.create as multipart. Returns the HTTP status."""
    name = name or CUSTOM
    boundary = uuid.uuid4().hex
    parts = []
    for field, value in (("name", name), ("aliases", aliases)):
        parts.append(
            ('--%s\r\nContent-Disposition: form-data; name="%s"\r\n\r\n%s\r\n' % (boundary, field, value)).encode()
        )
    parts.append(
        (
            '--%s\r\nContent-Disposition: form-data; name="emoji"; filename="%s.png"\r\n'
            "Content-Type: image/png\r\n\r\n" % (boundary, name)
        ).encode()
        + png_1x1()
        + b"\r\n"
    )
    parts.append(("--%s--\r\n" % boundary).encode())
    import urllib.error
    import urllib.request

    request = urllib.request.Request(
        pe.BASE + "/api/v1/emoji-custom.create",
        data=b"".join(parts),
        method="POST",
        headers={
            "Content-Type": "multipart/form-data; boundary=%s" % boundary,
            "X-User-Id": admin[0],
            "X-Auth-Token": admin[1],
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return response.status
    except urllib.error.HTTPError as error:
        return error.code
    except (urllib.error.URLError, OSError):
        return 0


def delete_custom_emoji(admin, name=None):
    name = name or CUSTOM
    status, listing = pe.api("GET", "/api/v1/emoji-custom.list", auth=admin)
    for emoji in ((listing or {}).get("emojis") or {}).get("update", []):
        if emoji.get("name") == name:
            pe.api("POST", "/api/v1/emoji-custom.delete", {"emojiId": emoji["_id"]}, admin)


def react(auth, mid, emoji, should_react=True):
    status, parsed = pe.api(
        "POST", "/api/v1/chat.react", {"messageId": mid, "emoji": emoji, "shouldReact": should_react}, auth
    )
    return status


def message_events(events, mids):
    """The `fields` of every stream `changed` event whose first argument is one of `mids`."""
    kept = []
    for event in events:
        if event.get("msg") != "changed":
            continue
        fields = event.get("fields") or {}
        args = fields.get("args") or []
        first = args[0] if args else None
        if isinstance(first, dict) and first.get("_id") in mids:
            kept.append(fields)
    return kept


def scrub(value):
    """Drop lastMessage and md (parsed markdown) everywhere, as dev/seed.py does."""
    if isinstance(value, dict):
        return {k: scrub(v) for k, v in value.items() if k not in ("lastMessage", "md")}
    if isinstance(value, list):
        return [scrub(item) for item in value]
    return value


def write(name, document, secret_values):
    text = json.dumps(scrub(document), indent=2) + "\n"
    for value in secret_values:
        if value and value in text:
            pe.out("CAPTURE FAIL: a credential occurs in %s; nothing written" % name)
            sys.exit(1)
    path = os.path.join(FIXTURES, name)
    with open(path, "w") as handle:
        handle.write(text)
    pe.out("wrote %s" % os.path.relpath(path, REPO))


def capture_custom_emoji():
    """The `custom-emoji` mode: one fixture, `emoji_custom_list.json`, and nothing else.

    Creates two throwaway custom emojis (one with aliases), reads `emoji-custom.list` as the app
    user (a normal account, like the client), writes the answer as it is, and deletes them again.
    """
    try:
        app = pe.read_env(pe.APP_ENV)
        env = pe.read_env(pe.ADMIN_ENV)
        pe.BASE = app["ROCKETTUI_SERVER_URL"].rstrip("/")
        dev = (app["ROCKETTUI_USER_ID"], app["ROCKETTUI_TOKEN"])
        admin_pass = env["ADMIN_PASS"]
    except (OSError, KeyError):
        print("CAPTURE FAIL: no dev state files; run dev/seed.py up and seed first")
        return 1
    pe.SECRETS.update(v for v in (dev[0], dev[1], admin_pass) if v)
    if urllib.parse.urlparse(pe.BASE).hostname not in pe.LOOPBACK:
        pe.out("CAPTURE FAIL: not a loopback server")
        return 1
    admin = pe.login("admin", admin_pass)
    secret_values = [dev[1], admin[1], admin_pass]
    made = (("rtshipit", "rtship,rt_ship"), ("rtplain", ""))
    try:
        for name, aliases in made:
            pe.out("custom emoji create %s: %d" % (name, create_custom_emoji(admin, name, aliases)))
        time.sleep(0.5)
        status, listing = pe.api("GET", "/api/v1/emoji-custom.list", auth=dev)
        pe.out("emoji-custom.list as the app user: %d" % status)
        if status != 200:
            return 1
        os.makedirs(FIXTURES, exist_ok=True)
        write("emoji_custom_list.json", listing, secret_values)
    finally:
        for name, _ in made:
            delete_custom_emoji(admin, name)
    return 0


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "custom-emoji":
        return capture_custom_emoji()
    try:
        app = pe.read_env(pe.APP_ENV)
        env = pe.read_env(pe.ADMIN_ENV)
        pe.BASE = app["ROCKETTUI_SERVER_URL"].rstrip("/")
        passwords = {"admin": env["ADMIN_PASS"], "alice": env["ALICE_PASS"], "bob": env["BOB_PASS"]}
        dev = (app["ROCKETTUI_USER_ID"], app["ROCKETTUI_TOKEN"])
    except (OSError, KeyError):
        print("CAPTURE FAIL: no dev state files; run dev/seed.py up and seed first")
        return 1
    pe.SECRETS.update(v for v in (dev[0], dev[1], *passwords.values()) if v)
    if urllib.parse.urlparse(pe.BASE).hostname not in pe.LOOPBACK:
        pe.out("CAPTURE FAIL: not a loopback server")
        return 1

    admin = pe.login("admin", passwords["admin"])
    alice = pe.login("alice", passwords["alice"])
    bob = pe.login("bob", passwords["bob"])
    # Tokens and passwords only: a user id is not a secret and appears in every message (`u._id`).
    secret_values = [dev[1], alice[1], bob[1], admin[1], *passwords.values()]

    status, listing = pe.api("GET", "/api/v1/subscriptions.get", auth=dev)
    room = next((s["rid"] for s in (listing or {}).get("update", []) if s.get("name") == "rt-public"), None)
    if status != 200 or not room:
        pe.out("CAPTURE FAIL: rt-public not found")
        return 1
    pe.api("POST", "/api/v1/channels.join", {"roomId": room}, bob)

    try:
        pe.out("custom emoji create: %d" % create_custom_emoji(admin))
        time.sleep(0.5)
        ws = pe.open_stream(dev, dev[0], [room])

        events = []
        first = pe.post_message(dev, room, "reaction capture one")
        second = pe.post_message(dev, room, "reaction capture two")
        events += ws.drain(1.0)

        # The first message: three users on :+1: in a fixed order, then the other shortcodes.
        for label, who, emoji in (
            ("+1 dev", dev, ":+1:"),
            ("+1 alice", alice, ":+1:"),
            ("+1 bob", bob, ":+1:"),
            ("tada alice", alice, ":tada:"),
            ("tone alice", alice, ":thumbsup_tone2:"),
            ("custom alice", alice, ":%s:" % CUSTOM),
            ("unknown alice", alice, ":%s:" % UNKNOWN),
        ):
            pe.out("react %s: %d" % (label, react(who, first, emoji)))
            events += ws.drain(0.4)
        # The second message: one reaction, taken back.
        pe.out("react one: %d" % react(alice, second, ":+1:"))
        events += ws.drain(0.4)
        pe.out("react undo: %d" % react(alice, second, ":+1:", False))
        events += ws.drain(1.5)

        kept = message_events(events, {first, second})
        pe.out("stream events for the two messages: %d" % len(kept))
        status, history = pe.api(
            "GET", "/api/v1/channels.history", auth=dev, query={"roomId": room, "count": 2}
        )
        if status != 200:
            pe.out("CAPTURE FAIL: history answered %d" % status)
            return 1
        os.makedirs(FIXTURES, exist_ok=True)
        write("history_reactions.json", history, secret_values)
        write("stream_reactions.json", kept, secret_values)
    finally:
        delete_custom_emoji(admin)
    return 0


if __name__ == "__main__":
    sys.exit(main())
