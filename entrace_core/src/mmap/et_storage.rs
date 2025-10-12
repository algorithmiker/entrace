use std::{
    any::Any,
    io::{BufReader, BufWriter, Read, Seek, Write},
    sync::RwLock,
    thread::JoinHandle,
};

use crate::{
    MixedTraceEntry, PoolEntry, TraceEntry,
    convert::{self, ConvertError, IETTableDataRef},
    entrace_magic_for,
    mmap::ETShutdownValue,
    storage::Storage,
    tree_layer::EnValue,
};

pub enum Message<Q: FileLike + Send> {
    Entry(MixedTraceEntry),
    Shutdown(Q),
}
#[derive(thiserror::Error, Debug)]
pub enum ETStorageError<T: FileLike> {
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error("Error while joining worker thread")]
    ThreadJoin(Box<dyn Any + Send>),
    #[error("No thread handle, storage was already finished or not initialized yet")]
    NoHandle,
    #[error("Cannot read thread handle, lock poisoned")]
    Poisoned,
    #[error("Failed to send shutdown message")]
    ShutdownSend,
    #[error("Failed final conversion from IET to ET. Buffer contains IET.")]
    /// Failed final conversion from IET to ET. Buffer contains IET.
    Convert {
        #[source]
        error: ConvertError,
        buf: T,
    },
}

pub trait FileLike: Read + Write + Seek {}
impl<T: Read + Write + Seek> FileLike for T {}
pub type ETResult<A, T> = Result<A, ETStorageError<T>>;
pub struct ETStorage<T: FileLike, Q: FileLike + Send> {
    pub sender: crossbeam_channel::Sender<Message<Q>>,
    pub thread_handle: RwLock<Option<JoinHandle<ETResult<ETShutdownValue<T, Q>, T>>>>,
}
impl<T: FileLike + Send + 'static, Q: FileLike + Send + 'static> ETStorage<T, Q> {
    pub fn init(mut file: T) -> Self
    where
        Self: std::marker::Sized,
    {
        let (tx, rx) = crossbeam_channel::unbounded::<Message<Q>>();
        let thread_handle = std::thread::spawn(move || {
            let magic = entrace_magic_for(1, crate::StorageFormat::IET);
            file.write_all(&magic).unwrap();
            let mut writer = BufWriter::new(&mut file);
            // Offsets relative to the start of the data section
            let mut offsets = vec![0u64];
            let mut child_lists = vec![PoolEntry::new()];
            let mut cur_offset = 0u64;
            let config = bincode::config::standard();
            let len =
                bincode::serde::encode_into_std_write(TraceEntry::root(), &mut writer, config)
                    .unwrap();
            cur_offset += len as u64;
            while let Ok(msg) = rx.recv() {
                match msg {
                    Message::Entry(entry) => {
                        offsets.push(cur_offset);
                        let len = child_lists.len() as u32;
                        child_lists[entry.parent as usize].children.push(len);
                        child_lists.push(PoolEntry::new());
                        let cfg = config;
                        let written =
                            bincode::serde::encode_into_std_write(&entry, &mut writer, cfg)
                                .unwrap();
                        cur_offset += written as u64;
                    }
                    Message::Shutdown(mut tmp_buf) => {
                        let mut tmp_buf_writer = BufWriter::new(&mut tmp_buf);
                        let table_data = IETTableDataRef::new(&offsets, &child_lists);
                        writer.flush().ok();
                        drop(writer);
                        let mut old_reader = BufReader::new(&mut file);
                        if let Err(y) = convert::iet_to_et_with_table(
                            &table_data,
                            &mut old_reader,
                            &mut tmp_buf_writer,
                            true,
                        ) {
                            return Err(ETStorageError::Convert { error: y, buf: file });
                        }

                        tmp_buf_writer.flush().ok();
                        drop(tmp_buf_writer);
                        drop(old_reader);
                        return Ok(ETShutdownValue {
                            temp_buf: Some(tmp_buf),
                            iet_buf: Some(file),
                        });
                    }
                }
            }
            Ok(ETShutdownValue { temp_buf: None, iet_buf: None })
        });

        Self { sender: tx, thread_handle: RwLock::new(Some(thread_handle)) }
    }

    pub fn finish(&self, param: Q) -> Result<ETShutdownValue<T, Q>, ETStorageError<T>> {
        use ETStorageError::*;
        self.sender.send(Message::Shutdown(param)).map_err(|_| ShutdownSend)?;
        let mut thread_handle = self.thread_handle.write().map_err(|_| Poisoned)?;
        let thread_handle = std::mem::take(&mut *thread_handle).ok_or(NoHandle)?;
        thread_handle.join().map_err(ThreadJoin)?
    }
}
impl<T: FileLike + Send + 'static, Q: FileLike + Send + 'static> Storage for ETStorage<T, Q> {
    fn new_span(&self, parent: u32, attrs: crate::Attrs, meta: &'static tracing::Metadata<'_>) {
        let message = attrs.iter().find(|x| x.0 == "message").map(|x| match &x.1 {
            EnValue::String(y) => y.clone(),
            q => format!("{q:?}"),
        });
        let entry = MixedTraceEntry { parent, metadata: meta.into(), attributes: attrs, message };

        self.sender.send(Message::Entry(entry)).ok();
    }
}
