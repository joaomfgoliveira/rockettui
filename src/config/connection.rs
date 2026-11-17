//! Connection settings: the server URL, user id and Personal Access Token (AUTH-01).
//!
//! This is part of the pure config core: the environment arrives as data (`ConnEnv`), nothing is
//! read here. The user id and token are held in `Secret` from the moment they are read, a wrapper
//! whose `Debug` prints no value and which has no `Display` (D-07). No message produced in this
//! module ever contains a value (T-03-01, T-03-03).

use crate::config::diag::{ConfigFile, Problem, Severity};
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

/// Where the token came from. Plan 03-05 adds `config.toml` credentials and the permission warning.
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

/// Resolve the connection from the environment. With `require`, or as soon as one of the three
/// is set, every missing value is an error (a partial setting is never silently ignored).
/// Returns `Some` only when all three are present and valid.
pub fn resolve(env: &ConnEnv, require: bool, out: &mut Vec<Problem>) -> Option<Connection> {
    let values = [
        (&env.server_url, "server_url", ENV_SERVER_URL),
        (&env.user_id, "user_id", ENV_USER_ID),
        (&env.token, "token", ENV_TOKEN),
    ];
    let any_present = values
        .iter()
        .any(|(value, _, _)| !matches!(value, EnvValue::Unset));
    let mut complete = true;
    for (value, key, var) in values {
        match value {
            EnvValue::NotUnicode => {
                complete = false;
                out.push(error(var, "is not valid UTF-8".to_string()));
            }
            EnvValue::Unset => {
                complete = false;
                if require || any_present {
                    out.push(error(key, format!("missing: set {var}")));
                }
            }
            EnvValue::Set(_) => {}
        }
    }
    if !complete {
        return None;
    }
    let (EnvValue::Set(url_text), EnvValue::Set(user_id), EnvValue::Set(token)) =
        (&env.server_url, &env.user_id, &env.token)
    else {
        return None;
    };
    match parse_server_url(url_text.expose()) {
        Ok(server_url) => Some(Connection {
            server_url,
            user_id: user_id.clone(),
            token: token.clone(),
            token_source: CredSource::Env,
        }),
        Err(message) => {
            out.push(error(ENV_SERVER_URL, message.to_string()));
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
        let conn = resolve(&full_env(), true, &mut problems).expect("connection");
        assert!(problems.is_empty());
        assert_eq!(conn.token_source, CredSource::Env);
        assert_eq!(conn.token.expose(), SENTINEL);
        assert_eq!(conn.user_id.expose(), "u1");
        assert_eq!(conn.server_url.as_str(), "http://127.0.0.1:3100/");
    }

    #[test]
    fn missing_reported() {
        let mut problems = Vec::new();
        assert!(resolve(&ConnEnv::default(), true, &mut problems).is_none());
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
        assert!(resolve(&ConnEnv::default(), false, &mut problems).is_none());
        assert!(problems.is_empty());

        // A partial setting is always an error, even when nothing is required.
        let mut problems = Vec::new();
        let env = ConnEnv {
            server_url: set("http://127.0.0.1:3100"),
            ..ConnEnv::default()
        };
        assert!(resolve(&env, false, &mut problems).is_none());
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
            assert!(resolve(&env, true, &mut problems).is_none());
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
        assert!(resolve(&env, true, &mut problems).is_none());
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
        let conn = resolve(&env, true, &mut problems).expect("connection");
        assert!(!format!("{conn:?}").contains(SENTINEL));
        assert!(!format!("{conn:?}").contains("u1"));
    }
}
