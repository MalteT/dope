use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::fs;
use std::path::Path;

mod file_config;

use crate::error::{Error, Result};
use crate::Opt;
use file_config::{Escape, FileConfig};

pub type Substitutions = HashMap<String, String>;

/// The complete, normalized configuration file.
///
/// All unset options of file_configurations have been filled with default options,
/// if those were defined.
#[derive(Debug)]
pub struct Config {
    /// The list of files to process.
    pub file_configurations: Vec<FileConfig>,
    /// The list of global substitutions.
    pub substitutions: Substitutions,
}

/// The raw, loaded TOML configuration file.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RawConfig {
    /// Default escape surrounding substitutions.
    default_escape: Option<Escape>,
    /// Default line prefix for commands.
    default_prefix: Option<String>,
    /// Default value for removing commands. If true, commands
    /// will be cut from the output file. Defaults to true.
    #[serde(default = "default_true")]
    default_remove_instructions: bool,
    /// The list of files to process.
    #[serde(default, rename = "config")]
    file_configurations: Vec<FileConfig>,
    /// The list of global substitutions.
    #[serde(default)]
    substitutions: Option<Substitutions>,
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
    /// Process all files.
    ///
    /// This will execute all preprocessing instructions and link the output file.
    pub fn process_files(&self, opt: &Opt) -> Result<()> {
        // Iterate over all config file entries
        for fc in &self.file_configurations {
            // Preprocess the current file
            match fc.preprocess(&self.substitutions, opt) {
                // Link the current file
                Ok(_) => match fc.create_link(opt) {
                    Ok(_) => {}
                    Err(e) => error!("{}", e),
                },
                Err(e) => error!("{}", e),
            }
        }
        Ok(())
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
        // Return a real config
        Config {
            file_configurations,
            substitutions,
        }
    }
}

fn default_true() -> bool {
    true
}
