use regex::Captures;
use regex::Regex;
use serde::{Deserialize, Serialize};

use std::borrow::Cow;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::Substitutions;
use crate::env::{expand, expand_env_path};
use crate::error::{Error, Result};
use crate::helper::get_link_function;
use crate::command::Command;
use crate::Opt;
use crate::command_reader::CommandReader;

const COMPILED_SUFFIX: &'static str = ".preprocessed";

/// An opening and a closing character sequence.
/// These delimit string that need special treatment.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Escape {
    /// Starting sequence. I.e. `{{-`
    pub start: String,
    /// Ending sequence. I.e. `-}}`
    pub end: String,
}

/// Configuration for a single dotfile.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FileConfig {
    /// Source file that will be read and preprocessed.
    source: PathBuf,
    /// Target path that will link to the preprocessed file.
    target: PathBuf,
    /// Escape sequence to use for this configuration.
    escape: Option<Escape>,
    /// Line prefix for commands.
    prefix: Option<String>,
    /// Remove instructions after processing?
    remove_instructions: Option<bool>,
}

impl Escape {
    /// Create a regular expression ([`Regex`]).
    ///
    /// The regular expression matches everything, inside `self.start` and `self.end`
    /// and includes `self.start` and `self.end`. This does not match, if `self.start`
    /// is preceded by a backslash (\).
    fn to_regex(&self) -> Result<Regex> {
        let start = regex::escape(&self.start);
        let end = regex::escape(&self.end);
        let s = format!(r"([^\\]){}(.*?[^\\]){}", start, end);
        Regex::new(&s).map_err(Error::FailedToParseRegex)
    }
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
    /// Preprocess this configuration file.
    ///
    /// 1) Replace substitutions.
    /// 2) Evaluate preprocessor instructions.
    /// 3) Write the file.
    pub fn preprocess(&self, substitutions: &Substitutions, opt: &Opt) -> Result<()> {
        // Root directory, config directory.
        let root = opt.config_file.parent().expect("No root directory found");
        info!("Preprocessing {:?}", self.source_path(root));
        // Read the file's contents
        let content = self.source(root)?;
        // Evaluate preprocessor instructions.
        let new_content = self.preprocess_instructions(&content)?;
        // Replace substitutions.
        let newest_content = self.preprocess_substitutions(&new_content, substitutions);
        // Write the preprocessed file.
        self.write_temp(root, newest_content)
    }
    /// Create a symbolic link from target to source.
    pub fn create_link(&self, opt: &Opt) -> Result<()> {
        let mut linker = get_link_function();
        let root = opt.config_file.parent().expect("No root found");
        // Expand environment variables in the paths
        let target_path = self.target_path(root);
        let source_path = self.source_path(root);
        // If the target already exists...
        if target_path.exists() {
            // Verify, that it's just a link...
            let target_md = fs::symlink_metadata(&target_path)
                .map_err(|e| Error::as_failed_link(&source_path, &target_path, e))?;
            if target_md.file_type().is_symlink() {
                // ... and remove it
                fs::remove_file(&target_path)
                    .map_err(|e| Error::as_failed_link(&source_path, &target_path, e))?;
            } else {
                // If it's not a symlink, we should not delete it
                return Err(Error::TargetAlreadyExists(target_path));
            }
        }
        // Get the temp path and remove garbage. This makes the path
        // absolute and removes redundent parts. This is necessary to
        // prevent bad and ugly links.
        let source_path: PathBuf = self
            .temp_path(root)
            .canonicalize()
            .map_err(|e| Error::as_failed_link(&source_path, &target_path, e))?;
        // Create a link from target to source
        info!("Linking {:?} to {:?}", &source_path, &target_path);
        linker(source_path, target_path)?;
        Ok(())
    }
    /// Preprocess substitutions.
    /// Assuming the escape sequences `{++` and `++}` are used. This function replaces
    /// all occurences of `{++KEY++}` with the `VALUE` defined in the given
    /// [`Substitutions`]. The returned content is unaltered, if no escape sequences
    /// are defined, or no usage is found in the given `content`.
    fn preprocess_substitutions<'a>(
        &self,
        content: &'a str,
        substitutions: &Substitutions,
    ) -> Cow<'a, str> {
        // Create a replacer for regex replacements
        let replacer = construct_replacer(substitutions);
        // Get the regex specified explicitly for this file configuration
        let regex = self.escape_regex();
        // Only if we have a regex to work with
        if let Some(regex) = regex {
            // Create the final file content by replacing stuff
            regex.replace_all(content.as_ref(), replacer)
        } else {
            // If no regex is given, inform the user
            info!("No escape characters defined, no substitution will be made");
            Cow::from(content)
        }
    }
    /// Preprocess instructions
    fn preprocess_instructions<'a>(&self, content: &'a str) -> Result<Cow<'a, str>> {
        let prefix = match self.prefix.as_ref() {
            Some(prefix) => prefix,
            None => {
                // Do nothing
                return Ok(Cow::from(content));
            }
        };
        let (mut cmd_lines, mut errors): (Vec<_>, Vec<_>) = content
            .lines()
            .enumerate()
            .filter_map(|(line_nr, line)| {
                Command::parse_from_line(prefix, line).map(|res| (line_nr, res))
            })
            .partition(|(_, res)| res.is_ok());
        if errors.len() > 0 {
            return errors.remove(0).1.map(|_| Cow::from(content));
        }
        let cmd_lines: Vec<_> = cmd_lines
            .drain(..)
            .map(|(line_nr, res)| (line_nr, res.unwrap()))
            .collect();
        let mut skips = CommandReader::read(&cmd_lines)?;
        // Add command lines to skip if necessary
        if self.remove_instructions.expect("Default") {
            let mut cmd_line_nrs = cmd_lines.iter().map(|(line_nr, _)| *line_nr);
            skips.extend(&mut cmd_line_nrs);
        }
        if skips.is_empty() {
            Ok(Cow::from(content))
        } else {
            let remaining_lines: Vec<_> = content
                .lines()
                .enumerate()
                .filter(|(line_nr, _)| !skips.contains(line_nr))
                .map(|(_, line)| line)
                .collect();
            // TODO: Plattform independet line endings
            Ok(remaining_lines.join("\n").into())
        }
    }
}

/// Create a [`regex::Replacer`] for the given substitutions. This replacer
/// can then be used to replace instances found by the regular expression
/// created by any [`Escape::to_regex`].
fn construct_replacer<'a>(
    substitutions: &'a Substitutions,
) -> impl FnMut(&Captures) -> String + 'a {
    move |captures| {
        let inner = &captures[2];
        match substitutions.get(inner) {
            Some(repl) => format!("{}{}", expand(&captures[1]), repl),
            None => format!("{}{}", &captures[1], expand(inner)),
        }
    }
}
