use std::{thread::JoinHandle, time::Duration};

use crate::{
    Header, IETPresentationConfig, LevelContainer, MetadataRefContainer, PoolEntry, TraceEntry,
    log_provider::{LogProvider, LogProviderError, LogProviderResult},
    tree_layer::EnValueRef,
};
use crossbeam::channel::{Receiver, Sender};

mod file_iet_log_provider;
pub use file_iet_log_provider::*;
mod remote_storage;
pub use remote_storage::*;
mod remote_log_provider;
pub use remote_log_provider::*;

#[derive(derive_more::Display)]
pub enum IETInfo {
    #[display("Server started, waiting for connections")]
    ServerStarted,
    #[display("Received connection")]
    ReceivedConnection,
    #[display("Remote client closed connection")]
    RemoteClosedConnection,
}
pub enum IETEvent {
    Error(LogProviderError),
    Info(IETInfo),
}
// TODO: shrink MainThreadMessage, because 168 bytes makes the event_buf allocation very large
pub enum MainThreadMessage {
    Insert(TraceEntry),
    InsertMany(Vec<TraceEntry>),
    ReplacePool(Vec<PoolEntry>),
    ReplaceData(Vec<TraceEntry>),
}

pub struct BaseIETLogProvider {
    pub handle: JoinHandle<()>,
    pub receiver: Receiver<MainThreadMessage>,
    // TODO: memory representation could likely be more concise
    pub pool: Vec<PoolEntry>,
    pub data: Vec<TraceEntry>,
}

impl BaseIETLogProvider {
    pub fn new<T, R: Refresh + Send + 'static>(
        buf: T, config: IETPresentationConfig<R>,
        worker_thread: impl FnOnce(T, Sender<MainThreadMessage>, IETPresentationConfig<R>)
        + 'static
        + Send,
    ) -> Self
    where
        T: Send + Sync + 'static,
        // Notifier: Notify + Send + 'static,
        // Refresher: Refresh + Send + 'static,
    {
        let (tx, rx) = crossbeam::channel::unbounded();
        let handle = std::thread::spawn(move || worker_thread(buf, tx, config));
        // no root data entry here, the client has to send it.
        Self { handle, receiver: rx, pool: vec![], data: vec![] }
    }
}
impl LogProvider for BaseIETLogProvider {
    fn children(&self, x: u32) -> LogProviderResult<&[u32]> {
        self.pool
            .get(x as usize)
            .map(|x| x.children.as_slice())
            .ok_or(LogProviderError::OutOfBounds)
    }

    fn parent(&self, x: u32) -> LogProviderResult<u32> {
        self.data.get(x as usize).map(|x| x.parent).ok_or(LogProviderError::OutOfBounds)
    }

    fn attrs(&'_ self, x: u32) -> LogProviderResult<Vec<(&'_ str, EnValueRef<'_>)>> {
        // HACK: maybe this should return an iterator instead
        // TODO: figure out if that will affect search
        // not high priority since attrs are only displayed on demand
        self.data
            .get(x as usize)
            .map(|x| x.attributes.iter().map(|(x, y)| (x.as_str(), y.as_ref())).collect())
            .ok_or(LogProviderError::OutOfBounds)
    }

    fn header(&'_ self, x: u32) -> LogProviderResult<Header<'_>> {
        let y = self.data.get(x as usize).ok_or(LogProviderError::OutOfBounds)?;
        let h = Header {
            name: &y.metadata.name,
            level: y.metadata.level,
            file: y.metadata.file.as_deref(),
            line: y.metadata.line,
            message: y.message.as_deref(),
        };
        Ok(h)
    }

    fn meta(&'_ self, x: u32) -> LogProviderResult<MetadataRefContainer<'_>> {
        self.data.get(x as usize).map(|x| x.metadata.as_ref()).ok_or(LogProviderError::OutOfBounds)
    }

    fn frame_callback(&mut self) {
        // TODO: make configurable ( maybe an interface for Storage to provide extra settings in
        // the dialog ? )
        #[allow(non_snake_case)]
        let N = 50;
        for _ in 0..N {
            match self.receiver.try_recv() {
                Ok(msg) => {
                    use MainThreadMessage::Insert;
                    match msg {
                        Insert(event) => {
                            let pl = self.pool.len() as u32;

                            self.pool.push(PoolEntry::new());
                            if pl != 0 {
                                self.pool[event.parent as usize].children.push(pl);
                            }
                            self.data.push(event);
                        }
                        MainThreadMessage::ReplacePool(pool) => self.pool = pool,
                        MainThreadMessage::ReplaceData(data) => self.data = data,
                        MainThreadMessage::InsertMany(events) => {
                            let old_pl = self.pool.len();
                            self.pool.extend(std::iter::repeat_n(PoolEntry::new(), events.len()));
                            for (idx, event) in events.iter().enumerate() {
                                let idx = idx + old_pl;
                                if idx != 0 {
                                    self.pool[event.parent as usize].children.push(idx as u32);
                                }
                            }
                            self.data.extend(events.into_iter());
                        }
                    }
                }
                Err(y) => match y {
                    crossbeam::channel::TryRecvError::Empty => (),
                    crossbeam::channel::TryRecvError::Disconnected => (),
                },
            }
        }
    }

    fn len(&self) -> usize {
        self.data.len()
    }
}

pub trait Refresh {
    /// A way of signaling from entrace to the consuming library that the data has changed
    fn refresh(&self);
}
pub struct DummyRefresher {}
impl Refresh for DummyRefresher {
    fn refresh(&self) {}
}

pub trait Notify {
    fn add_notification(&self, severity: LevelContainer, text: String, duration: Duration);
    fn remove_notification(&self, idx: usize);
}
pub trait NotifyExt {
    fn info(&self, text: impl Into<String>);
    fn error(&self, text: impl Into<String>);
}

impl<T: Notify> NotifyExt for T {
    fn info(&self, text: impl Into<String>) {
        self.add_notification(LevelContainer::Info, text.into(), Duration::from_secs(5));
    }

    fn error(&self, text: impl Into<String>) {
        self.add_notification(LevelContainer::Error, text.into(), Duration::MAX);
    }
}

pub struct StderrNotifier {}
impl Notify for StderrNotifier {
    fn add_notification(&self, severity: LevelContainer, text: String, _duration: Duration) {
        let level = match severity {
            LevelContainer::Trace => "[T]",
            LevelContainer::Debug => "[D]",
            LevelContainer::Info => "[I]",
            LevelContainer::Warn => "[W]",
            LevelContainer::Error => "[E]",
        };
        eprintln!("entrace notification: {level}: {text}",);
    }

    fn remove_notification(&self, _idx: usize) {}
}

pub struct DummyNotifier {}
impl Notify for DummyNotifier {
    fn add_notification(&self, _severity: LevelContainer, _text: String, _duration: Duration) {}
    fn remove_notification(&self, _idx: usize) {}
}
