use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::fs;
use std::path::{Path};

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
    pub file_configurations: Vec<FileConfig>,
    pub substitutions: Substitutions,
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
    substitutions: Option<Substitutions>,
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
        // Iterate over all config file entries
        for fc in &self.file_configurations {
            match fc.preprocess(&self.substitutions, opt) {
                Ok(_) => {},
                Err(e) => error!("{}", e),
            }
        }
        Ok(())
    }
    /// Link all temporary files to their final destination.
    pub fn link_files(&self, opt: &Opt) -> Result<()> {
        // Iterate over all file configurations
        for fc in &self.file_configurations {
            match fc.create_link(opt) {
                Ok(_) => {},
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
