use std::fs::File;

use memmap2::{Mmap, MmapOptions};
use serde::{Deserialize, Serialize};

use crate::{
    Header, LevelContainer, MetadataRefContainer, PoolEntry, TraceEntryRef,
    log_provider::{LogProvider, LogProviderError, LogProviderResult},
    tree_layer::EnValueRef,
};

pub struct MmapLogProvider {
    map: Mmap,
    pub offset_table: Vec<u64>,
    pub child_lists: Vec<PoolEntry>,
    pub entries_start_offset: usize,
}
#[derive(Debug, thiserror::Error)]
pub enum MmapError {
    #[error("Failed to memory map the requested file")]
    MapFileError(#[source] std::io::Error),
    #[error("Failed to decode the offset table")]
    DecodeOffsetTable(#[source] bincode::error::DecodeError),
    #[error("Failed to decode the child-list pool")]
    DecodePool(#[source] bincode::error::DecodeError),
}
impl MmapLogProvider {
    /// # Safety
    /// This is marked unsafe to warn you about mmap's inherent unsafety.
    /// There is not much you can do about it.
    pub unsafe fn from_file(file: &File) -> Result<Self, MmapError> {
        use MmapError::*;
        let map = unsafe { MmapOptions::new().map(file) }.map_err(MapFileError)?;
        let mut offset = 10;
        let (offset_table, offset_table_len): (Vec<u64>, usize) =
            bincode::serde::borrow_decode_from_slice(&map[offset..], CFG)
                .map_err(DecodeOffsetTable)?;
        offset += offset_table_len;
        let (child_lists, pool_len): (Vec<PoolEntry>, usize) =
            bincode::serde::decode_from_slice(&map[offset..], CFG).map_err(DecodePool)?;
        offset += pool_len;
        Ok(Self { map, offset_table, child_lists, entries_start_offset: offset })
    }
    pub fn offset_of(&self, id: u32) -> Result<usize, LogProviderError> {
        self.offset_table
            .get(id as usize)
            .map(|x| *x as usize + self.entries_start_offset)
            .ok_or_else(|| LogProviderError::OutOfBounds { idx: id as usize, len: self.len() })
    }
}
const CFG: bincode::config::Configuration = bincode::config::standard();
impl LogProvider for MmapLogProvider {
    fn children(&self, x: u32) -> LogProviderResult<&[u32]> {
        let idx = x as usize;
        self.child_lists
            .get(idx)
            .map(|x| x.children.as_slice())
            .ok_or_else(|| LogProviderError::OutOfBounds { idx, len: self.len() })
    }

    fn attr_names(&'_ self, idx: u32) -> LogProviderResult<Vec<&'_ str>> {
        let offset = self.offset_of(idx)?;
        // decode only the head part (without attr_values)
        #[derive(Serialize, Deserialize, Clone, Debug)]
        pub struct TraceEntry2RefHead<'a> {
            pub parent: u32,
            pub message: Option<&'a str>,
            #[serde(borrow)]
            pub metadata: MetadataRefContainer<'a>,
            pub attr_names: Vec<&'a str>,
        }
        let decoded: (TraceEntry2RefHead, usize) =
            bincode::serde::borrow_decode_from_slice(&self.map[offset..], CFG)?;
        Ok(decoded.0.attr_names)
    }
    fn attr_values(&'_ self, idx: u32) -> LogProviderResult<Vec<EnValueRef<'_>>> {
        let offset = self.offset_of(idx)?;
        let decoded: (TraceEntryRef, usize) =
            bincode::serde::borrow_decode_from_slice(&self.map[offset..], CFG)?;
        Ok(decoded.0.attr_values)
    }
    fn attr_value(&self, x: u32, name: &str) -> LogProviderResult<Option<EnValueRef<'_>>> {
        let offset = self.offset_of(x)?;
        #[derive(Serialize, Deserialize, Clone, Debug)]
        pub struct EntryHead<'a> {
            pub parent: u32,
            pub message: Option<&'a str>,
            #[serde(borrow)]
            pub metadata: MetadataRefContainer<'a>,

            pub attr_names: Vec<&'a str>,
            // pub attr_values: Vec<EnValueRef<'a>>,
        }
        // optimization: only decode the values if we know the key's present
        // TODO:: could optimize further by lazily decoding values
        let (decoded, len): (EntryHead, _) =
            bincode::serde::borrow_decode_from_slice(&self.map[offset..], CFG)?;
        match decoded.attr_names.binary_search(&name) {
            Ok(idx) => {
                let (values, _): (Vec<EnValueRef<'_>>, _) =
                    bincode::serde::borrow_decode_from_slice(&self.map[offset + len..], CFG)?;
                Ok(Some(values[idx].clone()))
            }
            Err(_) => Ok(None),
        }
    }

    fn header(&'_ self, idx: u32) -> LogProviderResult<Header<'_>> {
        let offset = self.offset_of(idx)?;
        // only deserialize what we need
        #[derive(Serialize, Deserialize)]
        struct HeaderPart<'a> {
            parent: u32,
            message: Option<&'a str>,
            metadata: MetadataPart<'a>,
        }
        #[derive(Serialize, Deserialize)]
        struct MetadataPart<'a> {
            pub name: &'a str,
            pub target: &'a str,
            pub level: LevelContainer,
            pub file: Option<&'a str>,
            pub line: Option<u32>,
        }
        let from_offset = self
            .map
            .get(offset..)
            .ok_or_else(|| LogProviderError::OutOfBounds { idx: idx as usize, len: self.len() })?;
        let decoded: (HeaderPart, usize) =
            bincode::serde::borrow_decode_from_slice(from_offset, CFG)?;
        let HeaderPart { message, metadata: MetadataPart { name, level, file, line, .. }, .. } =
            decoded.0;
        Ok(Header { name, level, file, line, message })
    }
    fn message(&'_ self, x: u32) -> Result<Option<&'_ str>, LogProviderError> {
        let offset = self.offset_of(x)?;
        // only deserialize what we need
        #[derive(Serialize, Deserialize)]
        struct HeaderPart<'a> {
            parent: u32,
            message: Option<&'a str>,
        }
        let decoded: (HeaderPart, _) =
            bincode::serde::borrow_decode_from_slice(&self.map[offset..], CFG)?;

        Ok(decoded.0.message)
    }
    fn meta(&self, x: u32) -> LogProviderResult<MetadataRefContainer<'_>> {
        #[derive(Serialize, Deserialize)]
        struct HeaderPart<'a> {
            parent: u32,
            message: Option<&'a str>,
            metadata: MetadataRefContainer<'a>,
        }
        let offset = self.offset_of(x)?;
        let decoded: (HeaderPart, _) =
            bincode::serde::borrow_decode_from_slice(&self.map[offset..], CFG)?;

        Ok(decoded.0.metadata)
    }
    fn len(&self) -> usize {
        self.child_lists.len()
    }

    fn parent(&self, x: u32) -> LogProviderResult<u32> {
        let offset = self.offset_of(x)?;
        // there is a MemmapEntryRef at this offset. but since its first field is the parent,
        // decode just that.
        let decoded: (u32, _) = bincode::serde::borrow_decode_from_slice(&self.map[offset..], CFG)?;
        Ok(decoded.0)
    }
}
