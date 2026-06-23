use anyhow::Context;
use clap::Parser;
use entrace_core::{EN_DISK_VERSION, convert};
use fs_err::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read};
#[derive(clap::Parser)]
pub struct Args {
    #[command(subcommand)]
    subcommand: Subcommand,
}
#[derive(clap::Subcommand)]
pub enum Subcommand {
    /// List supported conversions
    List,
    /// Convert a file from one format to another
    Convert(ConvertArgs),
}
#[derive(clap::Args)]
pub struct ConvertArgs {
    /// Input file
    #[arg(short, long)]
    input: std::path::PathBuf,

    /// Output file
    #[arg(short, long)]
    output: std::path::PathBuf,

    /// Output format
    #[arg(short = 'f', long)]
    out_format: StorageFormat,
}

#[derive(clap::ValueEnum, Debug, Copy, Clone, PartialEq)]
pub enum StorageFormat {
    ET = 0,
    IET = 1,
}
impl StorageFormat {
    pub fn from_entrace(format: entrace_core::StorageFormat) -> anyhow::Result<Self> {
        match format {
            entrace_core::StorageFormat::ET => Ok(StorageFormat::ET),
            entrace_core::StorageFormat::IET => Ok(StorageFormat::IET),
            _ => Err(anyhow::anyhow!("Unsupported input format: {:?}", format)),
        }
    }
}
const SUPPORTED_CONVERSIONS: &[(u8, StorageFormat, u8, StorageFormat)] = &[
    (1, StorageFormat::IET, EN_DISK_VERSION, StorageFormat::IET),
    (1, StorageFormat::ET, EN_DISK_VERSION, StorageFormat::ET),
    (EN_DISK_VERSION, StorageFormat::IET, EN_DISK_VERSION, StorageFormat::ET),
    (EN_DISK_VERSION, StorageFormat::ET, EN_DISK_VERSION, StorageFormat::IET),
];
fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    match args.subcommand {
        Subcommand::List => {
            println!("Available conversions:");
            for (v1, f1, v2, f2) in SUPPORTED_CONVERSIONS {
                println!(" - {f1:?}-v{v1} -> {f2:?}-v{v2}");
            }
            Ok(())
        }
        Subcommand::Convert(convert_args) => {
            let input = File::open(convert_args.input).context("failed to open input")?;
            let mut reader = BufReader::new(input);
            let mut magic_buf = [0; 10];
            reader.read_exact(&mut magic_buf).context("failed to read magic")?;
            let (in_version, format) =
                entrace_core::parse_entrace_magic(&magic_buf).context("failed to parse magic")?;
            let format = StorageFormat::from_entrace(format)?;
            let out_format = convert_args.out_format;

            let out_file = File::create(&convert_args.output)?;
            let mut out_writer = BufWriter::new(out_file);
            match (in_version, format, out_format) {
                (1, StorageFormat::IET, StorageFormat::IET) => {
                    convert::iet_v1_to_v2(&mut reader, &mut out_writer)
                        .context("Conversion failed")?;
                }
                (1, StorageFormat::ET, StorageFormat::ET) => {
                    let tmp_path = convert_args.output.with_extension("tmp");
                    let mut tmp = OpenOptions::new()
                        .create(true)
                        .truncate(true)
                        .write(true)
                        .read(true)
                        .open(&tmp_path)?;

                    convert::et_v1_to_v2(&mut reader, &mut out_writer, &mut tmp, true)
                        .context("Conversion failed")?;
                    fs_err::remove_file(&tmp_path)?;
                }
                (EN_DISK_VERSION, StorageFormat::IET, StorageFormat::ET) => {
                    convert::iet_to_et(&mut reader, &mut out_writer, true, false)
                        .context("Conversion failed")?;
                }
                (EN_DISK_VERSION, StorageFormat::ET, StorageFormat::IET) => {
                    convert::et_to_iet(&mut reader, &mut out_writer, true)
                        .context("Conversion failed")?;
                }
                _ => {
                    return Err(anyhow::anyhow!(
                        "Can't convert {format:?}-v{in_version} to \
                         {out_format:?}-v{EN_DISK_VERSION}\nUse `entrace_convert list` to see \
                         supported conversions"
                    ));
                }
            }

            Ok(())
        }
    }
}
