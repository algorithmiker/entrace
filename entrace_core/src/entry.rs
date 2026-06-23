use serde::{Deserialize, Serialize};

use crate::{
    MetadataContainer, MetadataRefContainer,
    tree_layer::{EnValue, EnValueRef},
};

/// Basic span data storage type.
/// You can't construct this on purpose. Use [TraceEntry::from_sorted_attrs] or
/// [TraceEntry::from_unsorted_attrs].
///
/// The canonical order of the fields here is `parent, message, metadata, attr_names, attr_values`
/// Methods reading the data are implemented on [TraceEntry2Ref]. You can get one with
/// [TraceEntry::as_ref]
#[derive(Serialize, Deserialize, Clone, Debug)]
#[allow(clippy::manual_non_exhaustive)]
pub struct TraceEntry {
    pub parent: u32,
    pub message: Option<String>,
    pub metadata: MetadataContainer,
    /// This MUST be sorted
    pub attr_names: Vec<String>,
    // TODO: to make this really fast, we'd need a way to decode just the k-th element of a bincode array
    // shouldn't be that hard tho
    pub attr_values: Vec<EnValue>,
    /// hack to disable the default TraceEntry {} constructor, so you must use a sorting constructor
    _sealed: (),
}
impl TraceEntry {
    pub fn root() -> TraceEntry {
        TraceEntry {
            parent: 0,
            message: None,
            metadata: MetadataContainer::root(),
            attr_names: vec![],
            attr_values: vec![],
            _sealed: (),
        }
    }
    pub fn from_sorted_attrs(
        parent: u32, message: Option<String>, metadata: MetadataContainer, attr_names: Vec<String>,
        attr_values: Vec<EnValue>,
    ) -> Self {
        TraceEntry { parent, message, metadata, attr_names, attr_values, _sealed: () }
    }
    pub fn from_unsorted_attrs(
        parent: u32, message: Option<String>, metadata: MetadataContainer,
        mut attr_names: Vec<String>, mut attr_values: Vec<EnValue>,
    ) -> Self {
        let mut pi = permutation::sort_unstable(&attr_names);
        pi.apply_slice_in_place(&mut attr_names);
        pi.apply_slice_in_place(&mut attr_values);
        TraceEntry { parent, message, metadata, attr_names, attr_values, _sealed: () }
    }

    pub fn as_ref(&'_ self) -> TraceEntryRef<'_> {
        TraceEntryRef::from_sorted_attrs(
            self.parent,
            self.message.as_deref(),
            self.metadata.as_ref(),
            self.attr_names.iter().map(|x| x.as_str()).collect(),
            self.attr_values.iter().map(|x| x.as_ref()).collect(),
        )
    }
}

/// Attribute names and values are always stored in sorted order
#[derive(Serialize, Deserialize, Clone, Debug)]
#[allow(clippy::manual_non_exhaustive)]
pub struct TraceEntryRef<'a> {
    pub parent: u32,
    pub message: Option<&'a str>,
    #[serde(borrow)]
    pub metadata: MetadataRefContainer<'a>,

    pub attr_names: Vec<&'a str>,
    pub attr_values: Vec<EnValueRef<'a>>,
    _sealed: (),
}
impl<'a> TraceEntryRef<'a> {
    pub fn from_sorted_attrs(
        parent: u32, message: Option<&'a str>, metadata: MetadataRefContainer<'a>,
        attr_names: Vec<&'a str>, attr_values: Vec<EnValueRef<'a>>,
    ) -> Self {
        TraceEntryRef { parent, message, metadata, attr_names, attr_values, _sealed: () }
    }
    pub fn get_attr(&self, name: &str) -> Option<EnValueRef<'a>> {
        let name_idx = self.attr_names.binary_search(&name).ok()?;
        // this clone should be ok since EnValueRef only contains references
        Some(self.attr_values[name_idx].clone())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[allow(clippy::manual_non_exhaustive)]
pub struct MixedTraceEntry {
    pub parent: u32,
    pub message: Option<String>,
    #[serde(borrow)]
    pub metadata: MetadataRefContainer<'static>,
    pub attr_names: Vec<String>,
    pub attr_values: Vec<EnValue>,
    /// hack to disable the default MixedTrace2Entry {} constructor, so you must use a sorting constructor
    _sealed: (),
}
impl MixedTraceEntry {
    pub fn from_sorted_attrs(
        parent: u32, message: Option<String>, metadata: MetadataRefContainer<'static>,
        attr_names: Vec<String>, attr_values: Vec<EnValue>,
    ) -> Self {
        MixedTraceEntry { parent, message, metadata, attr_names, attr_values, _sealed: () }
    }
    pub fn from_unsorted_attrs(
        parent: u32, message: Option<String>, metadata: MetadataRefContainer<'static>,
        mut attr_names: Vec<String>, mut attr_values: Vec<EnValue>,
    ) -> Self {
        let mut pi = permutation::sort_unstable(&attr_names);
        pi.apply_slice_in_place(&mut attr_names);
        pi.apply_slice_in_place(&mut attr_values);
        MixedTraceEntry { parent, message, metadata, attr_names, attr_values, _sealed: () }
    }

    pub fn as_ref<'a>(&'a self) -> TraceEntryRef<'a> {
        TraceEntryRef::from_sorted_attrs(
            self.parent,
            self.message.as_deref(),
            // this clone should be okay as MetadataRefContainer only contains references
            self.metadata.clone(),
            self.attr_names.iter().map(|x| x.as_str()).collect(),
            self.attr_values.iter().map(|x| x.as_ref()).collect(),
        )
    }
}
