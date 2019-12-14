use colored::Colorize;
use structopt::StructOpt;

use std::path::PathBuf;
use std::process;

#[macro_use]
mod logging;
mod config;
mod env;
mod error;
mod expand;
mod helper;

use config::Config;

#[derive(StructOpt, Debug)]
#[structopt(name = "dotfile-preprocessor")]
pub struct Opt {
    /// Specify the TOML configuration file.
    #[structopt(
        long = "config",
        short,
        default_value = "./preprocessor.toml",
        hide_default_value = true
    )]
    config_file: PathBuf,
    /// Panic on the first error, instead of continuing with the next configuration file.
    #[structopt(long, short)]
    panic: bool,
}

fn main() {
    let opt = Opt::from_args();
    let config = match Config::load(&opt.config_file) {
        Ok(config) => config,
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
