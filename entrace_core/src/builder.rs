use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    TreeLayer, display_error_context,
    mmap::{ETStorage, FileLike},
    remote::{IETStorage, IETStorageConfig},
};

/// Drop guard to call finish() on an [IETStorage]
pub struct IETGuard<T: Write + Send + 'static> {
    storage: Arc<IETStorage<T>>,
}
impl<T: Write + Send + 'static> Drop for IETGuard<T> {
    fn drop(&mut self) {
        if let Err(e) = self.storage.finish() {
            eprintln!("IETGuard: failed to finish tracing: {e}")
        }
    }
}

/// Drop guard to call finish() on an [ETStorage]
pub struct ETGuard<T: FileLike + Send + 'static + fmt::Debug, Q: FileLike + Send + 'static> {
    storage: Arc<ETStorage<T, Q>>,
    final_buf: Option<Q>,
    /// If this is a Some(file path), then it should point to the path of the tempfile backing
    /// [Self::storage], and will be autoremoved with [Self::drop]
    temp_path: Option<PathBuf>,
}
impl<T: FileLike + Send + 'static + fmt::Debug, Q: FileLike + Send + 'static> Drop
    for ETGuard<T, Q>
{
    fn drop(&mut self) {
        if let Some(fin) = self.final_buf.take() {
            match self.storage.finish(fin) {
                Ok(_x) => {
                    if let Some(path) = &self.temp_path
                        && let Err(e) = fs::remove_file(path)
                    {
                        eprintln!(
                            "ETGuard: shutdown successful, but failed to remove temporary file at \
                             {}:\n{e}",
                            path.display()
                        )
                    }
                }
                Err(e) => {
                    println!("ETGuard: failed to finish tracing:\n{}", display_error_context(&e))
                }
            }
        }
    }
}

pub struct IETBuilder<T> {
    writer: T,
    length_prefixed: bool,
}

impl IETBuilder<BufWriter<File>> {
    /// Create a builder writing to a file path. Will buffer writes.
    pub fn from_file(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let file = File::create(path)?;
        Ok(Self::new(BufWriter::new(file)))
    }
}
impl IETBuilder<TcpStream> {
    /// Create a builder writing to a TCP stream.
    /// See [Self::connect_buffered] if you want to buffer writes
    /// (this'll, of course, increase message latency dramatically).
    ///
    /// This'll enable length-prefixed mode automatically.
    pub fn connect(addr: &str) -> std::io::Result<Self> {
        let stream = TcpStream::connect(addr)?;
        Ok(Self::new(stream).length_prefixed(true))
    }
}
impl IETBuilder<BufWriter<TcpStream>> {
    /// Create a builder writing to a TCP stream.
    /// This'll buffer writes, increasing message latency dramatically, but improving throughput.
    ///
    /// See [Self::connect] or [IETBuilder::new] for an unbuffered version
    /// This'll enable length-prefixed mode automatically.
    pub fn connect_buffered(addr: &str) -> std::io::Result<Self> {
        let stream = TcpStream::connect(addr)?;
        Ok(Self::new(BufWriter::new(stream)).length_prefixed(true))
    }
}

impl<T> IETBuilder<T>
where
    T: Write + Send + 'static,
{
    /// Configure an IET writer on an user-provided buffer.
    ///
    /// See [IETBuilder::from_file] for a simpler interface.
    pub fn new(writer: T) -> Self {
        Self { writer, length_prefixed: false }
    }

    /// Toggle length-prefixed mode (recommended for network/TCP streams).                                                                    
    pub fn length_prefixed(mut self, length_prefixed: bool) -> Self {
        self.length_prefixed = length_prefixed;
        self
    }

    pub fn build(self) -> std::io::Result<(TreeLayer<IETStorage<T>>, IETGuard<T>)> {
        let config = if self.length_prefixed {
            IETStorageConfig::length_prefixed(self.writer)
        } else {
            IETStorageConfig::non_length_prefixed(self.writer)
        };
        let storage = Arc::new(IETStorage::init(config));
        let layer = TreeLayer::from_storage(storage.clone());
        Ok((layer, IETGuard { storage }))
    }
}

pub struct ETBuilder<Temp, Final> {
    writer: Temp,
    final_buf: Final,
    temp_path: Option<PathBuf>,
}
impl ETBuilder<File, File> {
    /// Create a builder writing to a file path.
    /// This'll also create a temporary file at `path.tmp` which'll hold temporary data.
    pub fn from_file(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let temp_path = path.as_ref().with_added_extension("tmp");
        let temp_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .read(true)
            .open(&temp_path)?;
        let final_file = File::create(&path)?;
        Ok(Self::new(temp_file, final_file).temp_path(temp_path))
    }
}
impl<Temp, Final> ETBuilder<Temp, Final>
where
    Temp: FileLike + Send + 'static + fmt::Debug,
    Final: FileLike + Send + 'static,
{
    /// Create a new builder, which'll write the ET format.
    /// You have to pass a temporary buffer for this to work.
    /// For a function with a simpler interface, see [ETBuilder::from_file]
    ///
    /// Writes are buffered internally in [ETStorage], so you don't need a BufWriter.
    pub fn new(temp_file: Temp, final_buf: Final) -> Self {
        Self { writer: temp_file, final_buf, temp_path: None }
    }

    /// Indicate that [Self::temp] is a file with the path given in the argument.
    ///
    /// This'll make the resulting [ETGuard] remove the temp file on successful shutdown (drop).
    pub fn temp_path(mut self, path: PathBuf) -> Self {
        self.temp_path = Some(path);
        self
    }
    pub fn build(
        self,
    ) -> std::io::Result<(TreeLayer<ETStorage<Temp, Final>>, ETGuard<Temp, Final>)> {
        let storage = Arc::new(ETStorage::init(self.writer));
        let layer = TreeLayer::from_storage(storage.clone());
        Ok((layer, ETGuard { storage, final_buf: Some(self.final_buf), temp_path: self.temp_path }))
    }
}
