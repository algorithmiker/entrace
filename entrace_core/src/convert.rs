use std::io::{Read, Seek, Write};

use crate::{PoolEntry, TraceEntry, entrace_magic_for};

#[derive(thiserror::Error, Debug)]
pub enum ConvertError {
    #[error("Cannot write to output buffer")]
    OutWriteError(#[source] std::io::Error),
    #[error("Failed to read from input buffer")]
    ReadInputError(#[source] std::io::Error),
    #[error("Wanted to read input starting from offset {0} but there is no data left")]
    NotEnoughBytes(usize),
    #[error("Failed to encode some data")]
    EncodeError(#[from] bincode::error::EncodeError),
    #[error("Failed to decode some data")]
    DecodeError(#[from] bincode::error::DecodeError),
    #[error("Failed to gather IET header")]
    GatherError(#[source] Box<ConvertError>),
}

/// Convert an IET file to a ET file.
///
/// Somewhat slow, as this will parse the whole file, in order to find where the offsets are.
/// It is the callers responsibility to buffer IO if desired.
///
/// See also: [iet_to_et_with_table], [gather_iet_table_data]
pub fn iet_to_et<R: Read + Seek, W: Write>(
    inp: &mut R, out: &mut W, skip_magic: bool, length_prefixed: bool,
) -> Result<(), ConvertError> {
    use ConvertError::GatherError;
    let table = gather_iet_table_data(inp, skip_magic, length_prefixed)
        .map_err(|x| GatherError(Box::new(x)))?;
    iet_to_et_with_table(&table.to_ref(), inp, out, skip_magic)
}

pub fn gather_iet_table_data<R: Read + Seek>(
    inp: &mut R, skip_magic: bool, length_prefixed: bool,
) -> Result<IETTableData, ConvertError> {
    if skip_magic {
        inp.seek(std::io::SeekFrom::Start(10)).map_err(ReadInputError)?;
    }
    let config = bincode::config::standard();
    let mut pool: Vec<PoolEntry> = vec![];
    let mut offsets = vec![];
    let mut had_root = false;
    let extra_offset = if skip_magic { 10 } else { 0 };
    use ConvertError::*;
    loop {
        if length_prefixed {
            let mut cl_buf = [0; 8];
            if let Err(y) = inp.read_exact(&mut cl_buf) {
                match y.kind() {
                    std::io::ErrorKind::UnexpectedEof => break,
                    _ => return Err(ReadInputError(y)),
                }
            }
            let _content_len = u64::from_le_bytes(cl_buf);
        }
        let pl = pool.len() as u32;
        let offset = inp
            .stream_position()
            .map_err(ConvertError::ReadInputError)?
            .saturating_sub(extra_offset);
        let decoded: Result<TraceEntry, _> = bincode::serde::decode_from_std_read(inp, config);
        match decoded {
            Ok(x) => {
                offsets.push(offset);
                pool.push(PoolEntry::new());
                if had_root {
                    pool[x.parent as usize].children.push(pl);
                }
                had_root = true;
            }
            Err(y) => match y {
                bincode::error::DecodeError::Io { inner, .. }
                    if inner.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break;
                }
                _ => return Err(DecodeError(y)),
            },
        }
    }
    Ok(IETTableData { offsets, child_lists: pool })
}

/// Span location data needed for [iet_to_et_with_table].
#[derive(Debug)]
pub struct IETTableData {
    offsets: Vec<u64>,
    child_lists: Vec<PoolEntry>,
}
impl IETTableData {
    pub fn to_ref(&'_ self) -> IETTableDataRef<'_, '_> {
        IETTableDataRef { offsets: &self.offsets, child_lists: &self.child_lists }
    }
}
/// A reference version of [IETTableData]
pub struct IETTableDataRef<'a, 'b> {
    offsets: &'a [u64],
    child_lists: &'b [PoolEntry],
}
impl<'a, 'b> IETTableDataRef<'a, 'b> {
    pub fn new(offsets: &'a [u64], child_lists: &'b [PoolEntry]) -> Self {
        Self { offsets, child_lists }
    }
}

/// Convert an IET file annotated with [IETTableData] to an ET file.
///
/// This is a faster alternative to [iet_to_et] if you happen to know the necessary tables already,
/// like [ETStorage](crate::mmap::ETStorage) does.
///
///
/// It is the caller's responsibility to buffer IO, if desired.
///
/// See also: [iet_to_et], [gather_iet_table_data]
pub fn iet_to_et_with_table<W: Write, R: Read + Seek>(
    table: &IETTableDataRef, inp: &mut R, out: &mut W, skip_magic: bool,
) -> Result<(), ConvertError> {
    use ConvertError::*;
    let magic = entrace_magic_for(1, crate::StorageFormat::ET);
    out.write_all(&magic).map_err(OutWriteError)?;

    let config = bincode::config::standard();
    bincode::serde::encode_into_std_write(table.offsets, out, config)?;
    bincode::serde::encode_into_std_write(table.child_lists, out, config)?;
    if skip_magic {
        inp.seek(std::io::SeekFrom::Start(10)).map_err(OutWriteError)?;
    }

    std::io::copy(inp, out).map_err(OutWriteError)?;

    Ok(())
}

/// Convert a ET file into an IET file.
///
/// It is the caller's responsibility to buffer IO.
///
/// For the reverse direction, see the [iet_to_et] family of functions.
pub fn et_to_iet<W: Write, R: Read + Seek>(
    inp: &mut R, out: &mut W, skip_magic: bool,
) -> Result<(), ConvertError> {
    use ConvertError::*;
    let magic = entrace_magic_for(1, crate::StorageFormat::IET);
    out.write_all(&magic).map_err(OutWriteError)?;
    if skip_magic {
        inp.seek(std::io::SeekFrom::Start(10)).map_err(ReadInputError)?;
    };
    let config = bincode::config::standard();
    // offset_table is a Vec<u32>.
    // we know from the bincode spec that these are encoded by an u64 for the length and then
    // the data. we don't want to allocate the data here, so skip it.
    let offset_table_len: u64 = bincode::serde::decode_from_std_read(inp, config)?;
    inp.seek_relative(offset_table_len as i64).map_err(OutWriteError)?;

    // we have to deserialize the whole thing here because each pool entry can have a dynamic
    // number of items
    let _pool: Vec<PoolEntry> = bincode::serde::decode_from_std_read(inp, config)?;
    std::io::copy(inp, out).map_err(OutWriteError)?;
    Ok(())
}
