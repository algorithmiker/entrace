use entrace_core::LogProvider;

pub mod filtersets;
pub mod lua_api;
pub mod lua_value;

pub(crate) type TraceProvider = Box<dyn LogProvider + Send + Sync>;
#[derive(thiserror::Error, Debug, Clone)]
pub enum QueryError {
    #[error("Index out of bounds. Tried to access element {index} of a container of size {actual}")]
    OutOfBounds { index: u32, actual: u32 },
    #[error(
        "The thread running your query died. This usually means an error in entrace, and not in \
         your code."
    )]
    QueryDied,
    #[error("Error while running your query")]
    LuaError(#[source] mlua::Error),
    #[error(
        "Failed to coerce the result of your query to Vec<u32>. Make sure to return a list from \
         the query!"
    )]
    FailedToCoerce(#[source] mlua::Error),
}
