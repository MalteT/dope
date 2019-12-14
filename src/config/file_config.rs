use regex::Regex;
use serde::{Deserialize, Serialize};

use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::config::Escape;
use crate::env::expand_env_path;
use crate::error::{Error, Result};

const COMPILED_SUFFIX: &'static str = ".preprocessed";

/// Configuration of a single dotfile.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FileConfig {
    source: PathBuf,
    target: PathBuf,
    escape: Option<Escape>,
    prefix: Option<String>,
    remove_instructions: Option<bool>,
}

impl FileConfig {
    /// Get the source path of the configuration.
    /// If source is relative, it will be interpreted as relative to the given root.
    /// If source is absolute, that path will be used.
    /// Environment variables in the path will be interpreted before
    /// determining whether the path is relative or not.
    pub fn source_path<P: AsRef<Path>>(&self, root: P) -> PathBuf {
        root.as_ref().join(expand_env_path(&self.source))
    }
    /// Get the content of the source file.
    /// See [`FileConfig::source_path`] for the path that will be read.
    pub fn source<P: AsRef<Path>>(&self, root: P) -> Result<String> {
        let source_path = self.source_path(root);
        fs::read_to_string(&source_path).map_err(|e| {
            let path_string = source_path.to_string_lossy().into();
            Error::FailedToReadSourceFile(path_string, e)
        })
    }
    /// Get the target path of the configuration.
    /// Behaves like [`FileConfig::source_path`] but returns the target path.
    pub fn target_path<P: AsRef<Path>>(&self, root: P) -> PathBuf {
        root.as_ref().join(expand_env_path(&self.target))
    }
    /// Get the temporary path for storing the preprocessed file.
    /// This will use the expanded source path (see [`FileConfig::source_path`])
    /// and append [`COMPILED_SUFFIX`].
    pub fn temp_path<P: AsRef<Path>>(&self, root: P) -> PathBuf {
        format!(
            "{}{}",
            self.source_path(root).to_string_lossy(),
            COMPILED_SUFFIX
        )
        .into()
    }
    /// Write the given `content` to the temporary file.
    /// See [`FileConfig::temp_path`] for the path that will be used.
    /// This will sync the file contents to disk on success.
    pub fn write_temp<P, S>(&self, root: P, content: S) -> Result<()>
    where
        P: AsRef<Path>,
        S: AsRef<str>,
    {
        let temp_path = self.temp_path(root);
        let mut temp = File::create(&temp_path).map_err(|e| {
            let path_string = temp_path.to_string_lossy().into();
            Error::FailedToOpenTempFile(path_string, e)
        })?;
        write!(temp, "{}", content.as_ref())
            .and_then(|_| temp.sync_all())
            .map_err(|e| {
                let path_string = temp_path.to_string_lossy().into();
                Error::FailedToWriteTempFile(path_string, e)
            })
    }
    /// Get the regex for finding escapes in the source file.
    /// This method returns `None` if no escape sequence was specified.
    pub fn escape_regex(&self) -> Option<Regex> {
        self.escape
            .as_ref()
            .map(Escape::to_regex)
            .map(Result::unwrap)
    }
    /// Replace `None`s with the given defaults.
    /// Defined values (`Some`s) will not be changed.
    pub fn supplement(
        &mut self,
        escape: &Option<Escape>,
        remove_instructions: bool,
        prefix: &Option<String>,
    ) {
        if self.escape.is_none() {
            self.escape = escape.clone();
        }
        if self.remove_instructions.is_none() {
            self.remove_instructions = Some(remove_instructions);
        }
        if self.prefix.is_none() {
            self.prefix = prefix.clone();
        }
    }
}
