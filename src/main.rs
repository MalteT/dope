use colored::Colorize;
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use structopt::StructOpt;

use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process;

#[macro_use]
mod logging;
mod env;
mod error;
mod expand;
mod helper;

use env::{expand, expand_env_path};
use error::{Error, Result};
use helper::get_link_function;

const COMPILED_SUFFIX: &'static str = ".preprocessed";

#[derive(StructOpt, Debug)]
#[structopt(name = "dotfile-preprcoessor")]
struct Opt {
    /// Specify the TOML configuration file.
    #[structopt(
        long = "config",
        short,
        default_value = "./preprocessor.toml",
        hide_default_value = true
    )]
    config_file: PathBuf,
}

/// The complete configuration file.
#[derive(Debug, Deserialize, Serialize, Clone)]
struct Config {
    pub default_escape: Option<Escape>,
    pub default_prefix: Option<String>,
    #[serde(default = "default_remove_instrucions")]
    pub default_remove_instructions: bool,
    #[serde(default, rename = "config")]
    pub file_configurations: Vec<FileConfig>,
    #[serde(default)]
    pub substitutions: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Escape {
    pub start: String,
    pub end: String,
}

/// Configuration of a single dotfile.
#[derive(Debug, Deserialize, Serialize, Clone)]
struct FileConfig {
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
    fn source_path<P: AsRef<Path>>(&self, root: P) -> PathBuf {
        root.as_ref().join(expand_env_path(&self.source))
    }
    /// Get the content of the source file.
    /// See [`FileConfig::source_path`] for the path that will be read.
    fn source<P: AsRef<Path>>(&self, root: P) -> Result<String> {
        let source_path = self.source_path(root);
        fs::read_to_string(&source_path).map_err(|e| {
            let path_string = source_path.to_string_lossy().into();
            Error::FailedToReadSourceFile(path_string, e)
        })
    }
    /// Get the target path of the configuration.
    /// Behaves like [`FileConfig::source_path`] but returns the target path.
    fn target_path<P: AsRef<Path>>(&self, root: P) -> PathBuf {
        root.as_ref().join(expand_env_path(&self.target))
    }
    /// Get the temporary path for storing the preprocessed file.
    /// This will use the expanded source path (see [`FileConfig::source_path`])
    /// and append [`COMPILED_SUFFIX`].
    fn temp_path<P: AsRef<Path>>(&self, root: P) -> PathBuf {
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
    fn write_temp<P, S>(&self, root: P, content: S) -> Result<()>
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
}

impl Config {
    /// Normalize the configuration.
    ///
    /// This sets unset values of file configurations, if a default is set.
    fn normalize(mut self) -> Self {
        let default_escape = self.default_escape.clone();
        let default_remove_instructions = self.default_remove_instructions;
        let default_prefix = self.default_prefix.clone();
        for fc in &mut self.file_configurations {
            if fc.escape.is_none() {
                fc.escape = default_escape.clone();
            }
            if fc.remove_instructions.is_none() {
                fc.remove_instructions = Some(default_remove_instructions);
            }
            if fc.prefix.is_none() {
                fc.prefix = default_prefix.clone();
            }
        }
        self
    }
    /// Preprocess all necessary files.
    ///
    /// This function iterates over all [`Config.config`]. The original file is loaded,
    /// replacements are applied and the file is written to a temporary location.
    fn preprocess_files(&self, opt: &Opt) -> Result<()> {
        // The default regex is created from the default_escape
        let default_regex = self.default_escape.as_ref().map(Escape::to_regex);
        // Root directory, config directory.
        let root = opt.config_file.parent().expect("No root directory found");
        // Iterate over all config file entries
        for fc in &self.file_configurations {
            info!("Preprocessing {:?}", fc.source);
            // Read the file's contents
            // TODO: Soft error handling
            let content = fc.source(root)?;
            // Create a replacer for regex replacements
            let replacer = construct_replacer(self);
            // Get the regex specified explicitly for this file configuration
            let regex = fc.escape.as_ref().map(Escape::to_regex);
            // Supplement the default regex, if the current is `None`
            let regex: Option<&Result<Regex>> = regex.as_ref().or(default_regex.as_ref());
            // Only if we have a regex to work with
            if let Some(Ok(regex)) = regex {
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
    fn link_files(&self, opt: &Opt) -> Result<()> {
        let mut linker = get_link_function();
        let root = opt.config_file.parent().expect("No root found");
        // Iterate over all file configurations
        for fc in &self.file_configurations {
            // Expand environment variables in target
            let target = fc.target_path(root);
            // If the target already exists...
            if target.exists() {
                // Verify, that it's just a link...
                // TODO: Soft error handling
                let target_md = fs::symlink_metadata(&target).map_err(|e| {
                    let src_string = fc.source.to_string_lossy().into();
                    let dst_string = target.to_string_lossy().into();
                    Error::FailedToCreateTargetLink(src_string, dst_string, e)
                })?;
                if target_md.file_type().is_symlink() {
                    // ... and remove it
                    fs::remove_file(&target).map_err(|e| {
                        let src_string = fc.source.to_string_lossy().into();
                        let dst_string = target.to_string_lossy().into();
                        Error::FailedToCreateTargetLink(src_string, dst_string, e)
                    })?;
                } else {
                    error!("Target {:?} already exists", target);
                    continue;
                }
            }
            // Get the temp path and remove garbage. This makes the path
            // absolute and removes redundent parts. This is necessary to
            // prevent bad and ugly links.
            let src: PathBuf = fc.temp_path(root).canonicalize().map_err(|e| {
                let src_string = fc.source.to_string_lossy().into();
                let dst_string = target.to_string_lossy().into();
                Error::FailedToCreateTargetLink(src_string, dst_string, e)
            })?;
            // Create a link from target to source
            info!("Linking {:?} to {:?}", src, target);
            linker(src, target)?;
        }

        Ok(())
    }
}

fn default_remove_instrucions() -> bool {
    true
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
fn construct_replacer(config: &Config) -> impl FnMut(&Captures) -> String {
    let replacements = config.substitutions.clone().unwrap_or(HashMap::new());
    move |captures| {
        let inner = &captures[2];
        match replacements.get(inner) {
            Some(repl) => format!("{}{}", expand(&captures[1]), repl),
            None => format!("{}{}", &captures[1], expand(inner)),
        }
    }
}

fn read_configuration_file(opt: &Opt) -> Result<Config> {
    let configuration_content =
        fs::read_to_string(&opt.config_file).map_err(Error::FailedToLoadConfiguration)?;
    toml::from_str(&configuration_content).map_err(Error::FailedToParseConfiguration)
}

fn main() {
    let opt = Opt::from_args();
    let config = match read_configuration_file(&opt) {
        Ok(config) => config.normalize(),
        Err(e) => {
            error!("{}", e);
            process::exit(1);
        }
    };
    if let Err(e) = config.preprocess_files(&opt) {
        error!("Preprocessing returned an error: {}", e);
    }
    if let Err(e) = config.link_files(&opt) {
        error!("Linking returned an error: {}", e);
    }
}
