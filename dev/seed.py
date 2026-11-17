#!/usr/bin/env python3
"""Throwaway local Rocket.Chat fixture for rockettui (DLVR-01).

Usage: python3 -I dev/seed.py <up|seed|check|capture|down>

Standard library only, same style as scripts/smoke_pty.py. This starts a disposable Rocket.Chat
on 127.0.0.1 through dev/compose.yml, seeds it with the data Phase 3 must handle, creates a
Personal Access Token for the app user and captures scrubbed REST responses as test fixtures.

It is a local fixture only and must never target the company server: the company policy on
third-party clients and PATs is unconfirmed (CLAUDE.md Policy). Everything here talks to
127.0.0.1.

Secrets (throwaway passwords, the PAT) live only in dev/state/<project>/ files created with
mode 0600 inside a 0700 directory, ignored by git through `/dev/state/`. They are never printed,
never put on a command line and never written to a fixture.

Settings from the environment: RT_PROJECT (default rockettui-dev), RT_RELEASE (default 8.9.1),
RT_HOST_PORT (default 3100).
"""

import json
import os
import secrets
import subprocess
import sys
import time
import urllib.error
import urllib.request

PROJECT = os.environ.get("RT_PROJECT", "rockettui-dev")
RELEASE = os.environ.get("RT_RELEASE", "8.9.1")
HOST_PORT = os.environ.get("RT_HOST_PORT", "3100")

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
COMPOSE = os.path.join(REPO, "dev", "compose.yml")
STATE_DIR = os.path.join(REPO, "dev", "state", PROJECT)
APP_ENV = os.path.join(STATE_DIR, ".env.local")
ADMIN_ENV = os.path.join(STATE_DIR, ".env.admin.local")
FIXTURES = os.path.join(REPO, "src", "api", "fixtures", "docker")

BASE = "http://127.0.0.1:" + HOST_PORT
USAGE = "usage: python3 -I dev/seed.py <up|seed|check|capture|down>"


class Fail(Exception):
    pass


def assert_no_secrets(text, secret_values):
    """Raise if any secret value occurs in text. Used before anything is written to a fixture."""
    for value in secret_values:
        if value and value in text:
            raise Fail("a secret value occurs in the text; refusing to write")


def api(method, path, body=None, auth=None):
    """JSON request against the local server. Errors carry method, path and status only."""
    headers = {"Content-Type": "application/json"}
    if auth is not None:
        headers["X-User-Id"] = auth[0]
        headers["X-Auth-Token"] = auth[1]
    data = None if body is None else json.dumps(body).encode()
    request = urllib.request.Request(BASE + path, data=data, method=method, headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            raw = response.read()
    except urllib.error.HTTPError as error:
        raise Fail("%s %s -> HTTP %d" % (method, path, error.code)) from None
    except (urllib.error.URLError, OSError):
        raise Fail("%s %s -> connection error" % (method, path)) from None
    return json.loads(raw) if raw else {}


def write_secret(path, text):
    """Write a file with mode 0600, whether or not it existed before."""
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w") as handle:
        handle.write(text)
    os.chmod(path, 0o600)


def ensure_state_dir():
    os.makedirs(STATE_DIR, mode=0o700, exist_ok=True)
    os.chmod(STATE_DIR, 0o700)


def read_env(path):
    values = {}
    with open(path) as handle:
        for line in handle:
            line = line.strip()
            if line and not line.startswith("#") and "=" in line:
                key, _, value = line.partition("=")
                values[key] = value
    return values


def write_env(path, values):
    write_secret(path, "".join("%s=%s\n" % (key, value) for key, value in values.items()))


def compose_env(admin_pass):
    env = dict(os.environ)
    env["RELEASE"] = RELEASE
    env["HOST_PORT"] = HOST_PORT
    env["ADMIN_PASS"] = admin_pass
    return env


def compose(args, admin_pass, **kwargs):
    command = ["docker", "compose", "-f", COMPOSE, "-p", PROJECT] + args
    return subprocess.run(command, env=compose_env(admin_pass), check=False, **kwargs)


def admin_pass_from_state():
    try:
        return read_env(ADMIN_ENV)["ADMIN_PASS"]
    except (OSError, KeyError):
        raise Fail("no throwaway passwords found; run `up` first") from None


def login(user, password):
    reply = api("POST", "/api/v1/login", {"user": user, "password": password})
    data = reply.get("data") or {}
    return data["userId"], data["authToken"]


def cmd_up():
    ensure_state_dir()
    if not os.path.exists(ADMIN_ENV):
        write_env(
            ADMIN_ENV,
            {
                "ADMIN_PASS": secrets.token_urlsafe(18),
                "DEV_PASS": secrets.token_urlsafe(18),
                "ALICE_PASS": secrets.token_urlsafe(18),
                "BOB_PASS": secrets.token_urlsafe(18),
            },
        )
    admin_pass = admin_pass_from_state()
    result = compose(["up", "-d"], admin_pass)
    if result.returncode != 0:
        raise Fail("docker compose up failed")
    deadline = time.time() + 300
    while True:
        try:
            info = api("GET", "/api/info")
            version = info.get("version", "unknown")
            break
        except Fail:
            if time.time() > deadline:
                raise Fail("server did not answer GET /api/info within 300 s") from None
            time.sleep(3)
    print("server up: %s version %s on 127.0.0.1:%s" % (PROJECT, version, HOST_PORT))


def post(auth, room_id, text, **extra):
    body = {"roomId": room_id, "text": text}
    body.update(extra)
    api("POST", "/api/v1/chat.postMessage", body, auth)


def mongosh(js, admin_pass):
    """Run one mongosh expression in the stack's MongoDB; returns stdout (no secrets in it)."""
    result = compose(
        ["exec", "-T", "mongodb", "mongosh", "--quiet", "rocketchat", "--eval", js],
        admin_pass,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise Fail("mongosh failed")
    return result.stdout


def matched_one(output):
    try:
        return json.loads(output.strip().splitlines()[-1]).get("matchedCount") == 1
    except (ValueError, IndexError, AttributeError):
        return False


def cmd_seed():
    passwords = read_env(ADMIN_ENV)
    if "SEEDED" in passwords:
        raise Fail("already seeded; re-seeding means `down` then `up`")
    admin_pass = passwords["ADMIN_PASS"]
    admin = None
    for _ in range(20):
        try:
            admin = login("admin", admin_pass)
            break
        except Fail:
            time.sleep(3)
    if admin is None:
        raise Fail("admin login did not succeed")
    print("admin logged in")

    for name, key in (("dev", "DEV_PASS"), ("alice", "ALICE_PASS"), ("bob", "BOB_PASS")):
        api(
            "POST",
            "/api/v1/users.create",
            {
                "email": "%s@example.invalid" % name,
                "name": name.capitalize(),
                "password": passwords[key],
                "username": name,
                "verified": True,
            },
            admin,
        )
        print("user created: %s" % name)
    dev = login("dev", passwords["DEV_PASS"])
    alice = login("alice", passwords["ALICE_PASS"])

    # rt-public: paging beyond one page and beyond the server's 100 cap, a join system message
    # (alice's `uj`), one attachment, and unread messages for dev (alice posts last).
    public = api("POST", "/api/v1/channels.create", {"name": "rt-public"}, dev)["channel"]["_id"]
    post(dev, public, "seed message 000")
    api("POST", "/api/v1/channels.join", {"roomId": public}, alice)
    discussion = None
    for n in range(1, 130):
        author = dev if n % 2 == 0 else alice
        if n == 127:
            # Created late so its `discussion-created` system message sits in the newest page
            # and alice still posts last (rt-public stays unread for dev).
            discussion = api(
                "POST",
                "/api/v1/rooms.createDiscussion",
                {"prid": public, "t_name": "rt-discussion"},
                dev,
            )["discussion"]["_id"]
        if n == 125:
            post(
                author,
                public,
                "seed message %03d" % n,
                attachments=[{"title": "report.pdf", "text": "quarterly report"}],
            )
        else:
            post(author, public, "seed message %03d" % n)
    print("channel seeded: rt-public (130 messages, join, attachment)")

    # rt-private: favorited by dev.
    private = api(
        "POST", "/api/v1/groups.create", {"name": "rt-private", "members": ["alice"]}, dev
    )["group"]["_id"]
    post(dev, private, "private seed message 1")
    post(alice, private, "private seed message 2")
    post(dev, private, "private seed message 3")
    api("POST", "/api/v1/rooms.favorite", {"roomId": private, "favorite": True}, dev)
    print("group seeded: rt-private (favorite)")

    # DM with alice, two unread messages from alice.
    dm_alice = api("POST", "/api/v1/im.create", {"username": "alice"}, dev)["room"]["_id"]
    post(dev, dm_alice, "dm seed message 1")
    post(alice, dm_alice, "dm seed message 2")
    post(alice, dm_alice, "dm seed message 3")
    print("dm seeded: alice (unread)")

    # DM with bob, closed by dev (open=false, skipped by the app, D-09).
    dm_bob = api("POST", "/api/v1/im.create", {"username": "bob"}, dev)["room"]["_id"]
    post(dev, dm_bob, "dm seed message 1")
    api("POST", "/api/v1/im.close", {"roomId": dm_bob}, dev)
    print("dm seeded: bob (closed)")

    # secrets: an encrypted group, flagged directly in MongoDB (RESEARCH step 4).
    secret = api("POST", "/api/v1/groups.create", {"name": "secrets"}, dev)["group"]["_id"]
    post(dev, secret, "secret seed message 1")
    out = mongosh(
        'JSON.stringify(db.rocketchat_room.updateOne({name:"secrets"},{$set:{encrypted:true}}))',
        admin_pass,
    )
    if not matched_one(out):
        raise Fail("could not mark the secrets group encrypted")
    print("group seeded: secrets (encrypted)")

    # rt-discussion under rt-public (created inside the rt-public loop above).
    post(dev, discussion, "discussion seed message 1")
    print("discussion seeded: rt-discussion")

    # rt-team and its main room.
    team = api("POST", "/api/v1/teams.create", {"name": "rt-team", "type": 0}, dev)["team"]
    team_room = team["roomId"]
    post(dev, team_room, "team seed message 1")
    print("team seeded: rt-team")

    # Custom sidebar category, best effort and license free (RESEARCH D-10 findings).
    try:
        api(
            "POST",
            "/api/v1/users.setPreferences",
            {
                "userId": dev[0],
                "data": {"sidebarCategories": [{"_id": "cat-work", "name": "Work"}]},
            },
            dev,
        )
        js = (
            "JSON.stringify(db.rocketchat_subscription.updateOne("
            '{rid:"%s","u._id":"%s"},{$set:{category:"cat-work"}}))' % (team_room, dev[0])
        )
        if not matched_one(mongosh(js, admin_pass)):
            raise Fail("subscription not found")
        print("category seeded: Work holds rt-team")
    except Fail as error:
        print("categories skipped: %s" % error)

    pat = api(
        "POST",
        "/api/v1/users.generatePersonalAccessToken",
        {"tokenName": "rockettui-%d" % int(time.time()), "bypassTwoFactor": True},
        dev,
    )["token"]
    write_env(
        APP_ENV,
        {
            "ROCKETTUI_SERVER_URL": BASE,
            "ROCKETTUI_USER_ID": dev[0],
            "ROCKETTUI_TOKEN": pat,
        },
    )
    print("personal access token created")

    passwords["SEEDED"] = "1"
    write_env(ADMIN_ENV, passwords)
    print("seeded: %s" % PROJECT)


def git_ignored(path):
    return (
        subprocess.run(
            ["git", "-C", REPO, "check-ignore", "-q", path],
            check=False,
        ).returncode
        == 0
    )


def cmd_check():
    for path in (APP_ENV, ADMIN_ENV):
        name = os.path.relpath(path, REPO)
        if not os.path.exists(path):
            raise Fail("missing state file %s" % name)
        if not git_ignored(path):
            raise Fail("%s is not ignored by git" % name)
        mode = os.stat(path).st_mode & 0o777
        if mode != 0o600:
            raise Fail("%s has mode %o, expected 600" % (name, mode))
    dir_mode = os.stat(STATE_DIR).st_mode & 0o777
    if dir_mode != 0o700:
        raise Fail("state directory has mode %o, expected 700" % dir_mode)
    app = read_env(APP_ENV)
    auth = (app["ROCKETTUI_USER_ID"], app["ROCKETTUI_TOKEN"])
    reply = api("GET", "/api/v1/subscriptions.get", auth=auth)
    # A discussion's `name` is its room id; its readable title is `fname`.
    names = sorted(
        (sub.get("fname") if sub.get("prid") else None) or sub.get("name") or sub.get("fname") or "?"
        for sub in reply.get("update", [])
    )
    print("ok: %d subscriptions: %s" % (len(names), ", ".join(names)))


def cmd_down():
    try:
        admin_pass = admin_pass_from_state()
    except Fail:
        admin_pass = "unused"
    compose(["down", "-v"], admin_pass)
    if os.path.isdir(STATE_DIR):
        for name in os.listdir(STATE_DIR):
            os.remove(os.path.join(STATE_DIR, name))
        os.rmdir(STATE_DIR)
    print("down: %s" % PROJECT)


def scrub(value):
    """Drop lastMessage (carries message text) and md (parsed markdown) everywhere."""
    if isinstance(value, dict):
        return {k: scrub(v) for k, v in value.items() if k not in ("lastMessage", "md")}
    if isinstance(value, list):
        return [scrub(item) for item in value]
    return value


def cmd_capture():
    app = read_env(APP_ENV)
    passwords = read_env(ADMIN_ENV)
    auth = (app["ROCKETTUI_USER_ID"], app["ROCKETTUI_TOKEN"])
    # Fresh login tokens exist only so the secret check below can look for them.
    secret_values = [app["ROCKETTUI_TOKEN"]]
    secret_values += [v for k, v in passwords.items() if k.endswith("_PASS")]
    for user, key in (("dev", "DEV_PASS"), ("alice", "ALICE_PASS"), ("bob", "BOB_PASS")):
        secret_values.append(login(user, passwords[key])[1])

    subscriptions = api("GET", "/api/v1/subscriptions.get", auth=auth)
    by_name = {}
    for sub in subscriptions.get("update", []):
        by_name[sub.get("name")] = sub.get("rid")
    public = by_name.get("rt-public")
    dm_alice = by_name.get("alice")
    if not public or not dm_alice:
        raise Fail("rt-public or the alice DM is missing from subscriptions.get")

    documents = {
        "subscriptions": subscriptions,
        "rooms": api("GET", "/api/v1/rooms.get", auth=auth),
        "preferences": api("GET", "/api/v1/users.getPreferences", auth=auth),
        "history_rt_public": api(
            "GET", "/api/v1/channels.history?roomId=%s&count=50" % public, auth=auth
        ),
        "history_dm_alice": api(
            "GET", "/api/v1/im.history?roomId=%s&count=50" % dm_alice, auth=auth
        ),
    }
    texts = {
        name: json.dumps(scrub(doc), indent=2, sort_keys=True) + "\n"
        for name, doc in documents.items()
    }
    for text in texts.values():
        assert_no_secrets(text, secret_values)
    os.makedirs(FIXTURES, exist_ok=True)
    for name, text in texts.items():
        path = os.path.join(FIXTURES, name + ".json")
        with open(path, "w") as handle:
            handle.write(text)
        print("wrote %s" % os.path.relpath(path, REPO))


COMMANDS = {
    "up": cmd_up,
    "seed": cmd_seed,
    "check": cmd_check,
    "capture": cmd_capture,
    "down": cmd_down,
}


def main(argv):
    if len(argv) != 2 or argv[1] not in COMMANDS:
        print(USAGE)
        return 2
    try:
        COMMANDS[argv[1]]()
    except Fail as error:
        print("FAIL: %s" % error)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
