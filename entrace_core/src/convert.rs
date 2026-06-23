use std::io::{BufReader, BufWriter, Read, Seek, Write};

use bincode::config::Configuration;
use serde::{Deserialize, Serialize};

use crate::{
    EN_DISK_VERSION, EnValue, MagicParseError, MetadataContainer, PoolEntry, StorageFormat,
    TraceEntry, entrace_magic_for, parse_entrace_magic,
};

#[derive(thiserror::Error, Debug)]
pub enum ConvertError {
    #[error("Cannot write to output buffer")]
    OutWriteError(#[source] std::io::Error),
    #[error("Failed to read from input buffer")]
    ReadInputError(#[source] std::io::Error),
    #[error("Failed to write to temporary buffer")]
    TempWriteError(#[source] std::io::Error),
    #[error("Failed to parse magic")]
    MagicParseError(#[from] MagicParseError),
    #[error("Wanted to read input starting from offset {0} but there is no data left")]
    NotEnoughBytes(usize),
    #[error("Failed to encode some data")]
    EncodeError(#[from] bincode::error::EncodeError),
    #[error("Failed to decode some data")]
    DecodeError(#[from] bincode::error::DecodeError),
    #[error("Failed to gather IET header")]
    GatherError(#[source] Box<ConvertError>),
    #[error("Input file has version {0}, but I'm told to convert from version {1}")]
    InputVersionMismatch(u8, u8),
    #[error("Input file has format {0:?}, but I'm told to convert from format {1:?}")]
    InputFormatMismatch(StorageFormat, StorageFormat),
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
    let magic = entrace_magic_for(EN_DISK_VERSION, crate::StorageFormat::ET);
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
    let magic = entrace_magic_for(EN_DISK_VERSION, crate::StorageFormat::IET);
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

// Old trace entry, from version 1
#[derive(Serialize, Deserialize, Clone, Debug)]
struct TraceEntry1 {
    pub parent: u32,
    pub message: Option<String>,
    pub metadata: MetadataContainer,
    pub attributes: Vec<(String, EnValue)>,
}

impl TraceEntry1 {
    pub fn into_trace_entry_2(self) -> TraceEntry {
        let TraceEntry1 { parent, message, metadata, mut attributes } = self;
        attributes.sort_unstable_by(|x, y| x.0.cmp(&y.0));
        let (attr_names, attr_values) = attributes.into_iter().unzip();

        TraceEntry::from_sorted_attrs(parent, message, metadata, attr_names, attr_values)
    }
}
/// Convert a version 1 et file to a version 2 (latest as of writing) format.
/// It is the caller's responsibility to buffer IO, but temp SHOULD not be buffered
/// (it'll be buffered internally, separately for read/write).
///
/// Temp MUST be an EMPTY scratch buffer (eg. a temp file)
/// If [skip_validating_magic] is set, it will not try to parse the magic, and assume you've already validated
/// this is an ET-v1 file.
pub fn et_v1_to_v2<W: Write, R: Read + Seek, RW: Read + Write + Seek>(
    inp: &mut R, out: &mut W, temp: &mut RW, skip_validating_magic: bool,
) -> Result<(), ConvertError> {
    use ConvertError::*;
    use bincode::serde::{decode_from_std_read, encode_into_std_write};
    const CFG: Configuration = bincode::config::standard();

    if !skip_validating_magic {
        let mut input_magic = [0; 10];
        inp.read_exact(&mut input_magic).map_err(ReadInputError)?;
        let (version, ty) = parse_entrace_magic(&input_magic)?;
        if version != 1 {
            return Err(ConvertError::InputVersionMismatch(version, 1));
        } else if ty != StorageFormat::ET {
            return Err(ConvertError::InputFormatMismatch(ty, StorageFormat::ET));
        }
    }

    let _inp_offsets: Vec<u64> = decode_from_std_read(inp, CFG)?;
    let child_lists: Vec<PoolEntry> = decode_from_std_read(inp, CFG)?;

    let mut temp_writer = BufWriter::new(temp);
    let mut new_offsets = vec![];
    for _processed in 0..child_lists.len() {
        let offset = temp_writer.stream_position().map_err(ReadInputError)?;
        new_offsets.push(offset);

        let entry1: TraceEntry1 = decode_from_std_read(inp, CFG)?;
        encode_into_std_write(entry1.into_trace_entry_2(), &mut temp_writer, CFG)?;
    }
    temp_writer.seek(std::io::SeekFrom::Start(0)).map_err(TempWriteError)?;
    let temp = temp_writer.into_inner().map_err(|x| TempWriteError(x.into_error()))?; // this will flush too

    let out_magic = entrace_magic_for(EN_DISK_VERSION, crate::StorageFormat::ET);
    out.write_all(&out_magic).map_err(OutWriteError)?;
    encode_into_std_write(new_offsets, out, CFG)?;
    encode_into_std_write(child_lists, out, CFG)?;
    let mut temp_reader = BufReader::new(temp);
    std::io::copy(&mut temp_reader, out).map_err(OutWriteError)?;

    Ok(())
}

/// Convert a version 1 iet file to a version 2 (latest as of writing) format.
/// It is the caller's responsibility to buffer IO.
pub fn iet_v1_to_v2<W: Write, R: Read + Seek>(
    inp: &mut R, out: &mut W,
) -> Result<(), ConvertError> {
    use ConvertError::*;
    use bincode::serde::{decode_from_std_read, encode_into_std_write};
    const CFG: Configuration = bincode::config::standard();

    let mut input_magic = [0; 10];
    inp.read_exact(&mut input_magic).map_err(ReadInputError)?;
    let (version, ty) = parse_entrace_magic(&input_magic)?;
    if version != 1 {
        return Err(ConvertError::InputVersionMismatch(version, 1));
    } else if ty != StorageFormat::ET {
        return Err(ConvertError::InputFormatMismatch(ty, StorageFormat::IET));
    }

    let out_magic = entrace_magic_for(EN_DISK_VERSION, crate::StorageFormat::ET);
    out.write_all(&out_magic).map_err(OutWriteError)?;

    loop {
        let decoded: Result<TraceEntry1, _> = decode_from_std_read(inp, CFG);
        match decoded {
            Ok(x) => encode_into_std_write(x.into_trace_entry_2(), out, CFG)?,
            Err(y) => match y {
                bincode::error::DecodeError::Io { inner, .. }
                    if inner.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break;
                }
                _ => return Err(ConvertError::DecodeError(y)),
            },
        };
    }

    Ok(())
}
