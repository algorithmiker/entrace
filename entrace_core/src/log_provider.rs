use crate::{
    Header, MetadataRefContainer,
    remote::{FileIETError, RemoteLogProviderError},
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
