use anyhow::bail;
use command_error::CommandExt;
use std::{error::Error, io::Write, path::PathBuf, process::Command, time::Duration};

#[derive(Debug)]
enum SourceDesc {
    GitRevision(String),
    DirtyRev,
}

impl SourceDesc {
    pub fn executable_name(&self) -> &str {
        match self {
            SourceDesc::GitRevision(x) => x,
            SourceDesc::DirtyRev => "dirty",
        }
    }
}

fn canonicalize_commit(s: &str) -> anyhow::Result<String> {
    let can = Command::new("git").arg("rev-parse").arg(s).output_checked()?;
    let str = str::from_utf8(&can.stdout)?.trim().to_string();
    Ok(str)
}

fn parse_source_desc(s: &str) -> anyhow::Result<SourceDesc> {
    if let Some(end) = s.strip_prefix("Git(") {
        if let Some(rev) = end.strip_suffix(")") {
            let canonicalized = canonicalize_commit(rev)?;
            return Ok(SourceDesc::GitRevision(canonicalized));
        } else {
            bail!("Bad SourceDesc::Git {s}");
        }
    }
    if s == "." || s == "./" {
        return Ok(SourceDesc::DirtyRev);
    };
    bail!("Cannot parse source desc {s}")
}

fn ensure_benchmark_dir() -> Result<(PathBuf, PathBuf), Box<dyn Error>> {
    let rpo = Command::new("git").arg("rev-parse").arg("--show-toplevel").output()?;
    let toplevel = PathBuf::from(str::from_utf8(&rpo.stdout)?.trim());
    let bench_p = toplevel.join(".bench");
    std::fs::create_dir(&bench_p).ok();
    Ok((toplevel, bench_p))
}

fn get_original_checkout() -> Result<String, Box<dyn Error>> {
    let head = Command::new("git").arg("rev-parse").arg("--abbrev-ref").arg("HEAD").output()?;
    let mut o = String::from_utf8(head.stdout)?;
    o.pop();
    Ok(o)
}
fn join(mut it: impl Iterator<Item: AsRef<str>>, sep: char) -> String {
    let mut x = it.by_ref().fold(String::new(), |mut acc, x| {
        acc.push_str(x.as_ref());
        acc.push(sep);
        acc
    });
    x.pop();
    x
}
fn main_inner() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let (toplevel, benchdir) = ensure_benchmark_dir()?;
    info!("toplevel directory is {toplevel:?}");
    let sources: anyhow::Result<Vec<SourceDesc>> =
        args.by_ref().take_while(|x| x != "--").map(|x| parse_source_desc(x.trim())).collect();
    let bench_args = args;
    let sources = sources?;
    if sources.is_empty() {
        warning!("No sources given");
        warning!(
            "Help: Call this script like: bench 'Git(HEAD~1)' 'Git(HEAD)' -- --log-mode disk-et --work spammer"
        );
        return Ok(());
    }
    let paths: Vec<PathBuf> = sources.iter().map(|x| benchdir.join(x.executable_name())).collect();
    for (source, path) in sources.iter().zip(&paths) {
        if std::fs::exists(path).unwrap_or(false)
            && let SourceDesc::GitRevision(_) = source
        {
            debug!("Already have {path:?}.");
            continue;
        }

        if let SourceDesc::GitRevision(x) = source {
            debug!("Checking out and compiling {x}");
            Command::new("git").arg("checkout").arg(x).output_checked()?;
        } else {
            debug!("Compiling dirty revision");
        }
        Command::new("cargo")
            .args(["build", "-p", "entrace_example", "--release"])
            .output_checked()?;
        std::fs::copy(toplevel.join("target/release/entrace_example"), path)?;
    }
    info!("Built everything, waiting 0.5s to avoid interference");
    std::thread::sleep(Duration::from_millis(500));
    let mut cmd = Command::new("poop");
    let args_joined = join(bench_args, ' ');
    for path in paths {
        cmd.arg(format!("{} {args_joined}", path.to_string_lossy()));
    }
    cmd.spawn()?.wait()?;

    Ok(())
}
fn main() -> Result<(), Box<dyn Error>> {
    let initial_commit = get_original_checkout()?;
    let inner_res = main_inner();
    info!("Checking out original revision ({initial_commit})");
    Command::new("git").arg("checkout").arg(initial_commit).output_checked()?;
    inner_res
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
    ($($a:expr),*) => {
        let mut stderr = std::io::stderr().lock();
        write!(stderr, "{BLUE}{BOLD}[I]:{RESET} ").ok();
        writeln!(stderr, $($a),*).ok();
    };
}

#[macro_export]
macro_rules! debug {
    ($($a:expr),*) => {
        let mut stderr = std::io::stderr().lock();
        use std::io::Write;
        write!(stderr, "{GREEN}{BOLD}[D]:{RESET} ").ok();
        writeln!(stderr, $($a),*).ok();
    };
}
#[macro_export]
macro_rules! warning {
    ($($a:expr),*) => {
        let mut stderr = std::io::stderr().lock();
        write!(stderr, "{YELLOW}{BOLD}[W]:{RESET} ").ok();
        writeln!(stderr, $($a),*).ok();
    };
}
