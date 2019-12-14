use failure::Fail;
use regex::Error as RegexError;
use toml::de::Error as TomlDeError;

use std::io::Error as IOError;
use std::path::Path;

pub type Result<T> = ::std::result::Result<T, Error>;

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "Failed to load configuration file: {}", _0)]
    FailedToLoadConfiguration(#[cause] IOError),
    #[fail(display = "Failed to parse configuration file: {}", _0)]
    FailedToParseConfiguration(#[cause] TomlDeError),
    #[fail(display = "Failed to parse regex: {}", _0)]
    FailedToParseRegex(#[cause] RegexError),
    #[fail(display = "Failed to read source file {:?}: {}", _0, _1)]
    FailedToReadSourceFile(String, #[cause] IOError),
    #[fail(display = "Failed to open temp file {:?}: {}", _0, _1)]
    FailedToOpenTempFile(String, #[cause] IOError),
    #[fail(display = "Failed to write temp file {:?}: {}", _0, _1)]
    FailedToWriteTempFile(String, #[cause] IOError),
    #[fail(
        display = "Failed to create link {:?}, pointing to {:?}: {}",
        _1, _0, _2
    )]
    FailedToCreateTargetLink(String, String, #[cause] IOError),
}

impl Error {
    pub fn as_load_config(ioe: IOError) -> Self {
        Error::FailedToLoadConfiguration(ioe)
    }
    pub fn as_failed_link<P1, P2>(src: P1, dst: P2, ioe: IOError) -> Self
    where
        P1: AsRef<Path>,
        P2: AsRef<Path>,
    {
        let src = src.as_ref().to_string_lossy();
        let dst = dst.as_ref().to_string_lossy();
        Error::FailedToCreateTargetLink(src.as_ref().into(), dst.as_ref().into(), ioe)
    }
}
