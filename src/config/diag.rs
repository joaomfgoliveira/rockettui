//! Problems found while loading the config files: where they are, what they say, and the
//! `Reader` that walks a spanned TOML tree and collects them (D-02, D-03).
//!
//! Messages are built from the toml crate's `message()` and `span()` only. Its `Display` output
//! prints the offending source line, which could reveal a credential kept in `config.toml`
//! (T-02-01), so it is never used here.

use crate::ui::sanitize::sanitize_line;
use std::fmt;
use std::path::PathBuf;
use toml::Spanned;
use toml::de::{DeTable, DeValue};

/// The three config files. Declaration order is the order problems are reported in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConfigFile {
    Config,
    Keymap,
    Theme,
}

impl ConfigFile {
    pub fn file_name(self) -> &'static str {
        match self {
            ConfigFile::Config => "config.toml",
            ConfigFile::Keymap => "keymap.toml",
            ConfigFile::Theme => "theme.toml",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

/// One problem in one file. `pos` is 1-based (line, column).
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub file: ConfigFile,
    pub pos: Option<(usize, usize)>,
    pub key: Option<String>,
    pub message: String,
    pub severity: Severity,
}

impl Problem {
    /// `config.toml:3:17` or just `config.toml` when the position is unknown.
    pub fn location(&self) -> String {
        match self.pos {
            Some((line, col)) => format!("{}:{line}:{col}", self.file.file_name()),
            None => self.file.file_name().to_string(),
        }
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}: ", self.location(), self.severity.label())?;
        if let Some(key) = &self.key {
            write!(f, "{}: ", sanitize_line(key))?;
        }
        write!(f, "{}", sanitize_line(&self.message))
    }
}

/// 1-based (line, column) of byte `offset` in `src`; the column counts characters.
pub fn line_col(src: &str, offset: usize) -> (usize, usize) {
    let mut end = offset.min(src.len());
    while !src.is_char_boundary(end) {
        end -= 1;
    }
    let before = &src[..end];
    let line = before.matches('\n').count() + 1;
    let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, col)
}

/// Everything wrong with a configuration, errors first. Sorted by file, then position.
#[derive(Debug, Default)]
pub struct Report {
    pub dir: Option<PathBuf>,
    pub errors: Vec<Problem>,
    pub warnings: Vec<Problem>,
}

impl Report {
    /// Split by severity; each list is stably sorted by (file, position), positionless first, so
    /// problems at the same position keep their discovery order.
    pub fn new(problems: Vec<Problem>) -> Report {
        let (mut errors, mut warnings): (Vec<Problem>, Vec<Problem>) = problems
            .into_iter()
            .partition(|p| p.severity == Severity::Error);
        errors.sort_by_key(|p| (p.file, p.pos));
        warnings.sort_by_key(|p| (p.file, p.pos));
        Report {
            dir: None,
            errors,
            warnings,
        }
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "rockettui: invalid configuration")?;
        if let Some(dir) = &self.dir {
            write!(f, " in {}", sanitize_line(&dir.to_string_lossy()))?;
        }
        writeln!(f)?;
        for problem in self.errors.iter().chain(&self.warnings) {
            writeln!(f, "{problem}")?;
        }
        Ok(())
    }
}

/// Typed access to the values of one parsed file; every problem goes to `out`.
pub struct Reader<'a> {
    pub file: ConfigFile,
    pub src: &'a str,
    pub out: &'a mut Vec<Problem>,
}

impl Reader<'_> {
    fn push(&mut self, severity: Severity, offset: usize, key: &str, message: String) {
        self.out.push(Problem {
            file: self.file,
            pos: Some(line_col(self.src, offset)),
            key: Some(key.to_string()),
            message,
            severity,
        });
    }

    pub fn error(&mut self, offset: usize, key: &str, message: impl Into<String>) {
        self.push(Severity::Error, offset, key, message.into());
    }

    pub fn warning(&mut self, offset: usize, key: &str, message: impl Into<String>) {
        self.push(Severity::Warning, offset, key, message.into());
    }

    /// A string value, or an error when the value has another type.
    pub fn string(&mut self, value: &Spanned<DeValue<'_>>, key: &str) -> Option<String> {
        match value.get_ref() {
            DeValue::String(s) => Some(s.to_string()),
            other => {
                self.error(
                    value.span().start,
                    key,
                    format!("expected a string, found {}", other.type_str()),
                );
                None
            }
        }
    }

    /// A string value that must be one of `choices`.
    pub fn one_of<T: Copy>(
        &mut self,
        value: &Spanned<DeValue<'_>>,
        key: &str,
        choices: &[(&str, T)],
    ) -> Option<T> {
        let text = self.string(value, key)?;
        if let Some((_, found)) = choices.iter().find(|(name, _)| *name == text) {
            return Some(*found);
        }
        let allowed: Vec<String> = choices
            .iter()
            .map(|(name, _)| format!("\"{name}\""))
            .collect();
        self.error(
            value.span().start,
            key,
            format!("must be one of {}", allowed.join(", ")),
        );
        None
    }
}

/// Parse one file into a spanned table. On a syntax error the problem is recorded and `None` is
/// returned. The message comes from `message()` and `span()` only (T-02-01).
pub fn parse_table<'s>(
    file: ConfigFile,
    src: &'s str,
    out: &mut Vec<Problem>,
) -> Option<Spanned<DeTable<'s>>> {
    match DeTable::parse(src) {
        Ok(table) => Some(table),
        Err(e) => {
            out.push(Problem {
                file,
                pos: e.span().map(|span| line_col(src, span.start)),
                key: None,
                message: format!("TOML syntax error: {}", e.message()),
                severity: Severity::Error,
            });
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_col_counts_chars() {
        assert_eq!(line_col("ab\ncd", 0), (1, 1));
        assert_eq!(line_col("ab\ncd", 4), (2, 2));
        assert_eq!(line_col("é=1", 2), (1, 2));
        assert_eq!(line_col("x", 99), (1, 2));
    }

    #[test]
    fn problem_display_forms() {
        let with_all = Problem {
            file: ConfigFile::Config,
            pos: Some((3, 17)),
            key: Some("message_layout".into()),
            message: "bad".into(),
            severity: Severity::Error,
        };
        assert_eq!(
            with_all.to_string(),
            "config.toml:3:17: error: message_layout: bad"
        );
        let bare = Problem {
            pos: None,
            key: None,
            severity: Severity::Warning,
            ..with_all
        };
        assert_eq!(bare.to_string(), "config.toml: warning: bad");
    }
}
