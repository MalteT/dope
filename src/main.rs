use structopt::StructOpt;

use std::path::PathBuf;
use std::process;

#[macro_use]
mod logging;
mod config;
mod env;
mod error;
mod helper;
mod command;
mod command_reader;

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
    // Load CLI options
    let opt = Opt::from_args();
    // Load TOML configuration file
    let config = match Config::load(&opt.config_file) {
        Ok(config) => config,
        Err(e) => {
            error!("{}", e);
            process::exit(1);
        }
    };
    // Process files
    // All errors should have already been reported at this point
    if let Err(_) = config.process_files(&opt) {
        process::exit(1);
    }
}
