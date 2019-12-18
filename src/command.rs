//! Parsing module for [`Command`]s.

use nom::branch::alt;
use nom::bytes::complete::{is_not, tag, tag_no_case, take_until};
use nom::combinator::{map, value};
use nom::multi::{many0, many1};
use nom::sequence::{terminated, tuple};
use nom::{error::ErrorKind, Err, IResult, Needed};

use crate::error::{Error, Result};

type In<'a> = &'a str;
type Out<'a> = IResult<&'a str, &'a str>;
type CmdOut<'a> = IResult<&'a str, Command<'a>>;
type Var<'a> = &'a str;

/// All possible preprocessor commands.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Command<'a> {
    IfDef(Var<'a>),
    IfNDef(Var<'a>),
    If(Var<'a>, Var<'a>),
    Else,
    EndIf,
    Ask(Var<'a>),
    Option(Var<'a>),
    EndAsk,
    Comment,
}

impl<'a> Command<'a> {
    /// Parse a command from the given line.
    ///
    /// # Returns
    /// - `None`, if the input does not start with `prefix`,
    /// - `Some(cmd)`, if the parsing was successful.
    ///
    /// # Errors
    /// If the line starts with `prefix`, but does not parse
    /// successfully an [`Error`] is returned.
    pub fn parse_from_line(prefix: In<'a>, input: In<'a>) -> Option<Result<Self>> {
        let prefix = terminated(tag_from_prefix(prefix), ws_star);
        match prefix(input) {
            Ok((rest, _)) => match parse_command(rest) {
                Ok((_, cmd)) => Some(Ok(cmd)),
                Err(_) => Some(Err(Error::UnrecognizedPreprocessorInstruction(rest.into()))),
            },
            Err(_) => None,
        }
    }
}

fn ws<'a>(input: In<'a>) -> Out<'a> {
    alt((tag(" "), tag("\t")))(input)
}

fn ws_plus<'a>(input: In<'a>) -> Out<'a> {
    value(" ", many1(ws))(input)
}

fn ws_star<'a>(input: In<'a>) -> Out<'a> {
    value(" ", many0(ws))(input)
}

fn tag_from_prefix<'a>(prefix: In<'a>) -> impl Fn(&'a str) -> Out<'a> {
    move |input| {
        if input.starts_with(prefix) {
            Ok((&input[prefix.len()..], prefix))
        } else if input.len() < prefix.len() {
            if prefix.starts_with(input) {
                Err(Err::Incomplete(Needed::Size(prefix.len())))
            } else {
                Err(Err::Error((input, ErrorKind::Tag)))
            }
        } else {
            Err(Err::Error((input, ErrorKind::Tag)))
        }
    }
}

fn rest<'a>(input: In<'a>) -> Out<'a> {
    is_not("\r\n")(input)
}

fn cmd_ifdef<'a>(input: In<'a>) -> CmdOut<'a> {
    let tag_ifdef = tag_no_case("IFDEF");
    map(tuple((tag_ifdef, ws_plus, rest)), |(_, _, var)| {
        Command::IfDef(var)
    })(input)
}

fn cmd_ifndef<'a>(input: In<'a>) -> CmdOut<'a> {
    let tag_ifndef = tag_no_case("IFNDEF");
    map(tuple((tag_ifndef, ws_plus, rest)), |(_, _, var)| {
        Command::IfNDef(var)
    })(input)
}

fn cmd_if<'a>(input: In<'a>) -> CmdOut<'a> {
    let tag_if = tag_no_case("IF");
    let tag_equals = tag("==");
    let until_equals = take_until("==");
    map(
        tuple((
            tag_if,
            ws_plus,
            until_equals,
            ws_star,
            tag_equals,
            ws_star,
            rest,
        )),
        |(_, _, var1, _, _, _, var2)| Command::If(var1.trim(), var2.trim()),
    )(input)
}

fn cmd_else<'a>(input: In<'a>) -> CmdOut<'a> {
    value(Command::Else, tag_no_case("ELSE"))(input)
}

fn cmd_endif<'a>(input: In<'a>) -> CmdOut<'a> {
    value(Command::EndIf, tag_no_case("ENDIF"))(input)
}

fn cmd_ask<'a>(input: In<'a>) -> CmdOut<'a> {
    let tag_ask = tag_no_case("ASK");
    map(tuple((tag_ask, ws_plus, rest)), |(_, _, question)| {
        Command::Ask(question)
    })(input)
}

fn cmd_option<'a>(input: In<'a>) -> CmdOut<'a> {
    let tag_option = tag_no_case("OPTION");
    map(tuple((tag_option, ws_plus, rest)), |(_, _, option)| {
        Command::Option(option)
    })(input)
}

fn cmd_endask<'a>(input: In<'a>) -> CmdOut<'a> {
    value(Command::EndAsk, tag_no_case("ENDASK"))(input)
}

fn cmd_comment<'a>(input: In<'a>) -> CmdOut<'a> {
    value(Command::Comment, tag_no_case("#"))(input)
}

fn parse_command<'a>(input: In<'a>) -> CmdOut<'a> {
    alt((
        cmd_ifdef,
        cmd_ifndef,
        cmd_if,
        cmd_else,
        cmd_endif,
        cmd_ask,
        cmd_option,
        cmd_endask,
        cmd_comment,
    ))(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ws() {
        assert_eq!(ws("  ").unwrap(), (" ", " "));
        assert_eq!(ws("\t ").unwrap(), (" ", "\t"));
        assert!(ws("\n").is_err());
        assert!(ws("x").is_err());
    }

    #[test]
    fn test_ws_plus() {
        assert_eq!(ws_plus(" ").unwrap(), ("", " "));
        assert_eq!(ws_plus(" \t  \tx ").unwrap(), ("x ", " "));
        assert!(ws_plus("").is_err());
    }

    #[test]
    fn test_ws_star() {
        assert_eq!(ws_star(" ").unwrap(), ("", " "));
        assert_eq!(ws_star(" \t\t").unwrap(), ("", " "));
        assert_eq!(ws_star("").unwrap(), ("", " "));
    }

    #[test]
    fn test_tag_from_prefix() {
        let res = tag_from_prefix("~~~")("~~~ xyz");
        assert_eq!(res.unwrap(), (" xyz", "~~~"));
        let res = tag_from_prefix("~~~")("~~ xyz");
        assert!(res.is_err());
    }

    #[test]
    fn test_cmd_ifdef() {
        let res = cmd_ifdef("iFDef blub");
        assert_eq!(res.unwrap(), ("", Command::IfDef("blub")));
        let res = cmd_ifdef("iFDef x");
        assert_eq!(res.unwrap(), ("", Command::IfDef("x")));
        let res = cmd_ifdef("IFdef blub\nblub");
        assert_eq!(res.unwrap(), ("\nblub", Command::IfDef("blub")));
        let res = cmd_ifdef("iFDefblub");
        assert!(res.is_err());
    }

    #[test]
    fn test_cmd_ifndef() {
        let res = cmd_ifndef("iFnDef blubarb");
        assert_eq!(res.unwrap(), ("", Command::IfNDef("blubarb")));
        let res = cmd_ifndef("iFnDef x");
        assert_eq!(res.unwrap(), ("", Command::IfNDef("x")));
        let res = cmd_ifndef("IFndef blub\nblub");
        assert_eq!(res.unwrap(), ("\nblub", Command::IfNDef("blub")));
        let res = cmd_ifndef("iFNDefblub");
        assert!(res.is_err());
    }

    #[test]
    fn test_cmd_if() {
        let res = cmd_if("iF x\t== \ty");
        assert_eq!(res.unwrap(), ("", Command::If("x", "y")));
        let res = cmd_if("iF x == \t");
        assert!(res.is_err());
    }

    #[test]
    fn test_cmd_else() {
        assert_eq!(cmd_else("elSExyz").unwrap(), ("xyz", Command::Else));
        assert!(cmd_else("elSxyz").is_err());
    }

    #[test]
    fn test_cmd_endif() {
        assert_eq!(cmd_endif("ENDifblab").unwrap(), ("blab", Command::EndIf));
        assert!(cmd_else("EDIF").is_err());
    }

    #[test]
    fn test_cmd_ask() {
        assert_eq!(
            cmd_ask("asK\t\tblamber\nblab").unwrap(),
            ("\nblab", Command::Ask("blamber"))
        );
        assert!(cmd_ask("ASK\t").is_err());
    }

    #[test]
    fn test_cmd_option() {
        assert_eq!(
            cmd_option("OPTIOn\t one option\nnewline").unwrap(),
            ("\nnewline", Command::Option("one option"))
        );
        assert!(cmd_option("OPTIONN").is_err());
    }

    #[test]
    fn test_cmd_endask() {
        assert_eq!(cmd_endask("endASKabc").unwrap(), ("abc", Command::EndAsk));
        assert!(cmd_endask("endas").is_err());
    }

    #[test]
    fn test_comment() {
        assert_eq!(
            cmd_comment("# some comment").unwrap(),
            (" some comment", Command::Comment)
        );
        assert!(cmd_comment("// not a comment").is_err());
    }

    #[test]
    fn command_test_from_line() {
        let res = Command::parse_from_line("~~~", "~~ another line");
        assert!(res.is_none());

        let res = Command::parse_from_line("~~~", "~~~eLsE");
        assert_eq!(res.unwrap().unwrap(), Command::Else);

        let res = Command::parse_from_line(" ", " iF abc ==\txyz\t");
        assert_eq!(res.unwrap().unwrap(), Command::If("abc", "xyz"));
    }
}
