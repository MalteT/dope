use lazy_static::lazy_static;
use regex::Captures;
use regex::Regex;
use serde::{Deserialize, Serialize};

use std::borrow::Cow;
use std::collections::HashSet;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::Substitutions;
use crate::env::{expand, expand_env_path};
use crate::error::{Error, Result};
use crate::helper::get_link_function;
use crate::parser::Command;
use crate::Opt;

type CmdLineSlice<'a, 'b> = &'a [(usize, Command<'b>)];
type ReadCmdReturn = Result<(usize, HashSet<usize>)>;

const COMPILED_SUFFIX: &'static str = ".preprocessed";

lazy_static! {
    static ref RE_IFDEF: Regex = Regex::new(r" *IFDEF +(.*)").unwrap();
    static ref RE_ELSE: Regex = Regex::new(r" *ELSE *").unwrap();
    static ref RE_ENDIF: Regex = Regex::new(r" *ENDIF *").unwrap();
}

/// Configuration of a single dotfile.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FileConfig {
    source: PathBuf,
    target: PathBuf,
    escape: Option<Escape>,
    prefix: Option<String>,
    remove_instructions: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Escape {
    pub start: String,
    pub end: String,
}

enum Answer {
    YesNo(bool),
    Option(usize),
}

fn ask_question<'a>(question: &'a str, options: Vec<Command<'a>>) -> Result<Answer> {
    if options.is_empty() {
        print!("ASK : {} (y/n) ", question);
        ::std::io::stdout().flush().unwrap();
        let mut input;
        let keep_block = loop {
            input = String::new();
            ::std::io::stdin()
                .read_line(&mut input)
                .map_err(Error::FailedToReadUserInput)?;
            input = input.trim_end_matches("\n").into();
            match input.as_ref() {
                "y" | "Y" => break true,
                "n" | "N" => break false,
                _ => {}
            }
        };
        Ok(Answer::YesNo(keep_block))
    } else {
        println!("ASK : {}", question);
        let mut idx = 1;
        for option in options {
            if let Command::Option(name) = option {
                println!("    : {:>2}> {}", idx, name);
            } else {
                panic!("BUG: ask_question received a non `Option` cmd");
            }
            idx += 1;
        }
        let mut input = String::new();
        // Get the user input until he succeds
        while usize::from_str_radix(&input, 10).is_err() {
            print!("    : Please enter a number: ");
            ::std::io::stdout().flush().unwrap();
            input = String::new();
            ::std::io::stdin()
                .read_line(&mut input)
                .map_err(Error::FailedToReadUserInput)?;
            input = input.trim_end_matches("\n").into();
        }
        let selection = usize::from_str_radix(&input, 10).unwrap() - 1;
        Ok(Answer::Option(selection))
    }
}

/// Read ASK command lines.
///
/// This method expects a slice of command lines. The first line is expected
/// to be a [`Command::Ask`]. On success a tuple containing the number of read lines
/// and a [`Vec`] of lines to skip is returned.
///
/// # Errors
/// This function will return an error, if an unexpected line is encountered. I.e. an
/// [`Else`](Command::Else) without an `if`.
///
/// # Panics
/// This function panics, if the first command in the given slice is not a [`Command::Ask`].
fn read_cmd_ask<'a, 'b>(cmds: CmdLineSlice<'a, 'b>) -> ReadCmdReturn {
    use Command::*;
    // Assert that the first line is an `Ask`
    if let (first_line, Ask(question)) = &cmds[0] {
        // Found option lines and their line nrs
        let mut options: Vec<Command> = vec![];
        let mut options_line_nrs: Vec<usize> = vec![];
        // Collected line skips
        let mut skips: HashSet<usize> = HashSet::new();
        // The current cmd line index
        let mut idx = 1;
        // Iterate over the remaining cmds
        while idx < cmds.len() {
            match cmds[idx].1 {
                // Handle all unexpected commands
                Ask(_) | EndIf | Else | Comment | IfDef(_) | IfNDef(_) | If(_, _) => {
                    // Read the current command and forward the error
                    let (cmds_handled, additional_skips) = read_cmd(&cmds[idx..])?;
                    // Skip all commands that have been handled by the read_cmd call
                    idx += cmds_handled;
                    // Add the skips to the collection
                    skips.extend(additional_skips.into_iter());
                }
                Option(name) => {
                    // We found an Option command. Add it to the collection
                    options.push(Option(name));
                    options_line_nrs.push(cmds[idx].0);
                    idx += 1;
                }
                EndAsk => {
                    // Everything has been handled, EndAsk was found
                    // Handle the user questioning
                    let answer = ask_question(question, options)?;
                    // Iterate over pairs of adjacent command lines. If the lines in between
                    // should be skipped, add them to the collected skips. Try not to add cmd lines
                    let mut last_cmd_line = first_line;
                    match answer {
                        // The question was considering a collection of options. `options_idx`
                        // is the index for the collected options lines .
                        Answer::Option(options_idx) => {
                            // Was the previous command line the selected option?
                            let mut skip_not = false;
                            for idx in 0..options_line_nrs.len() {
                                let cmd_line_nr = &options_line_nrs[idx];
                                if !skip_not {
                                    skips.extend(last_cmd_line + 1..*cmd_line_nr);
                                }
                                if idx == options_idx {
                                    skip_not = true;
                                } else {
                                    skip_not = false;
                                }
                                last_cmd_line = cmd_line_nr;
                            }
                            if ! skip_not {
                                skips.extend(last_cmd_line + 1..cmds[idx].0);
                            }
                        }
                        // The question was a simple yes-no-question. If `no` was answered,
                        // skip all lines between `Ask` and `EndAsk`
                        Answer::YesNo(keep) if !keep => {
                            skips.extend(first_line + 1..cmds[idx].0);
                        }
                        _ => {}
                    }
                    eprintln!("read_cmd_ask: {:?}", skips);
                    return Ok((idx + 1, skips));
                }
            }
        }
        Err(Error::MissingEndingInstruction(
            cmds[0].0,
            format!("{:?}", cmds[0].1),
        ))
    } else {
        panic!("BUG: read_cmd_ask called but no Ask found")
    }
}

fn read_cmd_ifdef<'a, 'b>(cmds: CmdLineSlice<'a, 'b>) -> ReadCmdReturn {
    use Command::*;
    if let (first_line, IfDef(var)) = &cmds[0] {
        let mut skips: HashSet<usize> = HashSet::new();
        let mut else_line = None;
        let mut idx = 1;
        while idx < cmds.len() {
            match cmds[idx].1 {
                EndAsk | Option(_) | Ask(_) | Comment | If(_, _) | IfNDef(_) | IfDef(_) => {
                    let (cmds_handled, additional_skips) = read_cmd(&cmds[idx..])?;
                    idx += cmds_handled;
                    skips.extend(additional_skips.into_iter());
                }
                Else => {
                    else_line = Some(cmds[idx].0);
                    idx += 1;
                }
                EndIf => {
                    match (evaluate_var(var), else_line.is_some()) {
                        (true, true) => skips.extend(else_line.unwrap() + 1..cmds[idx].0),
                        (true, false) => {}
                        (false, true) => skips.extend(first_line + 1..else_line.unwrap()),
                        (false, false) => skips.extend(first_line + 1..cmds[idx].0),
                    }
                    return Ok((idx + 1, skips));
                }
            }
        }
        Err(Error::MissingEndingInstruction(
            cmds[0].0,
            format!("{:?}", cmds[0].1),
        ))
    } else {
        panic!("BUG: read_cmd_ifdef called but no IfDef found")
    }
}

fn read_cmd_ifndef<'a, 'b>(cmds: CmdLineSlice<'a, 'b>) -> ReadCmdReturn {
    use Command::*;
    if let (first_line, IfNDef(var)) = &cmds[0] {
        let mut skips: HashSet<usize> = HashSet::new();
        let mut else_line = None;
        let mut idx = 1;
        while idx < cmds.len() {
            match cmds[idx].1 {
                EndAsk | Option(_) | Ask(_) | Comment | If(_, _) | IfNDef(_) | IfDef(_) => {
                    let (cmds_handled, additional_skips) = read_cmd(&cmds[idx..])?;
                    idx += cmds_handled;
                    skips.extend(additional_skips.into_iter());
                }
                Else => {
                    else_line = Some(cmds[idx].0);
                    idx += 1;
                }
                EndIf => {
                    match (evaluate_var(var), else_line.is_some()) {
                        (true, true) => skips.extend(first_line + 1..else_line.unwrap()),
                        (true, false) => skips.extend(first_line + 1..cmds[idx].0),
                        (false, true) => skips.extend(else_line.unwrap() + 1..cmds[idx].0),
                        (false, false) => {}
                    }
                    return Ok((idx + 1, skips));
                }
            }
        }
        Err(Error::MissingEndingInstruction(
            cmds[0].0,
            format!("{:?}", cmds[0].1),
        ))
    } else {
        panic!("BUG: read_cmd_ifndef called but no IfDef found")
    }
}

fn read_cmd_if<'a, 'b>(cmds: CmdLineSlice<'a, 'b>) -> ReadCmdReturn {
    use Command::*;
    if let (first_line, If(var1, var2)) = &cmds[0] {
        let mut skips: HashSet<usize> = HashSet::new();
        let mut else_line = None;
        let mut idx = 1;
        while idx < cmds.len() {
            match cmds[idx].1 {
                EndAsk | Option(_) | Ask(_) | Comment | If(_, _) | IfNDef(_) | IfDef(_) => {
                    let (cmds_handled, additional_skips) = read_cmd(&cmds[idx..])?;
                    idx += cmds_handled;
                    skips.extend(additional_skips.into_iter());
                }
                Else => {
                    else_line = Some(cmds[idx].0);
                    idx += 1;
                }
                EndIf => {
                    match (evaluate_expr(var1, var2), else_line.is_some()) {
                        (true, true) => skips.extend(else_line.unwrap() + 1..cmds[idx].0),
                        (true, false) => {}
                        (false, true) => skips.extend(first_line + 1..else_line.unwrap()),
                        (false, false) => skips.extend(first_line + 1..cmds[idx].0),
                    }
                    return Ok((idx + 1, skips));
                }
            }
        }
        Err(Error::MissingEndingInstruction(
            cmds[0].0,
            format!("{:?}", cmds[0].1),
        ))
    } else {
        panic!("BUG: read_cmd_if called but no If found")
    }
}

fn read_cmd<'a, 'b>(cmds: CmdLineSlice<'a, 'b>) -> ReadCmdReturn {
    use Command::*;
    match cmds
        .first()
        .expect("BUG: real_cmd called without commands")
        .1
    {
        IfDef(_) => read_cmd_ifdef(cmds),
        IfNDef(_) => read_cmd_ifndef(cmds),
        If(_, _) => read_cmd_if(cmds),
        Ask(_) => read_cmd_ask(cmds),
        Comment => Ok((1, HashSet::new())),
        Else | EndIf | Option(_) | EndAsk => {
            Err(Error::StrayCmdFound(cmds[0].0, format!("{:?}", cmds[0].1)))
        }
    }
}

/// Evaluate the given variable.
///
/// This returns true if the `var` contains more than just whitespaces
/// after expanding `${blub}`, `$blub` and `$(blub.sh)` stuff.
fn evaluate_var<'a>(var: &'a str) -> bool {
    !expand(var).trim().is_empty()
}

/// Evaluate the given expressions.
///
/// This returns true, if both `var`s are equal after expansion.
fn evaluate_expr<'a>(var1: &'a str, var2: &'a str) -> bool {
    let x = expand(var1).trim() == expand(var2).trim();
    eprintln!(
        "{:?} == {:?}? {}",
        expand(var1).trim(),
        expand(var2).trim(),
        x
    );
    x
}

impl FileConfig {
    /// Get the source path of the configuration.
    /// If source is relative, it will be interpreted as relative to the given root.
    /// If source is absolute, that path will be used.
    /// Environment variables in the path will be interpreted before
    /// determining whether the path is relative or not.
    pub fn source_path<P: AsRef<Path>>(&self, root: P) -> PathBuf {
        root.as_ref().join(expand_env_path(&self.source))
    }
    /// Get the content of the source file.
    /// See [`FileConfig::source_path`] for the path that will be read.
    pub fn source<P: AsRef<Path>>(&self, root: P) -> Result<String> {
        let source_path = self.source_path(root);
        fs::read_to_string(&source_path).map_err(|e| {
            let path_string = source_path.to_string_lossy().into();
            Error::FailedToReadSourceFile(path_string, e)
        })
    }
    /// Get the target path of the configuration.
    /// Behaves like [`FileConfig::source_path`] but returns the target path.
    pub fn target_path<P: AsRef<Path>>(&self, root: P) -> PathBuf {
        root.as_ref().join(expand_env_path(&self.target))
    }
    /// Get the temporary path for storing the preprocessed file.
    /// This will use the expanded source path (see [`FileConfig::source_path`])
    /// and append [`COMPILED_SUFFIX`].
    pub fn temp_path<P: AsRef<Path>>(&self, root: P) -> PathBuf {
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
    pub fn write_temp<P, S>(&self, root: P, content: S) -> Result<()>
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
    /// Get the regex for finding escapes in the source file.
    /// This method returns `None` if no escape sequence was specified.
    pub fn escape_regex(&self) -> Option<Regex> {
        self.escape
            .as_ref()
            .map(Escape::to_regex)
            .map(Result::unwrap)
    }
    /// Replace `None`s with the given defaults.
    /// Defined values (`Some`s) will not be changed.
    pub fn supplement(
        &mut self,
        escape: &Option<Escape>,
        remove_instructions: bool,
        prefix: &Option<String>,
    ) {
        if self.escape.is_none() {
            self.escape = escape.clone();
        }
        if self.remove_instructions.is_none() {
            self.remove_instructions = Some(remove_instructions);
        }
        if self.prefix.is_none() {
            self.prefix = prefix.clone();
        }
    }
    /// Preprocess this configuration file.
    ///
    /// 1) Replace substitutions.
    /// 2) Evaluate preprocessor instructions.
    /// 3) Write the file.
    pub fn preprocess(&self, substitutions: &Substitutions, opt: &Opt) -> Result<()> {
        // Root directory, config directory.
        let root = opt.config_file.parent().expect("No root directory found");
        info!("Preprocessing {:?}", self.source_path(root));
        // Read the file's contents
        let content = self.source(root)?;
        // Evaluate preprocessor instructions.
        let new_content = self.preprocess_instructions(&content)?;
        // Replace substitutions.
        let newest_content = self.preprocess_substitutions(&new_content, substitutions);
        // Write the preprocessed file.
        self.write_temp(root, newest_content)
    }
    /// Create a symbolic link from target to source.
    pub fn create_link(&self, opt: &Opt) -> Result<()> {
        let mut linker = get_link_function();
        let root = opt.config_file.parent().expect("No root found");
        // Expand environment variables in the paths
        let target_path = self.target_path(root);
        let source_path = self.source_path(root);
        // If the target already exists...
        if target_path.exists() {
            // Verify, that it's just a link...
            let target_md = fs::symlink_metadata(&target_path)
                .map_err(|e| Error::as_failed_link(&source_path, &target_path, e))?;
            if target_md.file_type().is_symlink() {
                // ... and remove it
                fs::remove_file(&target_path)
                    .map_err(|e| Error::as_failed_link(&source_path, &target_path, e))?;
            } else {
                // If it's not a symlink, we should not delete it
                return Err(Error::TargetAlreadyExists(target_path));
            }
        }
        // Get the temp path and remove garbage. This makes the path
        // absolute and removes redundent parts. This is necessary to
        // prevent bad and ugly links.
        let source_path: PathBuf = self
            .temp_path(root)
            .canonicalize()
            .map_err(|e| Error::as_failed_link(&source_path, &target_path, e))?;
        // Create a link from target to source
        info!("Linking {:?} to {:?}", &source_path, &target_path);
        linker(source_path, target_path)?;
        Ok(())
    }
    /// Preprocess substitutions.
    /// Assuming the escape sequences `{++` and `++}` are used. This function replaces
    /// all occurences of `{++KEY++}` with the `VALUE` defined in the given
    /// [`Substitutions`]. The returned content is unaltered, if no escape sequences
    /// are defined, or no usage is found in the given `content`.
    fn preprocess_substitutions<'a>(
        &self,
        content: &'a str,
        substitutions: &Substitutions,
    ) -> Cow<'a, str> {
        // Create a replacer for regex replacements
        let replacer = construct_replacer(substitutions);
        // Get the regex specified explicitly for this file configuration
        let regex = self.escape_regex();
        // Only if we have a regex to work with
        if let Some(regex) = regex {
            // Create the final file content by replacing stuff
            regex.replace_all(content.as_ref(), replacer)
        } else {
            // If no regex is given, inform the user
            info!("No escape characters defined, no substitution will be made");
            Cow::from(content)
        }
    }
    /// Preprocess instructions
    fn preprocess_instructions<'a>(&self, content: &'a str) -> Result<Cow<'a, str>> {
        let prefix = match self.prefix.as_ref() {
            Some(prefix) => prefix,
            None => {
                // Do nothing
                return Ok(Cow::from(content));
            }
        };
        let (mut cmd_lines, mut errors): (Vec<_>, Vec<_>) = content
            .lines()
            .enumerate()
            .filter_map(|(line_nr, line)| {
                Command::parse_from_line(prefix, line).map(|res| (line_nr, res))
            })
            .partition(|(_, res)| res.is_ok());
        if errors.len() > 0 {
            return errors.remove(0).1.map(|_| Cow::from(content));
        }
        let cmd_lines: Vec<_> = cmd_lines
            .drain(..)
            .map(|(line_nr, res)| (line_nr, res.unwrap()))
            .collect();
        let mut idx = 0;
        let mut skips = vec![];
        while idx < cmd_lines.len() {
            let (handled_lines, new_skips) = read_cmd(&cmd_lines[idx..])?;
            skips.extend(new_skips.into_iter());
            idx += handled_lines;
        }
        // Add command lines to skip if necessary
        if self.remove_instructions.expect("Default") {
            let mut cmd_line_nrs: Vec<_> = cmd_lines.iter().map(|(line_nr, _)| *line_nr).collect();
            skips.append(&mut cmd_line_nrs);
        }
        if skips.is_empty() {
            Ok(Cow::from(content))
        } else {
            let remaining_lines: Vec<_> = content
                .lines()
                .enumerate()
                .filter(|(line_nr, _)| !skips.contains(line_nr))
                .map(|(_, line)| line)
                .collect();
            // TODO: Plattform independet line endings
            Ok(remaining_lines.join("\n").into())
        }
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

/// Create a [`regex::Replacer`] for the given substitutions. This replacer
/// can then be used to replace instances found by the regular expression
/// created by any [`Escape::to_regex`].
fn construct_replacer<'a>(
    substitutions: &'a Substitutions,
) -> impl FnMut(&Captures) -> String + 'a {
    move |captures| {
        let inner = &captures[2];
        match substitutions.get(inner) {
            Some(repl) => format!("{}{}", expand(&captures[1]), repl),
            None => format!("{}{}", &captures[1], expand(inner)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::set_var;

    #[test]
    fn test_read_cmd_ifdef() {
        use Command::*;
        // With else branch
        let cmds = &[(1, IfDef("$ULTRA_LONG_VARIABLE")), (5, Else), (10, EndIf)];

        set_var("ULTRA_LONG_VARIABLE", "1");
        let (lines_read, skips) = read_cmd_ifdef(cmds).expect("Should work");
        assert_eq!(lines_read, 3);
        assert_eq!(skips, vec![6, 7, 8, 9].drain(..).collect());

        set_var("ULTRA_LONG_VARIABLE", " \t ");
        let (lines_read, skips) = read_cmd_ifdef(cmds).expect("Should work");
        assert_eq!(lines_read, 3);
        assert_eq!(skips, vec![2, 3, 4].drain(..).collect());

        // Without else branch
        let cmds = &[(1, IfDef("$ULTRA_LONG_VARIABLE")), (7, EndIf)];

        set_var("ULTRA_LONG_VARIABLE", "some value");
        let (lines_read, skips) = read_cmd_ifdef(cmds).expect("Should work");
        assert_eq!(lines_read, 2);
        assert_eq!(skips, HashSet::new());

        set_var("ULTRA_LONG_VARIABLE", " \t ");
        let (lines_read, skips) = read_cmd_ifdef(cmds).expect("Should work");
        assert_eq!(lines_read, 2);
        assert_eq!(skips, vec![2, 3, 4, 5, 6].drain(..).collect());

        // With no line in between
        let cmds = &[(1, IfDef("$ULTRA_LONG_VARIABLE")), (2, EndIf)];

        set_var("ULTRA_LONG_VARIABLE", "some value");
        let (lines_read, skips) = read_cmd_ifdef(cmds).expect("Should work");
        assert_eq!(lines_read, 2);
        assert_eq!(skips, HashSet::new());

        set_var("ULTRA_LONG_VARIABLE", " \t ");
        let (lines_read, skips) = read_cmd_ifdef(cmds).expect("Should work");
        assert_eq!(lines_read, 2);
        assert_eq!(skips, HashSet::new());
    }

    /// Test basic `read_cmd_if` stuff.
    #[test]
    fn test_read_cmd_if() {
        use Command::*;
        // With Else branch
        let cmds = &[
            (3, If("$ULTRA_LONG_VARIABLE", "SHORT_VALUE")),
            (6, Else),
            (11, EndIf),
        ];

        set_var("ULTRA_LONG_VARIABLE", "NOT_SHORT_VALUE");
        let (lines_read, skips) = read_cmd_if(cmds).expect("Should work");
        assert_eq!(lines_read, 3);
        assert_eq!(skips, vec![4, 5].drain(..).collect());

        set_var("ULTRA_LONG_VARIABLE", "\tSHORT_VALUE ");
        let (lines_read, skips) = read_cmd_if(cmds).expect("Should work");
        assert_eq!(lines_read, 3);
        assert_eq!(skips, vec![7, 8, 9, 10].drain(..).collect());

        // Without Else branch
        let cmds = &[(4, If("$ULTRA_LONG_VARIABLE", "öüä@")), (8, EndIf)];

        set_var("ULTRA_LONG_VARIABLE", "something else");
        let (lines_read, skips) = read_cmd_if(cmds).expect("Should work");
        assert_eq!(lines_read, 2);
        assert_eq!(skips, vec![5, 6, 7].drain(..).collect());

        set_var("ULTRA_LONG_VARIABLE", "öüä@");
        let (lines_read, skips) = read_cmd_if(cmds).expect("Should work");
        assert_eq!(lines_read, 2);
        assert_eq!(skips, HashSet::new());

        // With no lines in between
        let cmds = &[(5, If("$ULTRA_LONG_VARIABLE", "öüä@")), (6, EndIf)];

        set_var("ULTRA_LONG_VARIABLE", "something else");
        let (lines_read, skips) = read_cmd_if(cmds).expect("Should work");
        assert_eq!(lines_read, 2);
        assert_eq!(skips, HashSet::new());

        set_var("ULTRA_LONG_VARIABLE", "\t öüä@ ");
        let (lines_read, skips) = read_cmd_if(cmds).expect("Should work");
        assert_eq!(lines_read, 2);
        assert_eq!(skips, HashSet::new());
    }
}
