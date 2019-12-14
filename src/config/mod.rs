use colored::Colorize;
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

mod file_config;

use crate::env::{expand};
use crate::error::{Error, Result};
use crate::helper::get_link_function;
use crate::Opt;
pub use file_config::FileConfig;

/// The complete, normalized configuration file.
///
/// All unset options of file_configurations have been filled with default options,
/// if those were defined.
#[derive(Debug)]
pub struct Config {
    pub file_configurations: Vec<FileConfig>,
    pub substitutions: HashMap<String, String>,
}

/// The complete configuration file.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RawConfig {
    default_escape: Option<Escape>,
    default_prefix: Option<String>,
    #[serde(default = "default_remove_instrucions")]
    default_remove_instructions: bool,
    #[serde(default, rename = "config")]
    file_configurations: Vec<FileConfig>,
    #[serde(default)]
    substitutions: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Escape {
    pub start: String,
    pub end: String,
}

fn default_remove_instrucions() -> bool {
    true
}

impl RawConfig {
    /// Load a raw configuration from the given path.
    fn load<P: AsRef<Path>>(config_path: P) -> Result<Self> {
        let path = config_path.as_ref();
        let content = fs::read_to_string(path).map_err(Error::as_load_config)?;
        toml::from_str(&content).map_err(Error::FailedToParseConfiguration)
    }
}

impl Config {
    /// Load the configuration from the given path.
    pub fn load<P: AsRef<Path>>(config_path: P) -> Result<Self> {
        RawConfig::load(config_path).map(Config::from)
    }
    /// Preprocess all necessary files.
    ///
    /// This function iterates over all [`Config.config`]. The original file is loaded,
    /// replacements are applied and the file is written to a temporary location.
    pub fn preprocess_files(&self, opt: &Opt) -> Result<()> {
        // Root directory, config directory.
        let root = opt.config_file.parent().expect("No root directory found");
        // Iterate over all config file entries
        for fc in &self.file_configurations {
            info!("Preprocessing {:?}", fc.source_path(root));
            // Read the file's contents
            // TODO: Soft error handling
            let content = fc.source(root)?;
            // Create a replacer for regex replacements
            let replacer = construct_replacer(self);
            // Get the regex specified explicitly for this file configuration
            let regex = fc.escape_regex();
            // Only if we have a regex to work with
            if let Some(regex) = regex {
                // Create the final file content by replacing stuff
                let new = regex.replace_all(&content, replacer).to_string();
                // Write the temporary file.
                // TODO: Soft error handling
                fc.write_temp(root, &new)?;
            } else {
                // If no regex is given, inform the user
                warn!("No escape characters defined...");
            }
        }

        Ok(())
    }
    /// Link all temporary files to their final destination.
    pub fn link_files(&self, opt: &Opt) -> Result<()> {
        let mut linker = get_link_function();
        let root = opt.config_file.parent().expect("No root found");
        // Iterate over all file configurations
        for fc in &self.file_configurations {
            // Expand environment variables in the paths
            let target_path = fc.target_path(root);
            let source_path = fc.source_path(root);
            // If the target already exists...
            if target_path.exists() {
                // Verify, that it's just a link...
                // TODO: Soft error handling
                let target_md = fs::symlink_metadata(&target_path).map_err(|e| {
                    Error::as_failed_link(&source_path, &target_path, e)
                })?;
                if target_md.file_type().is_symlink() {
                    // ... and remove it
                    fs::remove_file(&target_path).map_err(|e| {
                        Error::as_failed_link(&source_path, &target_path, e)
                    })?;
                } else {
                    error!("Target {:?} already exists", &target_path);
                    continue;
                }
            }
            // Get the temp path and remove garbage. This makes the path
            // absolute and removes redundent parts. This is necessary to
            // prevent bad and ugly links.
            let source_path: PathBuf = fc.temp_path(root).canonicalize().map_err(|e| {
                Error::as_failed_link(&source_path, &target_path, e)
            })?;
            // Create a link from target to source
            info!("Linking {:?} to {:?}", &source_path, &target_path);
            linker(source_path, target_path)?;
        }

        Ok(())
    }
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
/// Create a [`regex::Replacer`] for the given [`Config`]. This replacer
/// can then be used to replace instances found by the regular expression
/// created by any [`Escape::to_regex`].
/// It uses the replacements stored in the [`Config`] to create substitutions.
fn construct_replacer<'a>(config: &'a Config) -> impl FnMut(&Captures) -> String + 'a {
    let replacements = config.substitutions.clone();
    move |captures| {
        let inner = &captures[2];
        match replacements.get(inner) {
            Some(repl) => format!("{}{}", expand(&captures[1]), repl),
            None => format!("{}{}", &captures[1], expand(inner)),
        }
    }
}

impl From<RawConfig> for Config {
    fn from(raw: RawConfig) -> Self {
        let mut file_configurations = raw.file_configurations;
        let prefix = raw.default_prefix;
        let remove_instructions = raw.default_remove_instructions;
        let escape = raw.default_escape;
        let substitutions = raw.substitutions.unwrap_or(HashMap::new());
        // Fill in the defaults where necessary
        for fc in &mut file_configurations {
            fc.supplement(&escape, remove_instructions, &prefix);
        }
        Config {
            file_configurations,
            substitutions,
        }
    }
}
