use std::{path::PathBuf, process::Command};

use command_error::CommandExt;

pub fn ensure_benchmark_dir() -> anyhow::Result<(PathBuf, PathBuf)> {
    let rev_parse = Command::new("git").arg("rev-parse").arg("--show-toplevel").output()?;
    let toplevel = PathBuf::from(str::from_utf8(&rev_parse.stdout)?.trim());
    let bench_p = toplevel.join(".bench");
    std::fs::create_dir(&bench_p).ok();
    Ok((toplevel, bench_p))
}

pub fn join(mut it: impl Iterator<Item: AsRef<str>>, sep: char) -> String {
    let mut s = String::new();
    for x in it.by_ref() {
        s.push_str(x.as_ref());
        s.push(sep);
    }
    s.pop();
    s
}
pub fn get_original_checkout() -> anyhow::Result<String> {
    let head = Command::new("git").arg("rev-parse").arg("--abbrev-ref").arg("HEAD").output()?;
    let mut o = String::from_utf8(head.stdout)?;
    o.pop();
    Ok(o)
}
pub fn canonicalize_commit(s: &str) -> anyhow::Result<String> {
    let can = Command::new("git").arg("rev-parse").arg(s).output_checked()?;
    let str = str::from_utf8(&can.stdout)?.trim().to_string();
    Ok(str)
}

pub const GREY: &str = "\x1b[90m";
pub const GREEN: &str = "\x1b[32m";
pub const BLUE: &str = "\x1b[34m";
pub const YELLOW: &str = "\x1b[33m";
pub const RED: &str = "\x1b[31m";
pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const RESET_BOLD: &str = "\x1b[22m";
#[macro_export]
macro_rules! info {
    ($($a:expr),*) => {{
        use $crate::utils::{BLUE,BOLD,RESET};
        let mut stderr = std::io::stderr().lock();
        write!(stderr, "{BLUE}{BOLD}[I]:{RESET} ").ok();
        writeln!(stderr, $($a),*).ok();
    }};
}

#[macro_export]
macro_rules! debug {
    ($($a:expr),*) => {{
        use $crate::utils::{GREEN,BOLD,RESET};
        let mut stderr = std::io::stderr().lock();
        use std::io::Write;
        write!(stderr, "{GREEN}{BOLD}[D]:{RESET} ").ok();
        writeln!(stderr, $($a),*).ok();
    }};
}
#[macro_export]
macro_rules! warning {
    ($($a:expr),*) => {{
        use $crate::utils::{YELLOW,BOLD,RESET};
        let mut stderr = std::io::stderr().lock();
        write!(stderr, "{YELLOW}{BOLD}[W]:{RESET} ").ok();
        writeln!(stderr, $($a),*).ok();
    }};
}
