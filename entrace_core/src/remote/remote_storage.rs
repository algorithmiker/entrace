use crate::{StorageFormat, TraceEntry, entrace_magic_for, storage::Storage, tree_layer::EnValue};
use crossbeam::channel::{SendError, Sender};
use std::{any::Any, io::Write, sync::RwLock, thread::JoinHandle};

pub enum RemoteMessage {
    NewSpan(TraceEntry),
    Shutdown,
}
pub struct IETStorageConfig<T: Write + Send> {
    writable: T,
    length_prefixed: bool,
}
impl<T: Write + Send> IETStorageConfig<T> {
    /// Recommended for [std::net::TcpStream] or [`std::io::BufWriter<std::net::TcpStream>`]
    pub fn length_prefixed(writable: T) -> Self {
        Self { writable, length_prefixed: true }
    }

    /// Recommended for [std::fs::File] or [std::io::BufWriter<File>]
    pub fn non_length_prefixed(writable: T) -> Self {
        Self { writable, length_prefixed: false }
    }
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
pub struct IETStorage<T: Write + Send + 'static> {
    pub sender: Sender<RemoteMessage>,
    pub thread_handle: RwLock<Option<JoinHandle<T>>>,
}
impl<T: Write + Send + 'static> IETStorage<T> {
    pub fn init(mut config: IETStorageConfig<T>) -> Self {
        let (tx, rx) = crossbeam::channel::unbounded();
        let format =
            if config.length_prefixed { StorageFormat::IETPrefix } else { StorageFormat::IET };
        let thread_handle = std::thread::spawn(move || {
            let magic = entrace_magic_for(1, format);
            config.writable.write_all(&magic).unwrap();
            let mut buffer: Vec<u8> = Vec::with_capacity(1024);
            /// Write a length-prefixed message.
            fn write_message<T: Write + Send>(
                buffer: &mut Vec<u8>, message: TraceEntry, config: &mut IETStorageConfig<T>,
            ) {
                let bcfg = bincode::config::standard();
                if config.length_prefixed {
                    buffer.clear();
                    bincode::serde::encode_into_std_write(message, buffer, bcfg).unwrap();

                    config.writable.write_all(&(buffer.len() as u64).to_le_bytes()).unwrap();
                    std::io::copy(&mut buffer.as_slice(), &mut config.writable).unwrap();
                } else {
                    bincode::serde::encode_into_std_write(message, &mut config.writable, bcfg)
                        .unwrap();
                }
            }

            write_message(&mut buffer, TraceEntry::root(), &mut config);
            while let Ok(msg) = rx.recv() {
                match msg {
                    RemoteMessage::NewSpan(m) => {
                        write_message(&mut buffer, m, &mut config);
                    }
                    RemoteMessage::Shutdown => break,
                }
            }
            config.writable.flush().ok();
            config.writable
        });
        IETStorage { sender: tx, thread_handle: RwLock::new(Some(thread_handle)) }
    }

    pub fn finish(&self) -> Result<T, IETStorageError> {
        self.sender.send(RemoteMessage::Shutdown).map_err(Box::new)?;
        let mut thread_handle =
            self.thread_handle.write().map_err(|_| IETStorageError::Poisoned)?;
        let thread_handle = std::mem::take(&mut *thread_handle).ok_or(IETStorageError::NoHandle)?;
        thread_handle.join().map_err(IETStorageError::ThreadJoin)
    }
}
impl<T: Write + Send + 'static> Storage for IETStorage<T> {
    fn new_span(&self, parent: u32, attrs: crate::Attrs, meta: &'static tracing::Metadata<'_>) {
        let message = attrs.iter().find(|x| x.0 == "message").map(|x| match &x.1 {
            EnValue::String(y) => y.clone(),
            q => format!("{q:?}"),
        });
        self.sender
            .send(RemoteMessage::NewSpan(TraceEntry {
                parent,
                message,
                metadata: meta.into(),
                attributes: attrs,
            }))
            .ok();
    }
}

impl<T: Write + Send + 'static> Drop for IETStorage<T> {
    fn drop(&mut self) {
        self.finish().ok();
    }
}
