macro_rules! warn {
    ( $($s:expr),* ) => {
        let s = format!($($s),*);
        let s = format!("WARN: {}", s);
        eprintln!("{}", s.yellow());
    }
}
macro_rules! error {
    ( $($s:expr),* ) => {
        let s = format!($($s),*);
        let s = format!("ERR : {}", s);
        eprintln!("{}", s.red());
    }
}
macro_rules! info {
    ( $($s:expr),* ) => {
        let s = format!($($s),*);
        eprintln!("LOG : {}", s);
    }
}
