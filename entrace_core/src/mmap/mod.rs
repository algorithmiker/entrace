// memory mappable file format
// structure:
// - entrace magic
// - index to offset mappings.
// - child lists
// - for each span: metadata and attributes
// serialization:
// - store an immediate IET trace on disk.
// - no memory mapping when writing
//   a) tracing data is valuable, and we don't want to lose it
//   b) we don't want to create potential UB for the entrace_core consumer (for example, if you
//   opened an mmap file in entrace while the traced process was writing it, that'd already be UB)
// - encode new spans one by one after each other.
// - on shutdown, we have to copy the entire file over, but that is an one time thing.
//   we generate the header based on the collected entries, then write the entries themselves.
mod et_storage;
#[cfg(feature = "mmap")]
mod mmap_log_provider;
pub use et_storage::*;
#[cfg(feature = "mmap")]
pub use mmap_log_provider::*;
use std::io::{Read, Seek, Write};

pub trait FileLike: Read + Write + Seek {}
impl<T: Read + Write + Seek> FileLike for T {}

pub struct ETShutdownValue<T: FileLike, Q: FileLike> {
    pub temp_buf: Option<Q>,
    pub iet_buf: Option<T>,
}
