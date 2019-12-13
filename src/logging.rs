macro_rules! warn {
    ( $($s:expr),* ) => {
        use textwrap::Wrapper;
        let s = format!($($s),*);
        let s = Wrapper::with_termwidth().initial_indent("WARN: ").subsequent_indent("    : ").fill(&s);
        eprintln!("{}", s.yellow());
    }
}
macro_rules! error {
    ( $($s:expr),* ) => {
        use textwrap::Wrapper;
        let s = format!($($s),*);
        let s = Wrapper::with_termwidth().initial_indent("ERR : ").subsequent_indent("    : ").fill(&s);
        eprintln!("{}", s.red());
    }
}
macro_rules! info {
    ( $($s:expr),* ) => {
        let s = format!($($s),*);
        eprintln!("LOG : {}", s);
    }
}
