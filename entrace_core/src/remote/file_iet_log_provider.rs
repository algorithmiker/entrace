use crate::IETLoadConfig;
use crate::LogProviderError;
#[cfg(feature = "notify-watch")]
use crate::remote::IETInfo;
use crate::tree_layer::EnValueRef;
use std::{
    fs::File,
    io::{self, BufReader, Read, Seek, SeekFrom},
    path::PathBuf,
    time::Instant,
};

use bincode::error::DecodeError;
use crossbeam_channel::Sender;
use tracing::trace;
use tracing::{error, info};

use crate::{
    Header, IETPresentationConfig, MetadataRefContainer, PoolEntry, TraceEntry,
    log_provider::{LogProvider, LogProviderResult},
    remote::{BaseIETLogProvider, MainThreadMessage, Refresh},
};
#[derive(Debug, thiserror::Error)]
pub enum LoadIETError {
    #[error(transparent)]
    DecodeError(#[from] bincode::error::DecodeError),
    #[error(
        "The pool length ({pool_len}) doesn't equal the data length {data_len}. The IET file is \
         most likely corrupt."
    )]
    LengthMismatch { data_len: usize, pool_len: usize },
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error("Want to watch a file, but you didn't enable the notify-watch feature in entrace_core")]
    NotifyNeeded,
}

pub struct InitialIETData {
    pub pool: Vec<PoolEntry>,
    pub data: Vec<TraceEntry>,
}

/// Load an IET trace, from a reader which has data immediately available (like a file).
///
/// `length_prefixed` is usually `false` for files and `true` for streams.
///
/// To get something that implements [LogProvider], see [FileIETLogProvider::new]
pub fn load_iet_trace(
    mut reader: impl std::io::Read, length_prefixed: bool,
) -> Result<InitialIETData, LoadIETError> {
    let cfg = bincode::config::standard();
    let mut pool: Vec<PoolEntry> = vec![PoolEntry::new()];
    // no root entry here, the client has to send it.
    let mut data = vec![];
    let mut had_root = false;
    loop {
        if length_prefixed {
            let mut cl_buf = [0; 8];
            if let Err(y) = reader.read_exact(&mut cl_buf) {
                if y.kind() == std::io::ErrorKind::UnexpectedEof {
                    break;
                } else {
                    return Err(LoadIETError::IO(y));
                }
            }
        }
        // TODO: mabye be paranoid here, and only read up to content-len.
        let decoded: Result<TraceEntry, _> = bincode::serde::decode_from_std_read(&mut reader, cfg);
        match decoded {
            Ok(x) => {
                let pl = pool.len() as u32;
                // we are pushing a pool entry with "delayed" data for the root, account
                // for this.
                if had_root {
                    pool[x.parent as usize].children.push(pl);
                    pool.push(PoolEntry::new())
                }
                data.push(x);
                had_root = true;
            }
            Err(y) => match y {
                DecodeError::Io { inner, .. } if inner.kind() == io::ErrorKind::UnexpectedEof => {
                    break;
                }
                _ => return Err(LoadIETError::DecodeError(y)),
            },
        }
    }

    let data_len = data.len();
    let pool_len = pool.len();
    if data.len() != pool.len() {
        return Err(LoadIETError::LengthMismatch { data_len, pool_len });
    }
    Ok(InitialIETData { pool, data })
}

pub enum FileWatchConfig {
    DontWatch,
    Watch(PathBuf),
}
pub struct FileIETLogProvider(BaseIETLogProvider);
impl FileIETLogProvider {
    pub fn new<R>(
        mut file: File, load_config: IETLoadConfig<R>, length_prefixed: bool,
    ) -> Result<Self, LoadIETError>
    where
        R: Refresh + Send + 'static,
    {
        let mut reader = BufReader::new(&mut file);

        let start = Instant::now();
        let initial = load_iet_trace(&mut reader, length_prefixed)?;
        info!(duration = ?start.elapsed(), "RemoteLogProvider: loaded initial iet file");

        let worker_thread = move |mut file2, tx: Sender<_>, config2: IETPresentationConfig<R>| {
            tx.send(MainThreadMessage::ReplaceData(initial.data)).unwrap();
            tx.send(MainThreadMessage::ReplacePool(initial.pool)).unwrap();

            match load_config.watch {
                FileWatchConfig::DontWatch => (),
                FileWatchConfig::Watch(file_path) => {
                    #[cfg(feature = "notify-watch")]
                    {
                        let mut reader = BufReader::new(&mut file2);
                        let mut worker =
                            IETNotifyWorker::new(tx, &mut reader, file_path, config2, false);
                        if let Err(y) = worker.work() {
                            if let LogProviderError::FileIETError(ref yy) = y
                                && yy.is_fatal()
                            {
                                worker.send_err(y);
                                return;
                            }
                            worker.send_err(y);
                        }
                    }
                    #[cfg(not(feature = "notify-watch"))]
                    {
                        if let Some(etx) = &config2.event_tx {
                            use crate::remote::IETEvent;
                            etx.send(IETEvent::Error(FileIETError::NeedNotify.into())).ok();
                        }
                        return;
                    }
                }
            }
        };
        let base = BaseIETLogProvider::new(file, load_config.presentation, worker_thread);
        Ok(Self(base))
    }
}
#[derive(thiserror::Error, Debug)]
pub enum FileIETError {
    #[error("Wanted to watch a file, but the notify-watch feature of entrace_core is not enabled")]
    NeedNotify,
    #[cfg(feature = "notify-watch")]
    #[error(transparent)]
    NotifyError(#[from] notify::Error),
    #[error("Failed to read incoming data after 8 retries")]
    NoMoreRetries(#[from] bincode::error::DecodeError),
}
impl FileIETError {
    pub fn is_fatal(&self) -> bool {
        match self {
            FileIETError::NeedNotify => true,
            #[cfg(feature = "notify-watch")]
            FileIETError::NotifyError(_) => false,
            FileIETError::NoMoreRetries(_) => true,
        }
    }
}
pub enum ReadState {
    Standby,
    Retrying { retries: u16 },
}

#[cfg(feature = "notify-watch")]
pub struct IETNotifyWorker<'a, F: Read + Seek, R: Refresh> {
    tx: Sender<MainThreadMessage>,
    file_path: PathBuf,
    cfg: IETPresentationConfig<R>,
    length_prefixed: bool,

    reader: &'a mut F,
    last_good_position: u64,
    read_state: ReadState,
    entries: Vec<TraceEntry>,
}
#[cfg(feature = "notify-watch")]
impl<'a, R: Refresh, F: Read + Seek> IETNotifyWorker<'a, F, R> {
    pub fn new(
        tx: Sender<MainThreadMessage>, reader: &'a mut F, file_path: PathBuf,
        config: IETPresentationConfig<R>, length_prefixed: bool,
    ) -> Self {
        let last_good_position = reader.stream_position().unwrap();
        Self {
            tx,
            cfg: config,
            file_path,
            length_prefixed,
            last_good_position,
            reader,
            read_state: ReadState::Standby,
            entries: vec![],
        }
    }
    pub fn send_err(&self, err: LogProviderError) {
        if let Some(ref tx) = self.cfg.event_tx {
            use crate::remote::IETEvent;
            tx.send(IETEvent::Error(err)).ok();
        }
    }
    pub fn info(&self, i: IETInfo) {
        if let Some(ref tx) = self.cfg.event_tx {
            use crate::remote::IETEvent;
            tx.send(IETEvent::Info(i)).unwrap();
        }
    }

    pub fn send_entries(&mut self) {
        match self.entries.len() {
            0 => (),
            1 => {
                let pop = self.entries.pop().unwrap();
                self.tx.send(MainThreadMessage::Insert(pop)).unwrap();
                self.cfg.refresher.refresh();
            }
            x => {
                self.tx
                    .send(MainThreadMessage::InsertMany(std::mem::take(&mut self.entries)))
                    .unwrap();
                self.cfg.refresher.refresh();
                println!("Sent batch of {x}");
            }
        }
    }
    pub fn on_modify(&mut self) -> Result<(), LogProviderError> {
        let cfg = bincode::config::standard();

        loop {
            if self.length_prefixed {
                let mut cl_buf = [0; 8];
                if let Err(y) = self.reader.read_exact(&mut cl_buf) {
                    if y.kind() == std::io::ErrorKind::UnexpectedEof {
                        // wait for next wake up on new data
                        break;
                    } else {
                        return Err(LogProviderError::IO(y));
                    }
                }
                //let content_len = u64::from_le_bytes(cl_buf);
            }

            let decoded: Result<TraceEntry, _> =
                bincode::serde::decode_from_std_read(&mut self.reader, cfg);
            match decoded {
                Ok(x) => {
                    self.entries.push(x);
                    if self.entries.len() > 32 {
                        self.send_entries();
                    }

                    self.last_good_position = self.reader.stream_position()?;
                    self.read_state = ReadState::Standby;
                }
                Err(y) => {
                    if matches!(y, bincode::error::DecodeError::UnexpectedEnd { .. })
                        || matches!(y,  bincode::error::DecodeError::Io { ref inner, .. } if inner.kind() == std::io::ErrorKind::UnexpectedEof)
                    {
                        //warn!(err=%y,"file ended before reading everyhting, seeking backk");
                        self.reader.seek(SeekFrom::Start(self.last_good_position)).unwrap();
                        self.cfg.refresher.refresh();
                        break; // wait for the next wake up
                    } else {
                        // this could still be an incomplete write
                        if let ReadState::Retrying { ref mut retries } = self.read_state {
                            if *retries > 8 {
                                self.send_entries();
                                return Err(FileIETError::NoMoreRetries(y).into());
                            } else {
                                *retries += 1;
                            };
                        }
                    }
                }
            }
        }
        self.send_entries();
        Ok(())
    }

    pub fn work(&mut self) -> Result<(), LogProviderError> {
        use notify::{EventKind, Watcher, event::ModifyKind};
        info!("FileIETLogProvider worker start");
        let (atx, arx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
        let mut watcher = notify::recommended_watcher(atx).map_err(FileIETError::NotifyError)?;
        watcher.watch(self.file_path.as_path(), notify::RecursiveMode::NonRecursive).ok();
        info!("Setting up file watcher for IET file");

        loop {
            match arx.recv() {
                Ok(Ok(x)) => {
                    if let EventKind::Modify(ModifyKind::Data(_)) = x.kind {
                        trace!("IET file watcher fired");
                        if let Err(y) = self.on_modify() {
                            self.send_err(y);
                        }
                    }
                }
                x => error!(error=?x,"File watcher error"),
            }
        }
    }
}
impl LogProvider for FileIETLogProvider {
    fn children(&self, x: u32) -> LogProviderResult<&[u32]> {
        self.0.children(x)
    }

    fn parent(&self, x: u32) -> LogProviderResult<u32> {
        self.0.parent(x)
    }

    fn attrs(&'_ self, x: u32) -> LogProviderResult<Vec<(&'_ str, EnValueRef<'_>)>> {
        self.0.attrs(x)
    }

    fn header(&'_ self, x: u32) -> LogProviderResult<Header<'_>> {
        self.0.header(x)
    }

    fn meta(&'_ self, x: u32) -> LogProviderResult<MetadataRefContainer<'_>> {
        self.0.meta(x)
    }

    fn frame_callback(&mut self) {
        self.0.frame_callback();
    }

    fn len(&self) -> usize {
        self.0.len()
    }
}
