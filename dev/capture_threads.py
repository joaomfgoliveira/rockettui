#!/usr/bin/env python3
"""Capture real thread payloads from the local dev server (change add-thread-view).

Usage: python3 -I dev/capture_threads.py                          (from the repository root)
       RT_PROJECT=rockettui-dev9 python3 -I dev/capture_threads.py   (the 9.0.0 stack)

Standard library only. Runs against the throwaway local server that dev/seed.py started. It
reuses the helpers of dev/probe_edit.py (REST, login, a minimal websocket, the DDP stream).

What it does, in the `rt-public` room as the seeded users `dev`, `alice` and `bob`:
  * posts a plain message and a thread parent;
  * posts three replies to the parent (`tmid`): by alice, by bob, and one by dev with
    `tshow: true` ("also send to channel");
  * records the stream frames for all of them, the room history, `chat.getThreadMessages`
    (full, and paged with count=2 at offset 0 and 2), and `chat.getMessage` of the parent.

Output files (server order, NOT key-sorted: reply order is the point), per stack:
  src/api/fixtures/docker/thread_history[_9].json    channels.history of the five messages
  src/api/fixtures/docker/thread_messages[_9].json   chat.getThreadMessages, full
  src/api/fixtures/docker/thread_page1[_9].json      ... count=2 offset=0
  src/api/fixtures/docker/thread_page2[_9].json      ... count=2 offset=2
  src/api/fixtures/docker/thread_parent[_9].json     chat.getMessage of the parent
  src/api/fixtures/docker/thread_stream[_9].json     the stream `fields` of every event, in order

Output rule: stdout carries only probe ids, status codes and counts. Fixtures hold the seeded
usernames and synthetic text, and are written only after a check that no credential occurs in
them. The server must be on loopback.
"""

import importlib.util
import json
import os
import sys
import urllib.parse

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FIXTURES = os.path.join(REPO, "src", "api", "fixtures", "docker")

_spec = importlib.util.spec_from_file_location("probe_edit", os.path.join(REPO, "dev", "probe_edit.py"))
pe = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(pe)

SUFFIX = "_9" if pe.PROJECT.endswith("9") else ""


def post(auth, room, text, tmid=None, tshow=False):
    message = {"rid": room, "msg": text}
    if tmid:
        message["tmid"] = tmid
        if tshow:
            message["tshow"] = True
    status, parsed = pe.api("POST", "/api/v1/chat.sendMessage", {"message": message}, auth)
    pe.time.sleep(0.3)  # chat.sendMessage has its own five-per-second limiter
    sent = (parsed or {}).get("message") if isinstance(parsed, dict) else None
    mid = sent.get("_id") if isinstance(sent, dict) else None
    pe.out("post%s status: %d" % (" reply" if tmid else "", status))
    if status != 200 or not mid:
        pe.fail("setup could not post a message")
    return mid


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
    name = name.replace(".json", SUFFIX + ".json")
    text = json.dumps(scrub(document), indent=2) + "\n"
    for value in secret_values:
        if value and value in text:
            pe.out("CAPTURE FAIL: a credential occurs in %s; nothing written" % name)
            sys.exit(1)
    path = os.path.join(FIXTURES, name)
    with open(path, "w") as handle:
        handle.write(text)
    pe.out("wrote %s" % os.path.relpath(path, REPO))


def main():
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

    alice = pe.login("alice", passwords["alice"])
    bob = pe.login("bob", passwords["bob"])
    # Tokens and passwords only: a user id is not a secret and appears in every message (`u._id`).
    secret_values = [dev[1], alice[1], bob[1], *passwords.values()]

    status, listing = pe.api("GET", "/api/v1/subscriptions.get", auth=dev)
    room = next((s["rid"] for s in (listing or {}).get("update", []) if s.get("name") == "rt-public"), None)
    if status != 200 or not room:
        pe.out("CAPTURE FAIL: rt-public not found")
        return 1
    pe.api("POST", "/api/v1/channels.join", {"roomId": room}, bob)

    ws = pe.open_stream(dev, dev[0], [room])
    events = []
    plain = post(dev, room, "thread capture plain")
    parent = post(dev, room, "thread capture parent")
    events += ws.drain(1.0)
    r1 = post(alice, room, "thread capture reply one", tmid=parent)
    events += ws.drain(0.6)
    r2 = post(bob, room, "thread capture reply two", tmid=parent)
    events += ws.drain(0.6)
    r3 = post(dev, room, "thread capture reply three also in channel", tmid=parent, tshow=True)
    events += ws.drain(1.5)

    mids = {plain, parent, r1, r2, r3}
    kept = message_events(events, mids)
    pe.out("stream events for the five messages: %d" % len(kept))

    def get(path, query):
        status, parsed = pe.api("GET", path, auth=dev, query=query)
        pe.out("%s %s: %d" % (path, ",".join("%s=%s" % (k, "id" if k in ("tmid", "msgId", "roomId") else v)
                                             for k, v in query.items()), status))
        if status != 200:
            pe.out("CAPTURE FAIL: %s answered %d" % (path, status))
            sys.exit(1)
        return parsed

    history = get("/api/v1/channels.history", {"roomId": room, "count": 5})
    full = get("/api/v1/chat.getThreadMessages", {"tmid": parent})
    page1 = get("/api/v1/chat.getThreadMessages", {"tmid": parent, "count": 2, "offset": 0})
    page2 = get("/api/v1/chat.getThreadMessages", {"tmid": parent, "count": 2, "offset": 2})
    parent_doc = get("/api/v1/chat.getMessage", {"msgId": parent})

    os.makedirs(FIXTURES, exist_ok=True)
    write("thread_history.json", history, secret_values)
    write("thread_messages.json", full, secret_values)
    write("thread_page1.json", page1, secret_values)
    write("thread_page2.json", page2, secret_values)
    write("thread_parent.json", parent_doc, secret_values)
    write("thread_stream.json", kept, secret_values)
    pe.out("CAPTURE OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
