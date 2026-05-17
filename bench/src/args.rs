use crate::utils::canonicalize_commit;
use anyhow::bail;
use std::path::PathBuf;

#[derive(clap::Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct CliArgs {
    #[arg(long, short, default_value = "Git(HEAD)", value_parser = parse_source_desc)]
    pub revisions: Vec<SourceDesc>,
    #[command(subcommand)]
    pub target: Target,
}
#[derive(Debug, clap::Subcommand)]
pub enum Target {
    #[command(about = "bench log writing")]
    LogWrite {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        cmdline: Vec<String>,
    },
    #[command(about = "bench querying/entrace-script")]
    Script {
        #[arg(long, short)]
        logfile: PathBuf,
        #[arg(long, short)]
        script: PathBuf,
    },
}

#[derive(Debug, Clone)]
pub enum SourceDesc {
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
