mod args;
mod utils;
use crate::{
    args::{CliArgs, SourceDesc, Target},
    utils::{ensure_benchmark_dir, get_original_checkout, join},
};
use clap::Parser;
use command_error::CommandExt;
use std::{fs, io::Write, path::Path, process::Command, time::Duration};

fn log_write_bench(
    args: &CliArgs, cmdline: &[String], toplevel: &Path, benchdir: &Path,
) -> anyhow::Result<()> {
    let revs = &args.revisions;
    let paths: Vec<_> =
        revs.iter().map(|x| benchdir.join(format!("example-{}", x.executable_name()))).collect();
    for (source, path) in revs.iter().zip(&paths) {
        if fs::exists(path).unwrap_or(false)
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
    let args_joined = join(cmdline.iter(), ' ');
    for path in paths {
        cmd.arg(format!("{} {args_joined}", path.display()));
    }
    cmd.spawn()?.wait()?;
    Ok(())
}
fn script_bench(
    args: &CliArgs, script: &Path, logfile: &Path, toplevel: &Path, benchdir: &Path,
) -> anyhow::Result<()> {
    let revs = &args.revisions;
    let paths: Vec<_> =
        revs.iter().map(|x| benchdir.join(format!("script-{}", x.executable_name()))).collect();
    for (source, path) in revs.iter().zip(&paths) {
        if fs::exists(&path).unwrap_or(false)
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
            .args(["build", "-p", "entrace_script", "--release"])
            .output_checked()?;
        std::fs::copy(toplevel.join("target/release/entrace-script"), path)?;
    }
    info!("Built everything, waiting 0.5s to avoid interference");
    std::thread::sleep(Duration::from_millis(500));

    let mut cmd = Command::new("poop");
    for path in paths {
        cmd.arg(format!("{} -l {} -t {}", path.display(), script.display(), logfile.display()));
    }
    cmd.spawn()?.wait()?;
    Ok(())
}
fn main_inner(args: CliArgs) -> anyhow::Result<()> {
    let revs = &args.revisions;
    if revs.is_empty() {
        warning!("No sources given");
        warning!(
            "Help: Call this script like: bench -r 'Git(HEAD~1)' -r 'Git(HEAD)' log-write -- \
             --log-mode disk-et spammer"
        );
        return Ok(());
    }
    let (toplevel, benchdir) = ensure_benchmark_dir()?;
    info!("toplevel directory is {toplevel:?}");
    match args.target {
        Target::LogWrite { ref cmdline } => log_write_bench(&args, cmdline, &toplevel, &benchdir)?,
        Target::Script { ref script, ref logfile } => {
            script_bench(&args, script, logfile, &toplevel, &benchdir)?
        }
    }

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();
    let initial_commit = get_original_checkout()?;

    let inner_res = main_inner(args);

    info!("Checking out original revision ({initial_commit})");
    Command::new("git").arg("checkout").arg(initial_commit).output_checked()?;
    inner_res
}
