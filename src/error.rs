use toml::de::Error as TomlDeError;
use failure::Fail;
use regex::Error as RegexError;

use std::io::Error as IOError;

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
