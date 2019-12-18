macro_rules! warn {
    ( $($s:expr),* ) => {
        log!( $($s),* ; "WARN"; |s: &str| s.yellow() )
    }
}
macro_rules! error {
    ( $($s:expr),* ) => {
        log!( $($s),* ; "ERR "; |s: &str| s.red().bold() )
    }
}
macro_rules! info {
    ( $($s:expr),* ) => {
        log!( $($s),* ; "INFO"; |s: &str| s.dimmed() )
    }
}
macro_rules! log {
    ( $($s:expr),* ; $prefix:expr; $fn:expr ) => {
        {
            use textwrap::Wrapper;
            use colored::Colorize;
            let s = format!($($s),*);
            let width = textwrap::termwidth() - 6;
            let lines: Vec<_> = Wrapper::new(width).wrap_iter(&s).collect();
            if lines.len() == 1 {
                let line = format!("{} ─ {}", $prefix, lines[0].as_ref());
                eprintln!("{}", $fn(&line));
            } else if lines.len() > 1 {
                let line = format!("{} ┬ {}", $prefix, lines[0].as_ref());
                eprintln!("{}", $fn(&line));
                for line in &lines[1..lines.len() - 1] {
                    let line = format!("     │ {}", line);
                    eprintln!("{}", $fn(&line));
                }
                let line = format!("     └ {}", lines.last().unwrap());
                eprintln!("{}", $fn(&line));
            }
        }
    }
}
