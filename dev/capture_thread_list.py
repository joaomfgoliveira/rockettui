#!/usr/bin/env python3
"""Capture chat.getThreadsList payloads from the local dev server (change add-thread-list).

Usage: python3 -I dev/capture_thread_list.py                          (from the repository root)
       RT_PROJECT=rockettui-dev9 python3 -I dev/capture_thread_list.py   (the 9.0.0 stack)

Standard library only. Runs against the throwaway local server that dev/seed.py started, and
reuses the helpers of dev/probe_edit.py. Run dev/capture_threads.py first is NOT required: this
script creates its own thread parents in `rt-public`.

What it does, as the seeded users `dev`, `alice` and `bob`:
  * posts three thread parents (one by alice, two by dev), replies to them, and makes `dev`
    follow only some (dev follows threads it replies to; alice's parent is replied to by bob);
  * records chat.getThreadsList with no type (all), following, unread, with `text`, with
    `count=1&offset=1`, with a text holding regex characters `(`, which may be rejected, and for a
    room with no threads (rt-private-empty is not assumed: the first room without threads is used).

Output files, per stack (suffix _9 for 9.0.0), in src/api/fixtures/docker/:
  thread_list_all, thread_list_following, thread_list_unread, thread_list_text,
  thread_list_page2, thread_list_badtext, thread_list_empty

Output rule: stdout carries only labels, status codes and counts. Fixtures are written only after
a check that no credential occurs in them. The server must be on loopback.
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


def post(auth, room, text, tmid=None):
    message = {"rid": room, "msg": text}
    if tmid:
        message["tmid"] = tmid
    status, parsed = pe.api("POST", "/api/v1/chat.sendMessage", {"message": message}, auth)
    pe.time.sleep(0.3)  # chat.sendMessage has its own five-per-second limiter
    sent = (parsed or {}).get("message") if isinstance(parsed, dict) else None
    mid = sent.get("_id") if isinstance(sent, dict) else None
    pe.out("post%s status: %d" % (" reply" if tmid else "", status))
    if status != 200 or not mid:
        pe.fail("setup could not post a message")
    return mid


def scrub(value):
    if isinstance(value, dict):
        return {k: scrub(v) for k, v in value.items() if k not in ("lastMessage", "md")}
    if isinstance(value, list):
        return [scrub(item) for item in value]
    return value


def write(name, document, secret_values):
    name = name + SUFFIX + ".json"
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
    secret_values = [dev[1], alice[1], bob[1], *passwords.values()]

    status, listing = pe.api("GET", "/api/v1/subscriptions.get", auth=dev)
    rooms = {s.get("name"): s["rid"] for s in (listing or {}).get("update", []) if s.get("name")}
    room = rooms.get("rt-public")
    if status != 200 or not room:
        pe.out("CAPTURE FAIL: rt-public not found")
        return 1
    pe.api("POST", "/api/v1/channels.join", {"roomId": room}, bob)

    # Parent A: by alice, bob replies (dev does not take part, so dev does not follow it).
    parent_a = post(alice, room, "threadlist capture alpha deploy plan")
    post(bob, room, "threadlist reply to alpha", tmid=parent_a)
    # Parent B: by dev, alice replies (dev follows its own thread).
    parent_b = post(dev, room, "threadlist capture beta (with parens) and [brackets]")
    post(alice, room, "threadlist reply to beta one", tmid=parent_b)
    post(bob, room, "threadlist reply to beta two", tmid=parent_b)
    # Parent C: by dev, no replies yet counted by others; one reply by dev itself.
    parent_c = post(dev, room, "threadlist capture gamma")
    post(dev, room, "threadlist reply to gamma", tmid=parent_c)

    def get(label, query, required=True):
        status, parsed = pe.api("GET", "/api/v1/chat.getThreadsList", auth=dev, query=query)
        pe.out("chat.getThreadsList %s: %d" % (label, status))
        if status != 200 and isinstance(parsed, dict):
            pe.out("  server error: %s" % str(parsed.get("error") or parsed.get("errorType"))[:200])
        if required and status != 200:
            pe.out("CAPTURE FAIL: %s answered %d" % (label, status))
            sys.exit(1)
        return parsed

    base = {"rid": room}
    docs = {
        "thread_list_all": get("no type (all)", {**base}),
        "thread_list_following": get("type=following", {**base, "type": "following"}),
        "thread_list_unread": get("type=unread", {**base, "type": "unread"}),
        "thread_list_text": get("text=deploy", {**base, "text": "deploy"}),
        "thread_list_page2": get("count=1 offset=1", {**base, "count": 1, "offset": 1}),
        # Not required to succeed: records what an unescaped special character does.
        "thread_list_badtext": get("text=( unescaped", {**base, "text": "("}, required=False),
    }

    empty_room = None
    for name, rid in rooms.items():
        if rid == room:
            continue
        status, parsed = pe.api("GET", "/api/v1/chat.getThreadsList", auth=dev, query={"rid": rid})
        if status == 200 and isinstance(parsed, dict) and not parsed.get("threads"):
            empty_room = parsed
            break
    if empty_room is not None:
        docs["thread_list_empty"] = empty_room
    else:
        pe.out("no empty room found: thread_list_empty not written")

    os.makedirs(FIXTURES, exist_ok=True)
    for name, document in docs.items():
        write(name, document, secret_values)
    pe.out("CAPTURE OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
