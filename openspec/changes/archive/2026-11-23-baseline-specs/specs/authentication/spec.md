# Spec Delta

## Purpose

Defines how rockettui authenticates to a Rocket.Chat server with a Personal Access Token and keeps credentials and message content out of logs and output.

## ADDED Requirements

### Requirement: PAT credentials
The app SHALL authenticate with a user id and a Personal Access Token, taken from `ROCKETTUI_USER_ID` and `ROCKETTUI_TOKEN` or from `user_id` and `token` in `config.toml`. Environment variables SHALL win over the file, key by key. The server URL SHALL come from `server_url` or `ROCKETTUI_SERVER_URL`.

#### Scenario: Env wins
- **WHEN** both the env var and the file provide a token
- **THEN** the env var value is used

### Requirement: Missing credentials
Missing or incomplete server URL, user id or token SHALL be reported on the config error screen, naming exactly what is missing and where to set it. There SHALL be no fake-data fallback.

#### Scenario: No token
- **WHEN** no token is configured
- **THEN** the error screen names the token and both places to set it

### Requirement: Server URL validation
The server URL MUST be an http or https URL with a host, and MUST NOT contain credentials, a query or a fragment. `http://` SHALL be allowed, and a cleartext `http` URL to a non-loopback host (anything other than `localhost`, 127.0.0.0/8 or ::1) SHALL produce a persistent warning.

#### Scenario: Cleartext remote
- **WHEN** `server_url` is `http://chat.example.com`
- **THEN** a warning is shown on every frame and repeated on stderr after exit

### Requirement: File permission warning
When the token comes from `config.toml` and the file is readable by group or others, the app SHALL still start but show a persistent warning and repeat it on stderr after exit. No warning SHALL be shown when the token comes from the environment.

#### Scenario: World-readable config
- **WHEN** the token is read from a file with mode 644
- **THEN** a chmod 600 warning is shown

### Requirement: Runtime connection errors
Connection failures (unauthorized, unreachable, TLS, timeout) SHALL show a short actionable status-bar error while the app stays usable. `:reconnect` (alias `:retry`) SHALL reload rooms and history. Server redirects SHALL NOT be followed and certificates SHALL be verified normally.

#### Scenario: Bad token
- **WHEN** the server answers 401
- **THEN** the status bar shows an authentication error and `:q` still works

### Requirement: No secret or content leakage
The token and message contents SHALL never appear in logs, `Debug` or `Display` output, or error strings. Errors SHALL be typed and generic.

#### Scenario: Debug output
- **WHEN** config, credentials, messages or errors are formatted for debugging
- **THEN** the token and message text are not present
