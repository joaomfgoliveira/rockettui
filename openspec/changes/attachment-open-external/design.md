# Design

## Context

See proposal.md for motivation. This change builds on `attachment-image-preview`:

- `Message.attachments: Vec<Attachment>` with `kind`, `name`, `path`, `mime`, `size`, `dims`, where only a `/file-upload/` path is kept as fetchable. This change adds video and audio fields to the wire and the mapping.
- Fetching is a normal-client request built from the configured base URL joined to the path, with the user's headers, never from a stored absolute URL (design D6 there).
- A disk cache directory already exists (`emoji_image::cache_dir`). Status notices exist for one-line results. Effects run async and report an `Action`.
- This is the first feature that starts another process. The config is the user's own, as with `cmd:` keybindings, but the file it opens is the server's.
- Elm/TEA: one `Action` enum, one reducer.

## Goals / Non-Goals

**Goals:**
- Video and audio can be watched or heard from the keyboard, with the user's own player.
- The risky parts (a server-chosen file, a process, a token) are contained and each has one place to check.

**Non-Goals:**
- Playing or decoding media in the terminal, streaming, seek or progress UI, a cache cleaner, opening documents or archives.

## Decisions

**1. A separate key, `go`, and a separate action.**
`gp` stays "show the image in the terminal" and `go` is "open it outside". They are different behaviors with different failure modes, and a user may want `go` for images too. Action name `open_attachment`, snake_case, in all presets.

**2. Download to the cache, then open the path.**
Passing the server's URL to the player would need the token on the command line or in the player's config, which would leak it into `ps` and shell history. Downloading first keeps the token inside the client. The cost is waiting for the whole file; the status line shows that it runs.

**3. The file name is made by the client.**
Cache path: `<cache>/rockettui/downloads/<hash of path>.<ext>`, with `ext` from a fixed table keyed by media type (png, jpg, gif, webp, mp4, webm, mov, ogg, mp3, m4a, wav, ...). A media type not in the table is refused, which also keeps scripts, `.desktop` files and archives from reaching an opener that might act on them. The server's file name is never part of a path.
Alternative: keep the original name for the player's window title. Rejected: path traversal and extension tricks; not worth it.

**4. The opener is an argument list, run directly.**
`[open]` holds arrays of strings. The process is started with `Command::new(program).args(rest).arg(file)`, no shell, with stdin, stdout and stderr set to null so a program cannot draw over the TUI. A terminal-based player (one that needs the terminal) is therefore unsupported and the docs say to pick a windowed one (`mpv`, `vlc`, `xdg-open`). Default is the platform opener.
Alternative: a single command string with a `{}` placeholder. Rejected: it needs shell parsing, which is the injection surface the list form removes.

**5. Spawn and forget, but reap.**
The child is started from an effect and waited on in a background task, so it does not become a zombie and an early non-zero exit (the program started but failed at once) can be reported as a notice. The client never blocks on it and never kills it on exit.

**6. Download limits.**
A fixed cap (proposed 200 MiB), checked against the reported `size` before the request and against received bytes while streaming to a temporary file in the same directory, then renamed on success so a partial file is never opened. An interrupted or over-cap download removes the temporary file. Timeouts follow the client's existing request settings, with no total timeout so a large video on a slow link can finish.

**7. One download per attachment at a time.**
The reducer keeps a set of attachment keys in flight, so a double `go` does nothing. A file already on disk is opened with no request.

**8. Notices carry no server text.**
Status notices use fixed wording ("downloading attachment", "could not start the program", "file too large"). The file name, the path and the program's output are not shown or logged, matching the rule for message contents.

## Risks / Trade-offs

- [A program opens a hostile file] -> Only image, video and audio types of a fixed list are opened, with client-built names; the player's own parsers are the user's choice, as when opening a downloaded file by hand.
- [A big download blocks the user's attention] -> It runs in the background and the TUI stays usable; the cap bounds it.
- [Downloads pile up on disk] -> Not solved here; a later cache-cleaning change can sweep both caches.
- [The opener config is wrong] -> The failure is a notice, not a crash, and validation catches empty or non-array values at startup.
- [Conflict with `attachment-image-preview` in the keymap spec] -> Archive that change first; this delta already includes `show_image` and `gp`.
