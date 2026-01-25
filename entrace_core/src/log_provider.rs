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
    fn children(&self, x: u32) -> Result<&[u32], LogProviderError>;
    fn parent(&self, x: u32) -> Result<u32, LogProviderError>;
    fn attrs(&'_ self, x: u32) -> Result<Vec<(&'_ str, EnValueRef<'_>)>, LogProviderError>;
    fn header(&'_ self, x: u32) -> Result<Header<'_>, LogProviderError>;
    fn meta(&'_ self, x: u32) -> Result<MetadataRefContainer<'_>, LogProviderError>;

    /// The total amount of messages in this provider.
    /// This MUST be cheap as the frontend might call this every frame.
    fn len(&self) -> usize;

    /// The frontent SHOULD call this at the beginning of each painted frame,
    /// but there is no guarantee to whether or when it will.
    /// This runs on the main thread.
    /// The [LogProvider] implementation MUST ensure that this terminates quickly,
    /// as it directly affects FPS.
    fn frame_callback(&mut self) {}
}

pub enum LogProviderImpl {
    BaseIET(BaseIETLogProvider),
    FileIET(FileIETLogProvider),
    Remote(RemoteLogProvider),
    #[cfg(feature = "mmap")]
    Mmap(crate::mmap::MmapLogProvider),
}

impl LogProvider for LogProviderImpl {
    fn children(&self, x: u32) -> Result<&[u32], LogProviderError> {
        match self {
            #[cfg(feature = "mmap")]
            Self::Mmap(inner) => inner.children(x),
            Self::BaseIET(inner) => inner.children(x),
            Self::FileIET(inner) => inner.children(x),
            Self::Remote(inner) => inner.children(x),
        }
    }

    fn parent(&self, x: u32) -> Result<u32, LogProviderError> {
        match self {
            #[cfg(feature = "mmap")]
            Self::Mmap(inner) => inner.parent(x),
            Self::BaseIET(inner) => inner.parent(x),
            Self::FileIET(inner) => inner.parent(x),
            Self::Remote(inner) => inner.parent(x),
        }
    }

    fn attrs(&'_ self, x: u32) -> Result<Vec<(&'_ str, EnValueRef<'_>)>, LogProviderError> {
        match self {
            #[cfg(feature = "mmap")]
            Self::Mmap(inner) => inner.attrs(x),
            Self::BaseIET(inner) => inner.attrs(x),
            Self::FileIET(inner) => inner.attrs(x),
            Self::Remote(inner) => inner.attrs(x),
        }
    }

    fn header(&'_ self, x: u32) -> Result<Header<'_>, LogProviderError> {
        match self {
            #[cfg(feature = "mmap")]
            Self::Mmap(inner) => inner.header(x),
            Self::BaseIET(inner) => inner.header(x),
            Self::FileIET(inner) => inner.header(x),
            Self::Remote(inner) => inner.header(x),
        }
    }

    fn meta(&'_ self, x: u32) -> Result<MetadataRefContainer<'_>, LogProviderError> {
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
