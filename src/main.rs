use colored::Colorize;
use regex::{Captures, Error as RegexError, Regex};
use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::Result as IOResult;
use std::io::Write;
use std::path::{Path, PathBuf};

#[macro_use]
mod logging;
mod env;
mod expand;

use env::{expand, expand_env_path};

const COMPILED_SUFFIX: &'static str = ".compiled";

/// The complete configuration file.
#[derive(Debug, Deserialize, Serialize)]
struct Config {
    pub default_escape: Option<Escape>,
    #[serde(default)]
    pub config: Vec<FileConfig>,
    #[serde(default)]
    pub replacement: Option<HashMap<String, String>>,
}

/// Escapes for some content.
#[derive(Debug, Deserialize, Serialize)]
struct Escape {
    pub start: String,
    pub end: String,
}

/// Configuration of a single file.
#[derive(Debug, Deserialize, Serialize)]
struct FileConfig {
    source: PathBuf,
    target: PathBuf,
    escape: Option<Escape>,
}

impl Escape {
    /// Create a regular expression ([`Regex`]).
    ///
    /// The regular expression matches everything, inside `self.start` and `self.end`
    /// and includes `self.start` and `self.end`. This does not match, if `self.start`
    /// is preceded by a backslash (\).
    fn to_regex(&self) -> Result<Regex, RegexError> {
        let start = regex::escape(&self.start);
        let end = regex::escape(&self.end);
        let s = format!(r"([^\\]){}(.*?[^\\]){}", start, end);
        Regex::new(&s)
    }
}
/// Create a [`regex::Replacer`] for the given [`Config`]. This replacer
/// can then be used to replace instances found by the regular expression
/// created by any [`Escape::to_regex`].
/// It uses the replacements stored in the [`Config`] to create substitutions.
fn construct_replacer(config: &Config) -> impl FnMut(&Captures) -> String {
    let replacements = config.replacement.clone().unwrap_or(HashMap::new());
    move |captures| {
        let inner = &captures[2];
        match replacements.get(inner) {
            Some(repl) => format!("{}{}", expand(&captures[1]), repl),
            None => format!("{}{}", &captures[1], expand(inner)),
        }
    }
}
/// Preprocess all necessary files.
///
/// This function iterates over all [`Config.config`]. The original file is loaded,
/// replacements are applied and the file is written to a temporary location.
fn preprocess(config: &Config) -> IOResult<()> {
    // The default regex is created from the default_escape
    let default_regex = config.default_escape.as_ref().map(Escape::to_regex);
    // Iterate over all config file entries
    for fc in &config.config {
        info!("Preprocessing {:?}", fc.source);
        // Read the file's contents
        let content = fs::read_to_string(expand_env_path(&fc.source))?;
        // Create a replacer for regex replacements
        let replacer = construct_replacer(config);
        // Get the regex specified explicitly for this file configuration
        let regex = fc.escape.as_ref().map(Escape::to_regex);
        // Supplement the default regex, if the current is `None`
        let regex: Option<&Result<Regex, _>> = regex.as_ref().or(default_regex.as_ref());
        // Only if we have a regex to work with
        if let Some(Ok(regex)) = regex {
            // Create the final file content by replacing stuff
            let new = regex.replace_all(&content, replacer).to_string();
            // Attach the temp file suffix to the source path
            let compiled_path = format!("{}{}", fc.source.to_str().unwrap(), COMPILED_SUFFIX);
            // Create a file object for writing to the temp file
            let mut file = File::create(expand_env_path(compiled_path.as_ref()))?;
            // Write the new content to the temporary file
            info!("Writing preprocessed file to {:?}", compiled_path);
            write!(file, "{}", new)?;
            // Sync, to prevent partial file writing
            file.sync_all()?;
        } else {
            // If no regex is given, inform the user
            error!("No escape characters defined...");
        }
    }

    Ok(())
}
/// Link all temporary files to their final destination.
fn link(config: &Config) -> IOResult<()> {
    #[cfg(unix)]
    let link_file = std::os::unix::fs::symlink;
    #[cfg(windows)]
    let link_file = std::os::windows::fs::symlink_file;
    // Iterate over all file configurations
    for fc in &config.config {
        // Expand environment variables in target
        let target = expand_env_path(&fc.target);
        // If the target already exists...
        if Path::new(&target).exists() {
            // Verify, that it's just a link...
            let target_md = fs::symlink_metadata(&target)?;
            if target_md.file_type().is_symlink() {
                // And Remove it
                fs::remove_file(&target)?;
            } else {
                error!("Target {:?} already exists", target);
                continue;
            }
        }
        // The souce needs the temporary suffix
        let source: PathBuf = format!("{}{}", fc.source.to_str().unwrap(), COMPILED_SUFFIX).into();
        // Make path absolute to prevent garbage linking
        let source = source.canonicalize()?;
        // Create a link from target to source
        info!("Linking {:?} to {:?}", source, target);
        link_file(source, target)?;
    }

    Ok(())
}

fn main() {
    let test_toml = fs::read_to_string("./test.toml").unwrap();
    let config = toml::from_str(&test_toml).unwrap();
    info!("PRE: {:?}", preprocess(&config));
    info!("LIN: {:?}", link(&config));
}
