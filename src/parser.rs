use nom::branch::alt;
use nom::bytes::complete::{tag, tag_no_case, take_until};
use nom::combinator::{map, rest, value};
use nom::multi::{many0, many1};
use nom::sequence::{terminated, tuple};
use nom::{error::ErrorKind, Err, IResult, Needed};

use crate::error::{Error, Result};

type In<'a> = &'a str;
type Out<'a> = IResult<&'a str, &'a str>;
type CmdOut<'a> = IResult<&'a str, Command<'a>>;
type Var<'a> = &'a str;

#[derive(Debug, Clone, PartialEq, Eq)]
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
        |(_, _, var1, _, _, _, var2)| Command::If(var1, var2),
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
mod tests {}
