use crate::{
    EN_DISK_VERSION, EnValueRef, StorageFormat, TraceEntry, entrace_magic_for, storage::Storage,
    tree_layer::EnValue,
};
use crossbeam_channel::{Receiver, SendError, Sender};
use std::{
    any::Any,
    collections::BTreeMap,
    io::Write,
    net::TcpListener,
    sync::{
        RwLock,
        atomic::{
            AtomicBool,
            Ordering::{Acquire, Release},
        },
    },
    thread::JoinHandle,
};

pub enum RemoteMessage {
    NewSpan { id: u32, entry: TraceEntry },
    Shutdown,
}

#[derive(thiserror::Error, Debug)]
pub enum IETStorageError {
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error("Error while joining worker thread")]
    ThreadJoin(Box<dyn Any + Send>),
    #[error("No thread handle, storage was already finished or not initialized yet")]
    NoHandle,
    #[error("Cannot read thread handle, lock poisoned")]
    Poisoned,
    #[error(transparent)]
    Send(#[from] Box<SendError<RemoteMessage>>),
}
pub struct IETStorage<T> {
    pub sender: Sender<RemoteMessage>,
    pub thread_handle: RwLock<Option<JoinHandle<T>>>,
    pub finished: AtomicBool,
}

impl<T: Write + Send + 'static> IETStorage<T> {
    pub fn init(writable: T, length_prefixed: bool) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        let thread_handle =
            std::thread::spawn(move || Self::writer_thread_main(writable, length_prefixed, rx));
        IETStorage {
            sender: tx,
            thread_handle: RwLock::new(Some(thread_handle)),
            finished: false.into(),
        }
    }

    fn writer_thread_main(
        mut writable: T, length_prefixed: bool, rx: Receiver<RemoteMessage>,
    ) -> T {
        let format = if length_prefixed { StorageFormat::IETPrefix } else { StorageFormat::IET };
        let magic = entrace_magic_for(EN_DISK_VERSION, format);
        writable.write_all(&magic).unwrap();
        let mut buffer: Vec<u8> = Vec::with_capacity(1024);
        /// Write a length-prefixed message.
        fn write_message(
            buffer: &mut Vec<u8>, message: TraceEntry,
            writable: &mut (impl Write + Send + 'static), length_prefixed: bool,
        ) {
            let bcfg = bincode::config::standard();
            if length_prefixed {
                buffer.clear();
                bincode::serde::encode_into_std_write(message, buffer, bcfg).unwrap();

                writable.write_all(&(buffer.len() as u64).to_le_bytes()).unwrap();
                std::io::copy(&mut buffer.as_slice(), writable).unwrap();
            } else {
                bincode::serde::encode_into_std_write(message, writable, bcfg).unwrap();
            }
        }

        write_message(&mut buffer, TraceEntry::root(), &mut writable, length_prefixed);
        // If multiple threads are writing subspans to the same parent, there can be a race
        // condition where a child arrives before the parent. This is reproducable on Windows
        // only, see issue #4.
        // To prevent this, we store disjoint leaves in a buffer until their parent arrives.
        // next_id keeps track of what id comes next in sequential order. if the buffer has next_id, we are good and the entry can be written.
        // Else we wait for it to appear.
        let mut reorder_buffer = BTreeMap::new();
        let mut next_id = 1u32;
        while let Ok(msg) = rx.recv() {
            match msg {
                RemoteMessage::NewSpan { id, entry } => {
                    reorder_buffer.insert(id, entry);
                    while let Some(entry) = reorder_buffer.remove(&next_id) {
                        write_message(&mut buffer, entry, &mut writable, length_prefixed);
                        next_id += 1;
                    }
                }
                RemoteMessage::Shutdown => break,
            }
        }
        writable.flush().ok();
        writable
    }
}
impl<T> IETStorage<T> {
    /// returns None if already finished
    pub fn finish(&self) -> Result<Option<T>, IETStorageError> {
        if self.finished.load(Acquire) {
            return Ok(None);
        }

        self.sender.send(RemoteMessage::Shutdown).map_err(Box::new)?;
        let mut thread_handle =
            self.thread_handle.write().map_err(|_| IETStorageError::Poisoned)?;
        let thread_handle = std::mem::take(&mut *thread_handle).ok_or(IETStorageError::NoHandle)?;
        self.finished.store(true, Release);
        Ok(Some(thread_handle.join().map_err(IETStorageError::ThreadJoin)?))
    }
}
impl IETStorage<std::net::TcpStream> {
    pub fn init_server(listener: TcpListener, length_prefixed: bool) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        let thread_handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("IETStorage: Failed to accept stream");
            Self::writer_thread_main(stream, length_prefixed, rx)
        });
        IETStorage {
            sender: tx,
            thread_handle: RwLock::new(Some(thread_handle)),
            finished: false.into(),
        }
    }
}
impl<T: Write + Send + 'static> Storage for IETStorage<T> {
    fn new_span(
        &self, id: u32, parent: u32, attr_names: Vec<String>, attr_values: Vec<EnValue>,
        meta: &'static tracing::Metadata<'_>,
    ) {
        let mut entry =
            TraceEntry::from_unsorted_attrs(parent, None, meta.into(), attr_names, attr_values);
        if let Some(val) = entry.as_ref().get_attr("message")
            && let EnValueRef::String(s) = val
        {
            entry.message = Some(s.to_string())
        }
        self.sender.send(RemoteMessage::NewSpan { id, entry }).ok();
    }
}

impl<T> Drop for IETStorage<T> {
    fn drop(&mut self) {
        self.finish().ok();
    }
}
