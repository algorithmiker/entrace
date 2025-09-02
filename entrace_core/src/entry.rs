use serde::{Deserialize, Serialize};

use crate::{
    MetadataContainer, MetadataRefContainer,
    tree_layer::{EnValue, EnValueRef},
};

#[derive(Serialize, Deserialize, Clone, Debug)]
/// Basic span data storage type.
///
/// The canonical order of the fields here is `parent, message, metadata, attributes`
pub struct TraceEntry {
    pub parent: u32,
    pub message: Option<String>,
    pub metadata: MetadataContainer,
    pub attributes: Vec<(String, EnValue)>,
}
impl TraceEntry {
    pub fn root() -> TraceEntry {
        TraceEntry {
            parent: 0,
            message: None,
            metadata: MetadataContainer::root(),
            attributes: vec![],
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TraceEntryRef<'a> {
    pub parent: u32,
    pub message: Option<&'a str>,
    #[serde(borrow)]
    pub metadata: MetadataRefContainer<'a>,
    pub attributes: Vec<(&'a str, EnValueRef<'a>)>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MixedTraceEntry {
    pub parent: u32,
    pub message: Option<String>,
    #[serde(borrow)]
    pub metadata: MetadataRefContainer<'static>,
    pub attributes: Vec<(String, EnValue)>,
}
