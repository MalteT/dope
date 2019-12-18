//! Handle environment variables
//!
//! **Note**: Supports Unix, nothing else...
//!
//! Environment variables can be used when replacing stuff. They may also appear
//! in paths in the configuration.
//!
//! # Unix
//! To use an environment variable, one can use `$YOUR_ENV_VAR` or `${YOUR_ENV_VAR}`.
//! `YOUR_ENV_VAR` may only contain the characters `a-z`, `A-Z` and `_` (Underscore).
//! **Note**: All used variables must expand to valid Unicode!
use lazy_static::lazy_static;
use regex::{Captures, Regex};

use std::borrow::Cow;
use std::env::var as resolve_env;
use std::env::VarError;
use std::path::{Path, PathBuf};
use std::process::Command;

lazy_static! {
    static ref RE_DOLLAR: Regex = Regex::new(r"([^\\]|^)\$([a-zA-Z_]+)").unwrap();
    static ref RE_DOLLAR_BRACES: Regex = Regex::new(r"([^\\]|^)\$\{([a-zA-Z_]+)\}").unwrap();
    static ref RE_DOLLAR_PARENS: Regex = Regex::new(r"([^\\]|^)\$\((.+?[^\\])\)").unwrap();
}

pub fn expand<'a>(s: &'a str) -> String {
    let s = expand_subst(s);
    expand_env(&s)
}

pub fn expand_env<'a>(s: &'a str) -> String {
    let simples_expanded = RE_DOLLAR.replace_all(s.as_ref(), env_replacer());
    let all_envs_expanded = RE_DOLLAR_BRACES.replace_all(&simples_expanded, env_replacer());
    all_envs_expanded.as_ref().to_owned()
}

pub fn expand_subst<'a>(s: &'a str) -> Cow<'a, str> {
    RE_DOLLAR_PARENS.replace_all(s, subst_replacer())
}

pub fn expand_env_path<'a>(p: &'a Path) -> PathBuf {
    let s = p.to_string_lossy();
    expand_env(&s).into()
}

fn env_replacer() -> impl FnMut(&Captures) -> String {
    |captures| {
        let key = &captures[2];
        let repl = match resolve_env(key) {
            Ok(repl) => repl,
            Err(VarError::NotPresent) => String::new(),
            Err(VarError::NotUnicode(_)) => {
                warn!("{:?} does not contain valid unicode", key);
                String::new()
            }
        };

        format!("{}{}", &captures[1], &repl)
    }
}

fn subst_replacer() -> impl FnMut(&Captures) -> String {
    |captures| {
        let prefix = &captures[1];
        let command = &captures[2];
        let output = if cfg!(unix) {
            Command::new("sh")
                .arg("-c")
                .arg(command)
                .output()
                // TODO
                .expect("failed to execute process")
        } else {
            Command::new("cmd")
                .arg("/C")
                .arg(command)
                .output()
                // TODO
                .expect("failed to execute process")
        };
        if output.status.success() {
            let output = String::from_utf8_lossy(&output.stdout);
            format!("{}{}", prefix, output.trim_end_matches("\n"))
        } else {
            // TODO
            warn!("Process {:?} exited abnormally", command);
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn test_expand_env() {
        ::std::env::set_var("DOTFILE_TESTING_STUFF", "FUBAR");
        assert_eq!(expand_env("$DOTFILE_TESTING_STUFF"), "FUBAR");
        assert_eq!(expand_env(" $DOTFILE_TESTING_STUFF "), " FUBAR ");
        assert_eq!(
            expand_env(" $SOME_VERY_UNLIKELY_VARIABLE_THAT_COULD_DESTROY_THIS_TEST "),
            "  "
        );
        assert_eq!(expand_env(r"\$HOME"), r"\$HOME");
    }

    #[cfg(unix)]
    #[test]
    fn test_expand_subst() {
        assert_eq!(expand_subst("$(echo 'Hello World')"), "Hello World");
        assert_eq!(expand_subst(" $(echo 'Hello World') "), " Hello World ");
        assert_eq!(expand_subst(" $(echo -n 'Hello World') "), " Hello World ");
        assert_eq!(
            expand_subst(" \\$(echo -n 'Hello World') "),
            " \\$(echo -n 'Hello World') "
        );
    }
}
