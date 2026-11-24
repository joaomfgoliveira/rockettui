## 1. Search direction

- [x] 1.1 Add `Search.seeking_newer`; make `on_page` ignore a newer wait and add `search::on_newer_page`, which jumps on a match, asks for the next page while the room is detached, and otherwise stops with "No newer match"
- [x] 1.2 Make `prev` start a newer wait in a detached room; share the forward request (`jump::request_newer`) between the cursor trigger and the search

## 2. Wiring and tests

- [x] 2.1 Call `search::on_newer_page` from `jump::on_newer_loaded` after a page is applied, and the existing `search::on_failed` from `on_newer_failed`
- [x] 2.2 Tests: match in the first page, match after several pages, newest reached without a match (cursor kept, status text), failed page, abandoned by room switch and by clearing the search, `n` ignored while waiting, a room that is not detached, and an older page never resolving a newer wait
- [x] 2.3 Run `cargo fmt`, `cargo clippy`, `cargo test`, and extend the `docker_search` smoke scenario: jump to the oldest result, `/` a message that exists only near the newest, `N` until it is found
