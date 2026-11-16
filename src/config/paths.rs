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

fn read_error(file: ConfigFile, message: String) -> Problem {
    Problem {
        file,
        pos: None,
        key: None,
        message,
        severity: Severity::Error,
    }
}

/// Read one file. A missing file is `None` and not a problem (CONF-02); any other failure is an
/// error and also `None`.
pub fn read_source(dir: &Path, file: ConfigFile, out: &mut Vec<Problem>) -> Option<String> {
    match fs::read_to_string(dir.join(file.file_name())) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(e) => {
            out.push(read_error(file, format!("cannot read file ({})", e.kind())));
            None
        }
    }
}

/// Read the three files of `dir`.
pub fn read_sources(dir: &Path) -> Sources {
    let mut read_errors = Vec::new();
    let config = read_source(dir, ConfigFile::Config, &mut read_errors);
    let keymap = read_source(dir, ConfigFile::Keymap, &mut read_errors);
    let theme = read_source(dir, ConfigFile::Theme, &mut read_errors);
    Sources {
        config,
        keymap,
        theme,
        read_errors,
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
}
