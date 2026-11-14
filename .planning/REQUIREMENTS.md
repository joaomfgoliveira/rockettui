# Requirements: rockettui

**Defined:** 2026-11-14
**Core Value:** Reading and sending Rocket.Chat messages (with live incoming messages) entirely from the keyboard in a modal, vim-like interface.

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### Authentication

- [ ] **AUTH-01**: User can authenticate with a Personal Access Token (user id + token) from an env var or config file
- [ ] **AUTH-02**: App warns if a config file holding credentials is not `chmod 600`
- [ ] **AUTH-03**: Token and message contents are never written to logs

### Rooms

- [ ] **ROOM-01**: User can see a list of channels, private groups and DMs in the sidebar
- [ ] **ROOM-02**: User can select a room to open it
- [ ] **ROOM-03**: Encrypted (E2EE) rooms show as "encrypted, not supported"

### Messages

- [ ] **MSG-01**: User can view message history of the selected room and scroll it
- [ ] **MSG-02**: User can send a message to the selected room
- [ ] **MSG-03**: User sees new incoming messages in real time without refreshing (DDP over WebSocket)
- [ ] **MSG-04**: App reconnects to the realtime connection after a drop

### Navigation

- [ ] **NAV-01**: User can switch between Normal, Insert and Command modes, with the current mode shown
- [ ] **NAV-02**: In Normal mode, `hjkl`, `gg`/`G` and `Ctrl-d`/`Ctrl-u` move and scroll
- [ ] **NAV-03**: User can quit with `:q` in Command mode
- [ ] **NAV-04**: Layout is sidebar (rooms) + messages + input bar

### Configuration

- [ ] **CONF-01**: App loads `config.toml`, `keymap.toml` and `theme.toml` from `~/.config/rockettui/` (honoring `$XDG_CONFIG_HOME`)
- [ ] **CONF-02**: App runs with built-in defaults when any or all config files are absent
- [ ] **CONF-03**: Invalid config entries show a clear error instead of crashing
- [ ] **CONF-04**: User can pick the `vim` or `vim-extras` keymap preset in `config.toml`
- [ ] **CONF-05**: User can rebind keys per mode in `keymap.toml`, overriding the preset
- [ ] **CONF-06**: User can set colors and styles for every UI element in `theme.toml`
- [ ] **CONF-07**: User can set server URL, history size, timestamp format and sidebar width in `config.toml`

### Delivery

- [ ] **DLVR-01**: App works against a local Docker Rocket.Chat server
- [ ] **DLVR-02**: App works day to day against the company server

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Authentication

- **AUTH-04**: User can log in with username + password + OTP
- **AUTH-05**: Token is stored in the system keyring

### Messages

- **MSG-05**: Threads, reactions, edit/delete
- **MSG-06**: Mentions and unread badges
- **MSG-07**: Light markdown rendering

### Other

- **MISC-01**: Search (`/`)
- **MISC-02**: Desktop notifications
- **MISC-03**: Config hot reload
- **MISC-04**: Bundled theme gallery

## Out of Scope

| Feature | Reason |
|---------|--------|
| E2EE rooms | Complexity; shown as "encrypted, not supported" |
| Voice/video calls | Not needed for basic chat |
| File uploads | Not needed for basic chat |
| Admin features | Not a goal |
| Mouse-first UX | Keyboard-driven by design |
| Replacing the official client | Personal tool, not a product |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| NAV-01 | Phase 1 | Pending |
| NAV-02 | Phase 1 | Pending |
| NAV-03 | Phase 1 | Pending |
| NAV-04 | Phase 1 | Pending |
| CONF-01 | Phase 2 | Pending |
| CONF-02 | Phase 2 | Pending |
| CONF-03 | Phase 2 | Pending |
| CONF-04 | Phase 2 | Pending |
| CONF-05 | Phase 2 | Pending |
| CONF-06 | Phase 2 | Pending |
| CONF-07 | Phase 2 | Pending |
| AUTH-01 | Phase 3 | Pending |
| AUTH-02 | Phase 3 | Pending |
| AUTH-03 | Phase 3 | Pending |
| ROOM-01 | Phase 3 | Pending |
| ROOM-02 | Phase 3 | Pending |
| ROOM-03 | Phase 3 | Pending |
| MSG-01 | Phase 3 | Pending |
| DLVR-01 | Phase 3 | Pending |
| MSG-02 | Phase 4 | Pending |
| MSG-03 | Phase 5 | Pending |
| MSG-04 | Phase 5 | Pending |
| DLVR-02 | Phase 6 | Pending |

**Coverage:**
- v1 requirements: 22 total
- Mapped to phases: 22
- Unmapped: 0 ✓

---
*Requirements defined: 2026-11-14*
*Last updated: 2026-11-14 after initial definition*
