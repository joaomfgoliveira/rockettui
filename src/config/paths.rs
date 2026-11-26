//! Where the config files live and how they are read. The only module that touches the
//! filesystem, and it only reads: nothing under the config directory is ever created, written,
//! chmodded or deleted (CONF-01).

use super::Sources;
use super::diag::{ConfigFile, Problem, Severity};
use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// `$XDG_CONFIG_HOME/rockettui` when that is an absolute path, else `$HOME/.config/rockettui`.
/// A relative or empty `$XDG_CONFIG_HOME` is ignored (XDG spec, T-02-04). `None` when no home
/// is known. The environment is passed in: edition 2024 makes `set_var` unsafe, so tests could
/// not set it.
pub fn config_dir(xdg: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let base = match xdg.map(PathBuf::from) {
        Some(p) if p.is_absolute() => p,
        _ => PathBuf::from(home.filter(|h| !h.is_empty())?).join(".config"),
    };
    Some(base.join("rockettui"))
}

/// `$XDG_STATE_HOME/rockettui` when that is an absolute path, else `$HOME/.local/state/rockettui`.
/// The same rules as `config_dir`: a relative or empty variable is ignored, and `None` when no home
/// is known. Only names the directory; this module never creates it.
pub fn state_dir(xdg: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let base = match xdg.map(PathBuf::from) {
        Some(p) if p.is_absolute() => p,
        _ => PathBuf::from(home.filter(|h| !h.is_empty())?)
            .join(".local")
            .join("state"),
    };
    Some(base.join("rockettui"))
}

fn read_error(file: ConfigFile, message: String) -> Problem {
    Problem {
        file,
        pos: None,
        key: None,
        message,
        severity: Severity::Error,
    }
}

/// Config files larger than this are an error, not read (T-02-03).
pub const MAX_CONFIG_BYTES: u64 = 1_048_576;

/// Read one file. A missing file is `None` and not a problem (CONF-02); anything else that stops
/// it being read (not a regular file, over 1 MiB, not UTF-8, an IO error) is an error and also
/// `None`. The metadata check runs first so a FIFO or device is never opened (T-02-03).
pub fn read_source(dir: &Path, file: ConfigFile, out: &mut Vec<Problem>) -> Option<String> {
    let path = dir.join(file.file_name());
    let meta = match fs::metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == ErrorKind::NotFound => return None,
        Err(e) => {
            out.push(read_error(file, format!("cannot read file ({})", e.kind())));
            return None;
        }
    };
    if !meta.is_file() {
        out.push(read_error(file, "not a regular file".to_string()));
        return None;
    }
    if meta.len() > MAX_CONFIG_BYTES {
        out.push(read_error(file, "larger than 1 MiB".to_string()));
        return None;
    }
    match fs::read_to_string(&path) {
        Ok(text) => Some(text),
        // The file vanished between the metadata check and the read: it is absent.
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(e) if e.kind() == ErrorKind::InvalidData => {
            out.push(read_error(file, "not valid UTF-8".to_string()));
            None
        }
        Err(e) => {
            out.push(read_error(file, format!("cannot read file ({})", e.kind())));
            None
        }
    }
}

/// The permission bits of `path`, following symlinks like `read_source`. Only reads the metadata:
/// nothing is created, written or chmodded (D-04 warns, it never fixes). `None` off Unix.
#[cfg(unix)]
fn file_mode(path: &Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .ok()
        .map(|meta| meta.permissions().mode())
}

#[cfg(not(unix))]
fn file_mode(_path: &Path) -> Option<u32> {
    None
}

/// Read the three files of `dir`. `config_mode` is filled only when `config.toml` was read as
/// text, once, together with the text (so a later chmod changes the warning at the next launch).
pub fn read_sources(dir: &Path) -> Sources {
    let mut read_errors = Vec::new();
    let config = read_source(dir, ConfigFile::Config, &mut read_errors);
    let config_mode = config
        .as_ref()
        .and_then(|_| file_mode(&dir.join(ConfigFile::Config.file_name())));
    let keymap = read_source(dir, ConfigFile::Keymap, &mut read_errors);
    let theme = read_source(dir, ConfigFile::Theme, &mut read_errors);
    Sources {
        config,
        keymap,
        theme,
        read_errors,
        config_mode,
        ..Sources::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(s: &str) -> Option<OsString> {
        Some(OsString::from(s))
    }

    #[test]
    fn config_dir_resolution() {
        let home_cfg = Some(PathBuf::from("/home/u/.config/rockettui"));
        assert_eq!(
            config_dir(os("/x"), os("/home/u")),
            Some(PathBuf::from("/x/rockettui"))
        );
        assert_eq!(config_dir(os("rel"), os("/home/u")), home_cfg);
        assert_eq!(config_dir(os(""), os("/home/u")), home_cfg);
        assert_eq!(config_dir(None, os("/home/u")), home_cfg);
        assert_eq!(config_dir(None, os("")), None);
        assert_eq!(config_dir(None, None), None);
    }

    #[test]
    fn state_dir_resolution() {
        let home_state = Some(PathBuf::from("/home/u/.local/state/rockettui"));
        assert_eq!(
            state_dir(os("/x"), os("/home/u")),
            Some(PathBuf::from("/x/rockettui"))
        );
        assert_eq!(state_dir(os("rel"), os("/home/u")), home_state);
        assert_eq!(state_dir(os(""), os("/home/u")), home_state);
        assert_eq!(state_dir(None, os("/home/u")), home_state);
        assert_eq!(state_dir(None, os("")), None);
        assert_eq!(state_dir(None, None), None);
    }

    #[test]
    fn read_source_hardening() {
        use crate::test_util::temp_config_dir;
        let dir = temp_config_dir("harden", &[("keymap.toml", "ok = 1\n")]);
        std::fs::create_dir(dir.join("config.toml")).unwrap();
        std::fs::write(dir.join("theme.toml"), vec![b'#'; 1_048_577]).unwrap();
        let entries = |d: &Path| {
            let mut names: Vec<_> = std::fs::read_dir(d)
                .unwrap()
                .map(|e| e.unwrap().file_name())
                .collect();
            names.sort();
            names
        };
        let before = entries(&dir);

        let mut out = Vec::new();
        assert_eq!(
            read_source(&dir, ConfigFile::Keymap, &mut out).as_deref(),
            Some("ok = 1\n")
        );
        assert!(out.is_empty());
        assert!(read_source(&dir, ConfigFile::Config, &mut out).is_none());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].message, "not a regular file");
        out.clear();
        assert!(read_source(&dir, ConfigFile::Theme, &mut out).is_none());
        assert_eq!(out[0].message, "larger than 1 MiB");
        out.clear();

        std::fs::remove_file(dir.join("theme.toml")).unwrap();
        assert!(read_source(&dir, ConfigFile::Theme, &mut out).is_none());
        assert!(out.is_empty(), "absent is not a problem");
        std::fs::write(dir.join("theme.toml"), [0xff, 0xfe]).unwrap();
        assert!(read_source(&dir, ConfigFile::Theme, &mut out).is_none());
        assert_eq!(out[0].message, "not valid UTF-8");
        std::fs::remove_file(dir.join("theme.toml")).unwrap();

        assert_eq!(entries(&dir), {
            let mut b = before;
            b.retain(|n| n != "theme.toml");
            b
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn perm_mode_recorded() {
        use crate::test_util::temp_config_dir;
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_config_dir("permmode", &[("config.toml", "sidebar_width = 30\n")]);
        std::fs::set_permissions(
            dir.join("config.toml"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        let entries = |d: &Path| {
            let mut names: Vec<_> = std::fs::read_dir(d)
                .unwrap()
                .map(|e| e.unwrap().file_name())
                .collect();
            names.sort();
            names
        };
        let before = entries(&dir);
        let sources = read_sources(&dir);
        assert_eq!(sources.config_mode.map(|mode| mode & 0o777), Some(0o644));
        assert_eq!(
            entries(&dir),
            before,
            "reading must not create or delete files"
        );
        let mode_after = std::fs::metadata(dir.join("config.toml"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode_after & 0o777, 0o644, "reading must not chmod");

        std::fs::remove_file(dir.join("config.toml")).unwrap();
        assert_eq!(read_sources(&dir).config_mode, None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
