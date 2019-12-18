use colored::Colorize;

use std::collections::HashMap;
use std::collections::HashSet;
use std::io::Write;

use crate::command::Command;
use crate::env::expand;
use crate::error::{Error, Result};

/// A slice containing commands.
///
/// The slice contains tuples with a line number and a [`Command`].
type CmdLineSlice<'bor, 'str> = &'bor [(usize, Command<'str>)];

/// An answer to a question.
///
/// Used for the [`Ask`](Command::Ask).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Answer {
    YesNo(bool),
    Option(usize),
}

/// Reader of commands!
pub struct CommandReader<'bor, 'str> {
    idx: usize,
    skips: HashSet<usize>,
    cmds: CmdLineSlice<'bor, 'str>,
    answered_questions: HashMap<(&'str str, Vec<Command<'str>>), Answer>,
}

impl<'bor, 'str> CommandReader<'bor, 'str> {
    /// Read and evaluate the given commands.
    ///
    /// # Returns
    /// A set of lines to skip.
    ///
    /// # Errors
    /// This will return an error, if an unexpected command is found,
    /// i.e. an EndIf without a starting if, or a closing command is missing. I.e.
    /// an Ask without an EndAsk
    pub fn read(cmds: CmdLineSlice<'bor, 'str>) -> Result<HashSet<usize>> {
        let mut cr = Self::new(cmds);
        while cr.idx < cmds.len() {
            cr.read_cmd()?;
        }
        Ok(cr.skips)
    }
    /// Create a new CommandReader, that will read the given commands.
    fn new(cmds: CmdLineSlice<'bor, 'str>) -> Self {
        CommandReader {
            idx: 0,
            skips: HashSet::new(),
            cmds,
            answered_questions: HashMap::new(),
        }
    }
    /// Ask the user the given question.
    ///
    /// If the question has already been asked. The cached
    /// answer will be returned without bothering the user.
    fn ask_question(&mut self, question: &'str str, options: Vec<Command<'str>>) -> Result<Answer> {
        match self.answered_questions.get(&(question, options.clone())) {
            Some(cached_answer) => Ok(*cached_answer),
            None => {
                if options.is_empty() {
                    print!("ASK  ─ {} (y/n) ", question);
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
                    self.answered_questions
                        .insert((question, options), Answer::YesNo(keep_block));
                    Ok(Answer::YesNo(keep_block))
                } else {
                    println!("ASK  ┬ {}", question);
                    let mut idx = 1;
                    for option in &options {
                        if let Command::Option(name) = option {
                            println!("     │ {:>2}> {}", idx.to_string().bold(), name);
                        } else {
                            panic!("BUG: ask_question received a non `Option` cmd");
                        }
                        idx += 1;
                    }
                    let mut input = String::new();
                    // Get the user input until he succeds
                    while usize::from_str_radix(&input, 10).is_err() {
                        print!("     └ Please enter a number: ");
                        ::std::io::stdout().flush().unwrap();
                        input = String::new();
                        ::std::io::stdin()
                            .read_line(&mut input)
                            .map_err(Error::FailedToReadUserInput)?;
                        input = input.trim_end_matches("\n").into();
                    }
                    let selection = usize::from_str_radix(&input, 10).unwrap() - 1;
                    self.answered_questions
                        .insert((question, options), Answer::Option(selection));
                    Ok(Answer::Option(selection))
                }
            }
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
    fn read_cmd_ask<'b>(&mut self) -> Result<()> {
        use Command::*;
        let first_idx = self.idx;
        // Assert that the first line is an `Ask`
        if let (first_line, Ask(question)) = &self.cmds[self.idx] {
            self.idx += 1;
            // Found option lines and their line nrs
            let mut options: Vec<Command> = vec![];
            let mut options_line_nrs: Vec<usize> = vec![];
            // Iterate over the remaining cmds
            while self.idx < self.cmds.len() {
                match self.cmds[self.idx].1 {
                    // Handle all unexpected commands
                    Ask(_) | EndIf | Else | Comment | IfDef(_) | IfNDef(_) | If(_, _) => {
                        // Read the current command and forward the error
                        self.read_cmd()?;
                    }
                    Option(name) => {
                        // We found an Option command. Add it to the collection
                        options.push(Option(name));
                        options_line_nrs.push(self.cmds[self.idx].0);
                        self.idx += 1;
                    }
                    EndAsk => {
                        // Everything has been handled, EndAsk was found
                        // Handle the user questioning
                        let answer = self.ask_question(question, options)?;
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
                                        self.skips.extend(last_cmd_line + 1..*cmd_line_nr);
                                    }
                                    if idx == options_idx {
                                        skip_not = true;
                                    } else {
                                        skip_not = false;
                                    }
                                    last_cmd_line = cmd_line_nr;
                                }
                                if !skip_not {
                                    self.skips.extend(last_cmd_line + 1..self.cmds[self.idx].0);
                                }
                            }
                            // The question was a simple yes-no-question. If `no` was answered,
                            // skip all lines between `Ask` and `EndAsk`
                            Answer::YesNo(keep) if !keep => {
                                self.skips.extend(first_line + 1..self.cmds[self.idx].0);
                            }
                            _ => {}
                        }
                        self.idx += 1;
                        return Ok(());
                    }
                }
            }
            Err(Error::MissingEndingInstruction(
                self.cmds[first_idx].0,
                format!("{:?}", self.cmds[first_idx].1),
            ))
        } else {
            panic!("BUG: read_cmd_ask called but no Ask found")
        }
    }

    fn read_cmd_ifdef<'b>(&mut self) -> Result<()> {
        use Command::*;
        let first_idx = self.idx;
        if let (first_line, IfDef(var)) = &self.cmds[self.idx] {
            self.idx += 1;
            let mut else_line = None;
            while self.idx < self.cmds.len() {
                match self.cmds[self.idx].1 {
                    EndAsk | Option(_) | Ask(_) | Comment | If(_, _) | IfNDef(_) | IfDef(_) => {
                        self.read_cmd()?;
                    }
                    Else => {
                        else_line = Some(self.cmds[self.idx].0);
                        self.idx += 1;
                    }
                    EndIf => {
                        match (evaluate_var(var), else_line) {
                            (true, Some(el)) => self.skips.extend(el + 1..self.cmds[self.idx].0),
                            (true, None) => {}
                            (false, Some(el)) => self.skips.extend(first_line + 1..el),
                            (false, None) => {
                                self.skips.extend(first_line + 1..self.cmds[self.idx].0)
                            }
                        }
                        self.idx += 1;
                        return Ok(());
                    }
                }
            }
            Err(Error::MissingEndingInstruction(
                self.cmds[first_idx].0,
                format!("{:?}", self.cmds[first_idx].1),
            ))
        } else {
            panic!("BUG: read_cmd_ifdef called but no IfDef found")
        }
    }

    fn read_cmd_ifndef<'b>(&mut self) -> Result<()> {
        use Command::*;
        let first_idx = self.idx;
        if let (first_line, IfNDef(var)) = &self.cmds[self.idx] {
            self.idx += 1;
            let mut else_line = None;
            while self.idx < self.cmds.len() {
                match self.cmds[self.idx].1 {
                    EndAsk | Option(_) | Ask(_) | Comment | If(_, _) | IfNDef(_) | IfDef(_) => {
                        self.read_cmd()?;
                    }
                    Else => {
                        else_line = Some(self.cmds[self.idx].0);
                        self.idx += 1;
                    }
                    EndIf => {
                        match (evaluate_var(var), else_line.is_some()) {
                            (true, true) => self.skips.extend(first_line + 1..else_line.unwrap()),
                            (true, false) => {
                                self.skips.extend(first_line + 1..self.cmds[self.idx].0)
                            }
                            (false, true) => self
                                .skips
                                .extend(else_line.unwrap() + 1..self.cmds[self.idx].0),
                            (false, false) => {}
                        }
                        self.idx += 1;
                        return Ok(());
                    }
                }
            }
            Err(Error::MissingEndingInstruction(
                self.cmds[first_idx].0,
                format!("{:?}", self.cmds[first_idx].1),
            ))
        } else {
            panic!("BUG: read_cmd_ifndef called but no IfDef found")
        }
    }

    fn read_cmd_if<'b>(&mut self) -> Result<()> {
        use Command::*;
        let first_idx = self.idx;
        if let (first_line, If(var1, var2)) = &self.cmds[self.idx] {
            let mut else_line = None;
            self.idx += 1;
            while self.idx < self.cmds.len() {
                match self.cmds[self.idx].1 {
                    EndAsk | Option(_) | Ask(_) | Comment | If(_, _) | IfNDef(_) | IfDef(_) => {
                        self.read_cmd()?;
                    }
                    Else => {
                        else_line = Some(self.cmds[self.idx].0);
                        self.idx += 1;
                    }
                    EndIf => {
                        match (evaluate_expr(var1, var2), else_line) {
                            (true, Some(el)) => self.skips.extend(el + 1..self.cmds[self.idx].0),
                            (true, None) => {}
                            (false, Some(el)) => self.skips.extend(first_line + 1..el),
                            (false, None) => {
                                self.skips.extend(first_line + 1..self.cmds[self.idx].0)
                            }
                        }
                        self.idx += 1;
                        return Ok(());
                    }
                }
            }
            Err(Error::MissingEndingInstruction(
                self.cmds[first_idx].0,
                format!("{:?}", self.cmds[first_idx].1),
            ))
        } else {
            panic!("BUG: read_cmd_if called but no If found")
        }
    }
    fn read_comment(&mut self) -> Result<()> {
        self.idx += 1;
        Ok(())
    }
    fn read_cmd<'b>(&mut self) -> Result<()> {
        use Command::*;
        match self.cmds[self.idx].1 {
            IfDef(_) => self.read_cmd_ifdef(),
            IfNDef(_) => self.read_cmd_ifndef(),
            If(_, _) => self.read_cmd_if(),
            Ask(_) => self.read_cmd_ask(),
            Comment => self.read_comment(),
            Else | EndIf | Option(_) | EndAsk => Err(Error::StrayCmdFound(
                self.cmds[self.idx].0,
                format!("{:?}", self.cmds[self.idx].1),
            )),
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
    expand(var1).trim() == expand(var2).trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_read_ifdef() {
        use Command::*;
        // With else branch
        let cmds = &[(1, IfDef("NOT_EMPTY_STRING")), (5, Else), (10, EndIf)];
        let mut reader = CommandReader::new(cmds);
        reader.read_cmd_ifdef().expect("Should work");
        assert_eq!(reader.idx, 3);
        assert_eq!(reader.skips, vec![6, 7, 8, 9].drain(..).collect());

        // Without else branch
        let cmds = &[(1, IfDef("ULTRA_LONG_VARIABLE")), (7, EndIf)];
        let mut reader = CommandReader::new(cmds);
        reader.read_cmd_ifdef().expect("Should work");
        assert_eq!(reader.idx, 2);
        assert_eq!(reader.skips, HashSet::new());

        // With no line in between
        let cmds = &[(1, IfDef("NOT_EMPTY_STRING")), (2, EndIf)];
        let mut reader = CommandReader::new(cmds);
        reader.read_cmd_ifdef().expect("Should work");
        assert_eq!(reader.idx, 2);
        assert_eq!(reader.skips, HashSet::new());
    }

    /// Test basic `read_cmd_if` stuff.
    #[test]
    fn test_command_read_if() {
        use Command::*;
        // With Else branch
        let cmds = &[
            (3, If("SHORT_VALUE", "SHORT_VALUE")),
            (6, Else),
            (11, EndIf),
        ];
        let mut reader = CommandReader::new(cmds);
        reader.read_cmd_if().expect("Should work");
        assert_eq!(reader.idx, 3);
        assert_eq!(reader.skips, vec![7, 8, 9, 10].drain(..).collect());

        // Without Else branch
        let cmds = &[(4, If("öüä@", "öüä@")), (8, EndIf)];
        let mut reader = CommandReader::new(cmds);
        reader.read_cmd_if().expect("Should work");
        assert_eq!(reader.idx, 2);
        assert_eq!(reader.skips, HashSet::new());

        // With no lines in between
        let cmds = &[(5, If("öüä@", "öüä@")), (6, EndIf)];
        let mut reader = CommandReader::new(cmds);
        reader.read_cmd_if().expect("Should work");
        assert_eq!(reader.idx, 2);
        assert_eq!(reader.skips, HashSet::new());
    }

    #[test]
    fn test_command_read() {
        use Command::*;

        let cmds = &[
            (1, IfDef("SHORT_VALUE")),
            (4, Else),
            (6, EndIf),
            (8, If("SOME", "SOME1")),
            (10, EndIf),
        ];
        let skips = CommandReader::read(cmds).expect("Should work");
        assert_eq!(skips, vec![5, 9].drain(..).collect())
    }
}
