//! Connection settings: the server URL, user id and Personal Access Token (AUTH-01).
//!
//! This is part of the pure config core: the environment arrives as data (`ConnEnv`), nothing is
//! read here. The user id and token are held in `Secret` from the moment they are read, a wrapper
//! whose `Debug` prints no value and which has no `Display` (D-07). No message produced in this
//! module ever contains a value (T-03-01, T-03-03).

use crate::config::diag::{ConfigFile, Problem, Reader, Severity, line_col, parse_table};
use reqwest::Url;
use std::ffi::OsString;
use std::fmt;

/// Environment variable holding the Rocket.Chat base URL.
pub const ENV_SERVER_URL: &str = "ROCKETTUI_SERVER_URL";
/// Environment variable holding the Rocket.Chat user id.
pub const ENV_USER_ID: &str = "ROCKETTUI_USER_ID";
/// Environment variable holding the Personal Access Token.
pub const ENV_TOKEN: &str = "ROCKETTUI_TOKEN";

/// A credential. `Debug` prints `Secret(<redacted>)`; there is deliberately no `Display`, so
/// `{}` of a `Secret` does not compile.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: String) -> Secret {
        Secret(value)
    }

    /// The raw value. Used only where a request header is built.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

/// One environment variable as the IO shell saw it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum EnvValue {
    /// Not set, or set to the empty string.
    #[default]
    Unset,
    Set(Secret),
    /// Set, but not valid UTF-8.
    NotUnicode,
}

impl EnvValue {
    /// Classify an `std::env::var_os` result. The value is wrapped in `Secret` right here.
    pub fn from_os(value: Option<OsString>) -> EnvValue {
        match value {
            None => EnvValue::Unset,
            Some(os) if os.is_empty() => EnvValue::Unset,
            Some(os) => match os.into_string() {
                Ok(text) => EnvValue::Set(Secret::new(text)),
                Err(_) => EnvValue::NotUnicode,
            },
        }
    }
}

/// The three connection variables.
#[derive(Clone, Debug, Default)]
pub struct ConnEnv {
    pub server_url: EnvValue,
    pub user_id: EnvValue,
    pub token: EnvValue,
}

/// The three connection values as `config.toml` holds them (D-01). The URL keeps the 1-based
/// line and column of its value so a bad one is reported where it was written; it is not a secret
/// (a URL with a user name or password is rejected). The user id and token are `Secret` from the
/// moment they are read. An empty string counts as unset.
#[derive(Clone, Debug, Default)]
pub struct FileConn {
    pub server_url: Option<(String, Option<(usize, usize)>)>,
    pub user_id: Option<Secret>,
    pub token: Option<Secret>,
    /// Keys whose value was present but not a string. `parse_file` already reported them, so
    /// `resolve` neither adds a second "missing" error for them nor treats them as unset.
    pub invalid: Vec<&'static str>,
}

/// Where the token came from: the environment, or `config.toml` (which is what the permission
/// warning is about, D-04).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredSource {
    Env,
    File,
}

/// A validated connection. The derived `Debug` is safe: `Secret` redacts and a URL carrying a
/// user name or password is rejected at parse time.
#[derive(Clone, Debug)]
pub struct Connection {
    pub server_url: Url,
    pub user_id: Secret,
    pub token: Secret,
    pub token_source: CredSource,
}

/// Parse and validate a server URL: http or https, a host, no user name or password, no query or
/// fragment. The error is fixed text that never contains the input.
pub fn parse_server_url(text: &str) -> Result<Url, &'static str> {
    let url = Url::parse(text.trim()).map_err(|_| "must be an http:// or https:// URL")?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("must be an http:// or https:// URL");
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("must not contain a user name or password");
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("must not contain a query or fragment");
    }
    Ok(url)
}

fn error(key: &str, message: String) -> Problem {
    Problem {
        file: ConfigFile::Config,
        pos: None,
        key: Some(key.to_string()),
        message,
        severity: Severity::Error,
    }
}

/// Runtime warnings for a resolved connection (D-04, T-03-21). Texts are fixed and never contain
/// the token, user id or URL.
pub fn warnings(_conn: Option<&Connection>, _config_mode: Option<u32>) -> Vec<String> {
    Vec::new()
}

/// Read `server_url`, `user_id` and `token` from the text of `config.toml` (`None` = absent).
/// A value of another type is an error naming the key and the found type, never the value; syntax
/// errors are left to `settings::parse`, which reports them once (a scratch list is used here).
pub fn parse_file(src: Option<&str>, out: &mut Vec<Problem>) -> FileConn {
    let mut file = FileConn::default();
    let Some(src) = src else {
        return file;
    };
    let mut scratch = Vec::new();
    let Some(root) = parse_table(ConfigFile::Config, src, &mut scratch) else {
        return file;
    };
    let table = root.get_ref();
    let mut reader = Reader {
        file: ConfigFile::Config,
        src,
        out,
    };
    for key in ["server_url", "user_id", "token"] {
        let Some(value) = table.get(key) else {
            continue;
        };
        let Some(text) = reader.string(value, key) else {
            file.invalid.push(key);
            continue;
        };
        if text.is_empty() {
            continue;
        }
        match key {
            "server_url" => {
                file.server_url = Some((text, Some(line_col(src, value.span().start))));
            }
            "user_id" => file.user_id = Some(Secret::new(text)),
            _ => file.token = Some(Secret::new(text)),
        }
    }
    file
}

/// One value after the per-key choice between the environment and `config.toml` (D-01).
enum Pick<'a> {
    Env(&'a str),
    File(&'a str),
    /// Missing or invalid; the error is already in `out`.
    Unusable,
}

/// An environment value that is set and non-empty wins over the file value.
fn pick<'a>(
    env: &'a EnvValue,
    file: Option<&'a str>,
    invalid: bool,
    names: (&'static str, &'static str),
    report_missing: bool,
    out: &mut Vec<Problem>,
) -> Pick<'a> {
    let (key, var) = names;
    match (env, file) {
        (EnvValue::NotUnicode, _) => {
            out.push(error(var, "is not valid UTF-8".to_string()));
            Pick::Unusable
        }
        (EnvValue::Set(secret), _) => Pick::Env(secret.expose()),
        (EnvValue::Unset, Some(text)) => Pick::File(text),
        (EnvValue::Unset, None) => {
            if report_missing && !invalid {
                out.push(error(
                    key,
                    format!("missing: set {key} in config.toml or {var}"),
                ));
            }
            Pick::Unusable
        }
    }
}

/// Resolve the connection from the environment and `config.toml`, each value on its own: a set,
/// non-empty variable wins, otherwise the file value is used (D-01). With `require`, or as soon as
/// one of the three is given anywhere, every missing value is an error naming both places (a
/// partial setting is never silently ignored). Returns `Some` only when all three are present and
/// valid.
pub fn resolve(
    env: &ConnEnv,
    file: &FileConn,
    require: bool,
    out: &mut Vec<Problem>,
) -> Option<Connection> {
    let any_present = [&env.server_url, &env.user_id, &env.token]
        .iter()
        .any(|value| !matches!(value, EnvValue::Unset))
        || file.server_url.is_some()
        || file.user_id.is_some()
        || file.token.is_some()
        || !file.invalid.is_empty();
    let report_missing = require || any_present;
    let url = pick(
        &env.server_url,
        file.server_url.as_ref().map(|(text, _)| text.as_str()),
        file.invalid.contains(&"server_url"),
        ("server_url", ENV_SERVER_URL),
        report_missing,
        out,
    );
    let user_id = pick(
        &env.user_id,
        file.user_id.as_ref().map(Secret::expose),
        file.invalid.contains(&"user_id"),
        ("user_id", ENV_USER_ID),
        report_missing,
        out,
    );
    let token = pick(
        &env.token,
        file.token.as_ref().map(Secret::expose),
        file.invalid.contains(&"token"),
        ("token", ENV_TOKEN),
        report_missing,
        out,
    );
    let (Pick::Env(url_text) | Pick::File(url_text), Pick::Env(user) | Pick::File(user)) =
        (&url, &user_id)
    else {
        return None;
    };
    let (Pick::Env(token_text) | Pick::File(token_text)) = &token else {
        return None;
    };
    match parse_server_url(url_text) {
        Ok(server_url) => Some(Connection {
            server_url,
            user_id: Secret::new((*user).to_string()),
            token: Secret::new((*token_text).to_string()),
            token_source: if matches!(token, Pick::Env(_)) {
                CredSource::Env
            } else {
                CredSource::File
            },
        }),
        Err(message) => {
            match (&url, &file.server_url) {
                (Pick::File(_), Some((_, pos))) => out.push(Problem {
                    pos: *pos,
                    ..error("server_url", message.to_string())
                }),
                _ => out.push(error(ENV_SERVER_URL, message.to_string())),
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SENTINEL: &str = "SENTINEL-TOKEN-9c41";

    fn set(text: &str) -> EnvValue {
        EnvValue::Set(Secret::new(text.to_string()))
    }

    fn full_env() -> ConnEnv {
        ConnEnv {
            server_url: set("http://127.0.0.1:3100"),
            user_id: set("u1"),
            token: set(SENTINEL),
        }
    }

    #[test]
    fn env_resolution() {
        let mut problems = Vec::new();
        let conn =
            resolve(&full_env(), &FileConn::default(), true, &mut problems).expect("connection");
        assert!(problems.is_empty());
        assert_eq!(conn.token_source, CredSource::Env);
        assert_eq!(conn.token.expose(), SENTINEL);
        assert_eq!(conn.user_id.expose(), "u1");
        assert_eq!(conn.server_url.as_str(), "http://127.0.0.1:3100/");
    }

    #[test]
    fn missing_reported() {
        let mut problems = Vec::new();
        assert!(
            resolve(
                &ConnEnv::default(),
                &FileConn::default(),
                true,
                &mut problems
            )
            .is_none()
        );
        assert_eq!(problems.len(), 3);
        let expected = [
            ("server_url", ENV_SERVER_URL),
            ("user_id", ENV_USER_ID),
            ("token", ENV_TOKEN),
        ];
        for (problem, (key, var)) in problems.iter().zip(expected) {
            assert_eq!(problem.severity, Severity::Error);
            assert_eq!(problem.key.as_deref(), Some(key));
            assert!(problem.message.contains(var), "{}", problem.message);
        }

        let mut problems = Vec::new();
        assert!(
            resolve(
                &ConnEnv::default(),
                &FileConn::default(),
                false,
                &mut problems
            )
            .is_none()
        );
        assert!(problems.is_empty());

        // A partial setting is always an error, even when nothing is required.
        let mut problems = Vec::new();
        let env = ConnEnv {
            server_url: set("http://127.0.0.1:3100"),
            ..ConnEnv::default()
        };
        assert!(resolve(&env, &FileConn::default(), false, &mut problems).is_none());
        let keys: Vec<_> = problems.iter().map(|p| p.key.clone().unwrap()).collect();
        assert_eq!(keys, ["user_id", "token"]);

        assert_eq!(EnvValue::from_os(Some(OsString::new())), EnvValue::Unset);
        assert_eq!(EnvValue::from_os(None), EnvValue::Unset);
    }

    #[test]
    fn empty_is_unset_and_not_unicode_is_reported() {
        assert_eq!(EnvValue::from_os(Some("x".into())), set("x"));
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let bad = OsString::from_vec(vec![0x66, 0xff, 0x6f]);
            assert_eq!(EnvValue::from_os(Some(bad)), EnvValue::NotUnicode);
            let mut env = full_env();
            env.token = EnvValue::NotUnicode;
            let mut problems = Vec::new();
            assert!(resolve(&env, &FileConn::default(), true, &mut problems).is_none());
            assert_eq!(problems.len(), 1);
            assert_eq!(problems[0].key.as_deref(), Some(ENV_TOKEN));
        }
    }

    #[test]
    fn server_url_rules() {
        for ok in [
            "http://127.0.0.1:3100",
            "http://localhost:3100/",
            "https://chat.example/sub/",
        ] {
            assert!(parse_server_url(ok).is_ok(), "{ok}");
        }
        for bad in [
            "localhost:3000",
            "ftp://x",
            "http://",
            "https://user:pass@h",
            "http://h/?a=1",
            "http://h/#f",
            "",
        ] {
            // Only the three fixed texts are possible, so no input can be echoed.
            let err = parse_server_url(bad).expect_err(bad);
            assert!(
                [
                    "must be an http:// or https:// URL",
                    "must not contain a user name or password",
                    "must not contain a query or fragment",
                ]
                .contains(&err),
                "{err}"
            );
        }
        // An invalid URL in the environment is reported with a fixed message, never the value.
        let mut env = full_env();
        env.server_url = set("https://user:SENTINEL-URL-PASS@h");
        let mut problems = Vec::new();
        assert!(resolve(&env, &FileConn::default(), true, &mut problems).is_none());
        assert_eq!(problems.len(), 1);
        assert!(!format!("{problems:?}").contains("SENTINEL-URL-PASS"));
    }

    #[test]
    fn secret_redacts() {
        let secret = Secret::new(SENTINEL.to_string());
        assert_eq!(format!("{secret:?}"), "Secret(<redacted>)");
        let env_value = set(SENTINEL);
        assert!(!format!("{env_value:?}").contains(SENTINEL));
        let env = full_env();
        assert!(!format!("{env:?}").contains(SENTINEL));
        let mut problems = Vec::new();
        let conn = resolve(&env, &FileConn::default(), true, &mut problems).expect("connection");
        assert!(!format!("{conn:?}").contains(SENTINEL));
        assert!(!format!("{conn:?}").contains("u1"));
    }

    fn file(src: &str) -> (FileConn, Vec<Problem>) {
        let mut problems = Vec::new();
        let conn = parse_file(Some(src), &mut problems);
        (conn, problems)
    }

    const ALL_THREE: &str = "server_url = \"http://127.0.0.1:3100\"\nuser_id = \"u1\"\ntoken = \"SENTINEL-TOKEN-9c41\"\n";

    #[test]
    fn file_values() {
        let (conn, problems) = file(ALL_THREE);
        assert!(problems.is_empty());
        assert_eq!(
            conn.server_url,
            Some(("http://127.0.0.1:3100".to_string(), Some((1, 14))))
        );
        assert_eq!(conn.user_id, Some(Secret::new("u1".to_string())));
        assert_eq!(conn.token, Some(Secret::new(SENTINEL.to_string())));
        let mut problems = Vec::new();
        let resolved =
            resolve(&ConnEnv::default(), &conn, true, &mut problems).expect("connection");
        assert!(problems.is_empty());
        assert_eq!(resolved.token_source, CredSource::File);
        assert_eq!(resolved.token.expose(), SENTINEL);
        assert_eq!(resolved.user_id.expose(), "u1");
        assert_eq!(resolved.server_url.as_str(), "http://127.0.0.1:3100/");
        assert!(!format!("{conn:?}").contains(SENTINEL));
        assert_eq!(parse_file(None, &mut problems).token, None);
    }

    #[test]
    fn env_overrides_per_key() {
        let (conn, _) = file(ALL_THREE);
        let mut problems = Vec::new();
        let env = ConnEnv {
            token: set("env-token"),
            ..ConnEnv::default()
        };
        let resolved = resolve(&env, &conn, true, &mut problems).expect("connection");
        assert!(problems.is_empty());
        assert_eq!(resolved.token_source, CredSource::Env);
        assert_eq!(resolved.token.expose(), "env-token");
        assert_eq!(resolved.user_id.expose(), "u1");
        assert_eq!(resolved.server_url.as_str(), "http://127.0.0.1:3100/");

        let env = ConnEnv {
            server_url: set("https://chat.example"),
            ..ConnEnv::default()
        };
        let resolved = resolve(&env, &conn, true, &mut problems).expect("connection");
        assert_eq!(resolved.token_source, CredSource::File);
        assert_eq!(resolved.token.expose(), SENTINEL);
        assert_eq!(resolved.server_url.as_str(), "https://chat.example/");
    }

    #[test]
    fn file_url_rules() {
        let (conn, problems) =
            file("server_url = \"localhost:3000\"\nuser_id = \"u\"\ntoken = \"t\"\n");
        assert!(problems.is_empty(), "the shape is checked by resolve");
        let mut problems = Vec::new();
        assert!(resolve(&ConnEnv::default(), &conn, true, &mut problems).is_none());
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].file, ConfigFile::Config);
        assert_eq!(problems[0].severity, Severity::Error);
        assert_eq!(problems[0].pos.map(|(line, _)| line), Some(1));
        assert_eq!(problems[0].key.as_deref(), Some("server_url"));
        assert_eq!(problems[0].message, "must be an http:// or https:// URL");
        assert!(!format!("{problems:?}").contains("localhost:3000"));

        for (bad, message) in [
            (
                "https://u:SENTINEL-URL-PASS@h",
                "must not contain a user name or password",
            ),
            ("http://h/?a=1", "must not contain a query or fragment"),
        ] {
            let (conn, _) = file(&format!(
                "user_id = \"u\"\ntoken = \"t\"\nserver_url = \"{bad}\"\n"
            ));
            let mut problems = Vec::new();
            assert!(resolve(&ConnEnv::default(), &conn, true, &mut problems).is_none());
            assert_eq!(problems.len(), 1);
            assert_eq!(problems[0].message, message);
            assert_eq!(problems[0].pos.map(|(line, _)| line), Some(3));
            assert!(!format!("{problems:?}").contains("SENTINEL-URL-PASS"));
        }

        // An invalid file URL is not reported when the environment supplies the URL.
        let (conn, _) = file("server_url = \"nope\"\nuser_id = \"u\"\ntoken = \"t\"\n");
        let env = ConnEnv {
            server_url: set("http://127.0.0.1:3100"),
            ..ConnEnv::default()
        };
        let mut problems = Vec::new();
        assert!(resolve(&env, &conn, true, &mut problems).is_some());
        assert!(problems.is_empty());
    }

    #[test]
    fn type_and_empty() {
        let (conn, problems) = file("token = 5\n");
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].severity, Severity::Error);
        assert_eq!(problems[0].key.as_deref(), Some("token"));
        assert_eq!(problems[0].message, "expected a string, found integer");
        assert_eq!(problems[0].pos, Some((1, 9)));
        assert!(conn.token.is_none());
        // The wrong type is reported once; resolve does not add a second error for the token.
        let mut out = Vec::new();
        assert!(resolve(&ConnEnv::default(), &conn, true, &mut out).is_none());
        let token_errors = out
            .iter()
            .filter(|p| p.key.as_deref() == Some("token"))
            .count();
        assert_eq!(token_errors, 0, "{out:?}");

        let (conn, problems) = file("token = \"\"\nuser_id = \"\"\nserver_url = \"\"\n");
        assert!(problems.is_empty());
        assert!(conn.token.is_none() && conn.user_id.is_none() && conn.server_url.is_none());
        let mut out = Vec::new();
        assert!(resolve(&ConnEnv::default(), &conn, true, &mut out).is_none());
        assert_eq!(out.len(), 3, "an empty string counts as unset: {out:?}");
        let mut out = Vec::new();
        assert!(resolve(&ConnEnv::default(), &conn, false, &mut out).is_none());
        assert!(out.is_empty());
    }

    #[test]
    fn missing_names_both_places() {
        let mut problems = Vec::new();
        assert!(
            resolve(
                &ConnEnv::default(),
                &FileConn::default(),
                true,
                &mut problems
            )
            .is_none()
        );
        let messages: Vec<&str> = problems.iter().map(|p| p.message.as_str()).collect();
        assert_eq!(
            messages,
            [
                "missing: set server_url in config.toml or ROCKETTUI_SERVER_URL",
                "missing: set user_id in config.toml or ROCKETTUI_USER_ID",
                "missing: set token in config.toml or ROCKETTUI_TOKEN",
            ]
        );
        assert!(problems.iter().all(|p| p.file == ConfigFile::Config));
    }
}
