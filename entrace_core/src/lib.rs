#![doc = include_str!("../README.md")]

use crate::remote::{FileIETLogProvider, FileWatchConfig, IETEvent, LoadIETError};
use serde::{Deserialize, Serialize};
use std::{fmt::Write, fs::File, io::Read, path::Path};
use storage::Storage;
use thiserror::Error;
use tracing::Level;

use crate::remote::{DummyRefresher, Refresh};

pub mod convert;
pub mod en_formatter;
mod entry;
pub use entry::*;
mod log_provider;
pub use log_provider::*;
pub mod mmap;
pub mod remote;
pub mod storage;
mod tree_layer;
pub use tree_layer::*;

type PoolRef = u32;
/// Item in the tree of spans tracked by entrace
#[derive(Default, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PoolEntry {
    pub children: Vec<PoolRef>,
}
impl PoolEntry {
    pub fn new() -> Self {
        Default::default()
    }
}
/// A serializable representation of [tracing::Level].
#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize)]
pub enum LevelContainer {
    #[default]
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
}
impl From<&tracing::Level> for LevelContainer {
    fn from(value: &tracing::Level) -> Self {
        match *value {
            Level::TRACE => Self::Trace,
            Level::DEBUG => Self::Debug,
            Level::INFO => Self::Info,
            Level::WARN => Self::Warn,
            Level::ERROR => Self::Error,
        }
    }
}
pub type Attrs = Vec<(String, EnValue)>;

// Warning: be extremely careful when changing the fields of this type,
// as bincode writes things in the order declared here!
/// Metadata about a span, which is provided by `tracing`, and not the library producing the
/// span.
///
///
/// The canonical order of the fields of this type is
/// `name, target, level, module_path, file, line`
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct MetadataContainer {
    pub name: String,
    pub target: String,
    pub level: LevelContainer,
    pub module_path: Option<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
}
impl MetadataContainer {
    pub fn root() -> MetadataContainer {
        MetadataContainer {
            name: "root".into(),
            target: String::new(),
            level: LevelContainer::Trace,
            module_path: None,
            file: None,
            line: None,
        }
    }
    pub fn as_ref(&self) -> MetadataRefContainer<'_> {
        MetadataRefContainer {
            name: &self.name,
            target: &self.target,
            level: self.level,
            module_path: self.module_path.as_deref(),
            file: self.file.as_deref(),
            line: self.line,
        }
    }
}
impl From<&tracing::Metadata<'_>> for MetadataContainer {
    fn from(value: &tracing::Metadata) -> Self {
        Self {
            name: value.name().to_owned(),
            target: value.target().to_owned(),
            module_path: value.module_path().map(|x| x.to_string()),
            file: value.file().map(|x| x.to_string()),
            line: value.line(),
            level: value.level().into(),
        }
    }
}
impl From<MetadataRefContainer<'_>> for MetadataContainer {
    fn from(val: MetadataRefContainer<'_>) -> Self {
        MetadataContainer {
            name: val.name.to_owned(),
            target: val.target.to_owned(),
            level: val.level,
            module_path: val.module_path.map(|x| x.to_string()),
            file: val.file.map(|x| x.to_string()),
            line: val.line.to_owned(),
        }
    }
}
/// A version of [MetadataContainer] with borrowed fields.
///
/// The canonical order of the fields of this type is
/// `name, target, level, module_path, file, line`
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct MetadataRefContainer<'a> {
    pub name: &'a str,
    pub target: &'a str,
    pub level: LevelContainer,
    pub module_path: Option<&'a str>,
    pub file: Option<&'a str>,
    pub line: Option<u32>,
}

impl<'a> From<&'a tracing::Metadata<'_>> for MetadataRefContainer<'a> {
    fn from(value: &'a tracing::Metadata) -> Self {
        Self {
            name: value.name(),
            target: value.target(),
            module_path: value.module_path(),
            file: value.file(),
            line: value.line(),
            level: value.level().into(),
        }
    }
}

/// Some common information about a span that might be used to represent it graphically.
pub struct Header<'a> {
    pub name: &'a str,
    pub level: LevelContainer,
    pub file: Option<&'a str>,
    pub line: Option<u32>,
    pub message: Option<&'a str>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum StorageFormat {
    ET = 0,
    IET = 1,
    IETPrefix = 2,
}
pub const EN_DISK_VERSION: u8 = 1;
#[derive(Error, Debug)]
pub enum LoadTraceError {
    #[error("Failed to parse magic number")]
    BadMagic(#[from] MagicParseError),
    #[error(
        "Cannot parse newer disk format than known. I have {EN_DISK_VERSION}, the file has {0}"
    )]
    InvalidVersion(u8),
    #[error("No version header, are you sure this is a entrace log?")]
    NoVersion,
    #[error("Deserialization error")]
    DeserializationError(#[from] bincode::error::DecodeError),
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[cfg(feature = "mmap")]
    #[error("Failed to create ET log provider")]
    MmapError(#[from] mmap::MmapError),
    #[error("Tried to load an ET file, but you didn't enable the mmap feature for entrace_core")]
    MmapNeeded,
    #[error("Failed to create IET log provider")]
    IETError(#[from] LoadIETError),
}
#[derive(Error, Debug)]
pub enum MagicParseError {
    #[error("The first byte is not null. This can't be a entrace file.")]
    FirstNonNull,
    #[error(
        "The [1,..,8) (0-indexed) bytes of the trace file should be b\"ENTRACE\" but they aren't"
    )]
    AppNameMismatch,
    #[error("The storage format byte (9) must be 0 or 1")]
    BadStorageFormat,
    #[error("IO Error while parsing magic. Make sure the file is non-empty.")]
    IoError(#[from] std::io::Error),
}
pub fn parse_entrace_magic(magic: &[u8; 10]) -> Result<(u8, StorageFormat), MagicParseError> {
    if magic[0] != 0 {
        return Err(MagicParseError::FirstNonNull);
    }
    if &magic[1..8] != b"ENTRACE" {
        return Err(MagicParseError::AppNameMismatch);
    }
    let s = match magic[9] {
        0 => StorageFormat::ET,
        1 => StorageFormat::IET,
        2 => StorageFormat::IETPrefix,
        _ => return Err(MagicParseError::BadStorageFormat),
    };
    Ok((magic[8], s))
}

pub fn entrace_magic_for(version: u8, format: StorageFormat) -> [u8; 10] {
    let mut magic = [0, 69, 78, 84, 82, 65, 67, 69, 0, 0]; // b"\0ENTRACE" and two temporary 0s
    magic[8] = version;
    magic[9] = format as u8;
    magic
}
pub struct LoadConfig<R: Refresh = DummyRefresher> {
    pub iht: IETLoadConfig<R>,
}
impl Default for LoadConfig {
    fn default() -> Self {
        Self { iht: IETLoadConfig::default() }
    }
}

pub struct IETLoadConfig<R: Refresh = DummyRefresher> {
    pub watch: FileWatchConfig,
    pub presentation: IETPresentationConfig<R>,
}
impl Default for IETLoadConfig {
    fn default() -> Self {
        Self { watch: FileWatchConfig::DontWatch, presentation: IETPresentationConfig::default() }
    }
}

pub struct IETPresentationConfig<R: Refresh = DummyRefresher> {
    pub event_tx: Option<crossbeam_channel::Sender<IETEvent>>,
    pub refresher: R,
}
impl Default for IETPresentationConfig {
    fn default() -> Self {
        IETPresentationConfig { event_tx: None, refresher: DummyRefresher {} }
    }
}
impl<R: Refresh> IETPresentationConfig<R> {
    pub fn new(event_tx: Option<crossbeam_channel::Sender<IETEvent>>, refresher: R) -> Self {
        Self { event_tx, refresher }
    }
}

/// Helper function to load a generic trace from a file.
///
/// # Safety
/// This function is only unsafe if you are using a file in the ET format, which is memory mapped.
///
/// See also [FileIETLogProvider::new] and [remote::load_iht_trace] for functions that read an IET trace,
/// in a safe way.
pub unsafe fn load_trace<R: Refresh + Send + 'static>(
    file_path: impl AsRef<Path> + Send + 'static, config: LoadConfig<R>,
) -> Result<Box<dyn LogProvider + Send + 'static + Sync>, LoadTraceError> {
    let mut file = File::open(&file_path)?;
    let mut buf = [0; 10];
    file.read_exact(&mut buf).map_err(|x| LoadTraceError::BadMagic(MagicParseError::IoError(x)))?;
    let (version, ty) = parse_entrace_magic(&buf)?;
    if version != EN_DISK_VERSION {
        Err(LoadTraceError::InvalidVersion(version))?;
    }
    match ty {
        StorageFormat::IET => {
            let provider = FileIETLogProvider::new(file, config.iht, false)?;
            Ok(Box::new(provider))
        }
        StorageFormat::IETPrefix => {
            let provider = FileIETLogProvider::new(file, config.iht, true)?;
            Ok(Box::new(provider))
        }
        StorageFormat::ET => {
            #[cfg(feature = "mmap")]
            {
                use crate::mmap::MmapLogProvider;
                // SAFETY: Mmap is inherently unsafe.
                let provider = unsafe { MmapLogProvider::from_file(&file) }
                    .map_err(LoadTraceError::MmapError)?;
                return Ok(Box::new(provider));
            }
            #[allow(unreachable_code)]
            Err(LoadTraceError::MmapNeeded)
        }
    }
}

pub fn display_error_context(mut err: &dyn std::error::Error) -> String {
    let mut s = format!("{err}");
    if err.source().is_none() {
        return s;
    }
    write!(s, "\n\nCaused by:\n").ok();
    let mut idx = 0;

    while let Some(source) = err.source() {
        write!(s, "  {idx}: {source}").ok();
        err = source;
        idx += 1;
    }
    s.pop();
    s
}
