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
    #[error("Out of bounds. Tried to access index {idx} from a collection of length {len}")]
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
    fn children(&self, idx: u32) -> LogProviderResult<&[u32]>;
    fn parent(&self, idx: u32) -> LogProviderResult<u32>;

    fn attr_names(&'_ self, idx: u32) -> LogProviderResult<Vec<&'_ str>>;
    fn attr_values(&'_ self, idx: u32) -> LogProviderResult<Vec<EnValueRef<'_>>>;
    /// Equivalent to a search on attr_names/attr_values, but might be faster depending on the
    /// implementation.
    fn attr_value(&self, idx: u32, name: &str) -> LogProviderResult<Option<EnValueRef<'_>>> {
        let attr_names = self.attr_names(idx)?;
        let attr_values = self.attr_values(idx)?;
        Ok(attr_names.iter().position(|&k| k == name).map(|i| attr_values[i].clone()))
    }

    fn header(&'_ self, idx: u32) -> LogProviderResult<Header<'_>>;
    fn meta(&'_ self, idx: u32) -> LogProviderResult<MetadataRefContainer<'_>>;
    /// Equivalent to header.message, but some implementations might offer a fast path for this.
    fn message(&'_ self, idx: u32) -> LogProviderResult<Option<&'_ str>> {
        Ok(self.header(idx)?.message)
    }

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

macro_rules! dispatch {
    (fn $name:ident ( $($varname:ident : $vart:ty),* ) -> $res: ty) => {
        fn $name(&self, $($varname : $vart),*) -> $res {
            match self {
            #[cfg(feature = "mmap")]
            Self::Mmap(inner) => inner.$name($($varname),*),
            Self::BaseIET(inner) => inner.$name($($varname),*),
            Self::FileIET(inner) => inner.$name($($varname),*),
            Self::Remote(inner) => inner.$name($($varname),*),
            }
        }
    };
}
impl LogProvider for LogProviderImpl {
    dispatch!(fn children(x: u32)-> LogProviderResult<&[u32]>);
    dispatch!(fn parent(x: u32)-> LogProviderResult<u32>);
    dispatch!(fn attr_names(x: u32)-> LogProviderResult<Vec<&'_ str>>);
    dispatch!(fn attr_values(x: u32)-> LogProviderResult<Vec<EnValueRef<'_>>>);
    dispatch!(fn attr_value(x: u32, name: &str)-> LogProviderResult<Option<EnValueRef<'_>>>);
    dispatch!(fn header(x: u32)-> LogProviderResult<Header<'_>>);
    dispatch!(fn message(x: u32)-> LogProviderResult<Option<&str>>);
    dispatch!(fn meta(x: u32)-> LogProviderResult<MetadataRefContainer<'_>>);
    dispatch!(fn len()-> usize);

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
