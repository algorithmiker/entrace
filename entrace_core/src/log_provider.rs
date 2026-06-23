use crate::{
    Header, MetadataRefContainer,
    remote::{
        BaseIETLogProvider, FileIETError, FileIETLogProvider, RemoteLogProvider,
        RemoteLogProviderError,
    },
    tree_layer::EnValueRef,
};

#[derive(thiserror::Error, Debug)]
pub enum LogProviderError {
    #[error("Out of bounds. Tried to access index {idx} from a trace of length {len}")]
    OutOfBounds { idx: usize, len: usize },
    // TODO: investigate if boxing here would result in better or worse performance
    #[error("Failed to decode a binary value")]
    DecodeError(#[from] bincode::error::DecodeError),
    #[error(transparent)]
    IO(#[from] std::io::Error),

    #[error(transparent)]
    FileIETError(#[from] FileIETError),
    #[error(transparent)]
    RemoteLogProviderError(#[from] RemoteLogProviderError),
    /// This is not actually an error, we just use it to signal that the lua vm should quit
    #[error("This thread was shutdown during a join")]
    JoinShutdown,
}
pub type LogProviderResult<T> = Result<T, LogProviderError>;
#[allow(clippy::len_without_is_empty)]
/// The primary interface to read spans out of entrace traces.
///
/// Get one with [crate::load_trace] or [crate::remote::RemoteLogProvider].
pub trait LogProvider {
    fn children(&self, x: u32) -> LogProviderResult<&[u32]>;
    fn parent(&self, x: u32) -> LogProviderResult<u32>;

    fn attr_names(&'_ self, x: u32) -> LogProviderResult<Vec<&'_ str>>;
    fn attr_values(&'_ self, x: u32) -> LogProviderResult<Vec<EnValueRef<'_>>>;
    /// Equivalent to a search on attr_names/attr_values, but might be faster depending on the
    /// implementation.
    fn attr_value(&self, x: u32, name: &str) -> LogProviderResult<Option<EnValueRef<'_>>> {
        let attr_names = self.attr_names(x)?;
        let attr_values = self.attr_values(x)?;
        Ok(attr_names.iter().position(|&k| k == name).map(|i| attr_values[i].clone()))
    }

    fn header(&'_ self, x: u32) -> LogProviderResult<Header<'_>>;
    fn meta(&'_ self, x: u32) -> LogProviderResult<MetadataRefContainer<'_>>;

    /// The total amount of messages in this provider.
    /// This MUST be cheap as the frontend might call this every frame.
    fn len(&self) -> usize;

    /// The frontent SHOULD call this at the beginning of each painted frame,
    /// but there is no guarantee to whether or when it will.
    /// This runs on the main thread.
    /// The [LogProvider] implementation MUST ensure that this terminates quickly,
    /// as it directly affects FPS.
    fn frame_callback(&mut self) {}

    /// Equivalent to header.message, but some implementations might offer a fast path for this.
    fn message(&'_ self, x: u32) -> LogProviderResult<Option<&'_ str>> {
        Ok(self.header(x)?.message)
    }
}

pub enum LogProviderImpl {
    BaseIET(BaseIETLogProvider),
    FileIET(FileIETLogProvider),
    Remote(RemoteLogProvider),
    #[cfg(feature = "mmap")]
    Mmap(crate::mmap::MmapLogProvider),
}

impl LogProvider for LogProviderImpl {
    fn children(&self, x: u32) -> LogProviderResult<&[u32]> {
        match self {
            #[cfg(feature = "mmap")]
            Self::Mmap(inner) => inner.children(x),
            Self::BaseIET(inner) => inner.children(x),
            Self::FileIET(inner) => inner.children(x),
            Self::Remote(inner) => inner.children(x),
        }
    }

    fn parent(&self, x: u32) -> LogProviderResult<u32> {
        match self {
            #[cfg(feature = "mmap")]
            Self::Mmap(inner) => inner.parent(x),
            Self::BaseIET(inner) => inner.parent(x),
            Self::FileIET(inner) => inner.parent(x),
            Self::Remote(inner) => inner.parent(x),
        }
    }

    fn attr_names(&self, x: u32) -> LogProviderResult<Vec<&'_ str>> {
        match self {
            #[cfg(feature = "mmap")]
            Self::Mmap(inner) => inner.attr_names(x),
            Self::BaseIET(inner) => inner.attr_names(x),
            Self::FileIET(inner) => inner.attr_names(x),
            Self::Remote(inner) => inner.attr_names(x),
        }
    }

    fn attr_values(&self, x: u32) -> LogProviderResult<Vec<EnValueRef<'_>>> {
        match self {
            #[cfg(feature = "mmap")]
            Self::Mmap(inner) => inner.attr_values(x),
            Self::BaseIET(inner) => inner.attr_values(x),
            Self::FileIET(inner) => inner.attr_values(x),
            Self::Remote(inner) => inner.attr_values(x),
        }
    }

    fn attr_value(&self, x: u32, name: &str) -> LogProviderResult<Option<EnValueRef<'_>>> {
        match self {
            #[cfg(feature = "mmap")]
            Self::Mmap(inner) => inner.attr_value(x, name),
            Self::BaseIET(inner) => inner.attr_value(x, name),
            Self::FileIET(inner) => inner.attr_value(x, name),
            Self::Remote(inner) => inner.attr_value(x, name),
        }
    }

    fn header(&'_ self, x: u32) -> LogProviderResult<Header<'_>> {
        match self {
            #[cfg(feature = "mmap")]
            Self::Mmap(inner) => inner.header(x),
            Self::BaseIET(inner) => inner.header(x),
            Self::FileIET(inner) => inner.header(x),
            Self::Remote(inner) => inner.header(x),
        }
    }
    fn message(&'_ self, x: u32) -> LogProviderResult<Option<&str>> {
        match self {
            #[cfg(feature = "mmap")]
            Self::Mmap(inner) => inner.message(x),
            Self::BaseIET(inner) => inner.message(x),
            Self::FileIET(inner) => inner.message(x),
            Self::Remote(inner) => inner.message(x),
        }
    }

    fn meta(&'_ self, x: u32) -> LogProviderResult<MetadataRefContainer<'_>> {
        match self {
            #[cfg(feature = "mmap")]
            Self::Mmap(inner) => inner.meta(x),
            Self::BaseIET(inner) => inner.meta(x),
            Self::FileIET(inner) => inner.meta(x),
            Self::Remote(inner) => inner.meta(x),
        }
    }

    fn len(&self) -> usize {
        match self {
            #[cfg(feature = "mmap")]
            Self::Mmap(inner) => inner.len(),
            Self::BaseIET(inner) => inner.len(),
            Self::FileIET(inner) => inner.len(),
            Self::Remote(inner) => inner.len(),
        }
    }

    fn frame_callback(&mut self) {
        match self {
            #[cfg(feature = "mmap")]
            Self::Mmap(inner) => inner.frame_callback(),
            Self::BaseIET(inner) => inner.frame_callback(),
            Self::FileIET(inner) => inner.frame_callback(),
            Self::Remote(inner) => inner.frame_callback(),
        }
    }
}
