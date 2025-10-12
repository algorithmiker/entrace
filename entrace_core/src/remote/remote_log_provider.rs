use crate::remote::IETInfo;
use crate::tree_layer::EnValueRef;
use crate::{LogProviderError, remote::IETEvent};
use std::{
    io::{BufRead, BufReader, Read},
    net::{TcpListener, TcpStream},
    ops::ControlFlow,
    time::Duration,
};

use crate::{
    Header, IETPresentationConfig, MetadataRefContainer, TraceEntry,
    log_provider::{LogProvider, LogProviderResult},
    remote::{BaseIETLogProvider, MainThreadMessage, Refresh},
};
use crossbeam_channel::Sender;

enum ReadState {
    WantMagic,
    WantMessage,
}

struct RemoteWorkerState<'a, R: Refresh> {
    event_tx: Option<crossbeam_channel::Sender<IETEvent>>,
    refresher: R,
    reader: BufReader<&'a mut TcpStream>,
    tx: Sender<MainThreadMessage>,
    read_state: ReadState,
    event_buf: Vec<TraceEntry>,
    byte_buf: Vec<u8>,
}
impl<'a, R: Refresh> RemoteWorkerState<'a, R> {
    pub fn new(
        event_tx: Option<crossbeam_channel::Sender<IETEvent>>, refresher: R,
        reader: BufReader<&'a mut TcpStream>, tx: Sender<MainThreadMessage>, read_state: ReadState,
    ) -> RemoteWorkerState<'a, R> {
        Self {
            refresher,
            reader,
            tx,
            read_state,
            event_buf: Vec::with_capacity(512),
            byte_buf: Vec::with_capacity(1024),
            event_tx,
        }
    }
    pub fn send_event_buf(&mut self) {
        use MainThreadMessage::*;
        match self.event_buf.len() {
            0 => (),
            1 => {
                let msg = Insert(self.event_buf.pop().unwrap());
                self.tx.send(msg).unwrap();
                self.refresher.refresh();
            }
            _x => {
                let old_event_buf = std::mem::replace(&mut self.event_buf, Vec::with_capacity(512));
                let msg = InsertMany(old_event_buf);
                self.tx.send(msg).unwrap();
                self.refresher.refresh();
            }
        }
    }
    const SHORT_TIMEOUT: Option<Duration> = Some(Duration::from_millis(50));
    pub fn set_short_timeout(&mut self) -> Result<(), LogProviderError> {
        Ok(self.reader.get_ref().set_read_timeout(Self::SHORT_TIMEOUT)?)
    }
    pub fn set_no_timeout(&mut self) -> Result<(), LogProviderError> {
        Ok(self.reader.get_ref().set_read_timeout(None)?)
    }

    pub fn block_on_data(&mut self) -> Result<(), LogProviderError> {
        self.set_no_timeout()?;
        self.reader.fill_buf()?;
        self.set_short_timeout()
    }
    pub fn info(&self, i: IETInfo) {
        if let Some(x) = &self.event_tx {
            x.send(IETEvent::Info(i)).ok();
        }
    }
    pub fn err(&self, e: LogProviderError) {
        if let Some(x) = &self.event_tx {
            x.send(IETEvent::Error(e)).ok();
        }
    }
    pub fn read_loop_body(&mut self) -> ControlFlow<Option<LogProviderError>> {
        let cfg = bincode::config::standard();
        match self.read_state {
            ReadState::WantMagic => {
                let mut header_buf = [0; 10];
                if let Err(y) = self.reader.read_exact(&mut header_buf) {
                    self.err(y.into());
                } else {
                    self.read_state = ReadState::WantMessage;
                }
            }
            ReadState::WantMessage => {
                // buffer for the content-length
                let mut cl_buf = [0; 8];
                if let Err(y) = self.reader.read_exact(&mut cl_buf) {
                    use std::io::ErrorKind::*;
                    if matches!(y.kind(), WouldBlock | TimedOut) {
                        self.send_event_buf();
                        if let Err(y) = self.block_on_data() {
                            self.err(y);
                        }
                        return ControlFlow::Continue(());
                    } else if matches!(y.kind(), UnexpectedEof) {
                        self.info(IETInfo::RemoteClosedConnection);
                        self.send_event_buf();
                        self.refresher.refresh();
                        return ControlFlow::Break(None);
                    } else {
                        self.err(y.into());
                    }
                }

                let content_len = u64::from_le_bytes(cl_buf);
                // Today's BufReader api doesn't allow to block until we have a specific number of
                // bytes is available, except for `read_exact`.
                // (Because `fill_buf` won't fill anything if there is data left in the buffer).
                // Therefore we are technically copying things twice here, once to BufReader's
                // internal buffer, then to ours.
                // This is still worth it performance-wise, if the client pushes a large amount of
                // data onto the stream, since on this end we consume it message-by-message.
                //
                // An alternative could be to use bincode::serde::decode_from_std_read, but we
                // choose to be paranoid about the content-len here.
                // See also: https://graphallthethings.com/posts/better-buf-read
                self.byte_buf.clear();
                self.byte_buf.resize(content_len as usize, 0);
                if let Err(y) = self.reader.read_exact(&mut self.byte_buf) {
                    return ControlFlow::Break(Some(y.into()));
                };
                let decoded: Result<(TraceEntry, usize), _> =
                    bincode::serde::decode_from_slice(&self.byte_buf, cfg);
                match decoded {
                    Ok(x) => self.event_buf.push(x.0),
                    Err(y) => self.err(y.into()),
                }
            }
        }
        ControlFlow::Continue(())
    }
}
#[derive(thiserror::Error, Debug)]
pub enum RemoteLogProviderError {
    #[error("Server sees a connection, but cannot establish a TCPStream. Quitting.")]
    CannotAccept(#[source] std::io::Error),
}
/// Provides a [crate::log_provider::LogProvider] based on incoming data from a TCP stream.
pub struct RemoteLogProvider(BaseIETLogProvider);
impl RemoteLogProvider {
    pub fn new<R: Refresh + Send + 'static>(
        listener: TcpListener, config: IETPresentationConfig<R>,
    ) -> Self {
        fn worker<R: Refresh + Send>(
            listener: TcpListener, tx: Sender<MainThreadMessage>, config: IETPresentationConfig<R>,
        ) {
            let IETPresentationConfig { refresher, event_tx } = config;
            let info = |i| {
                if let Some(q) = &event_tx {
                    q.send(IETEvent::Info(i)).ok();
                }
            };
            let err = |e| {
                if let Some(q) = &event_tx {
                    q.send(IETEvent::Error(e)).ok();
                }
            };

            info(IETInfo::ServerStarted);
            // block until someone connects
            let (mut stream, _socket) = match listener.accept() {
                Ok((stream, socket)) => (stream, socket),
                Err(y) => {
                    err(RemoteLogProviderError::CannotAccept(y).into());
                    refresher.refresh();
                    return;
                }
            };
            info(IETInfo::ReceivedConnection);
            refresher.refresh();
            let reader = BufReader::new(&mut stream);
            let mut state =
                RemoteWorkerState::new(event_tx, refresher, reader, tx, ReadState::WantMagic);
            if let Err(y) = state.set_short_timeout() {
                state.err(y);
            }
            loop {
                match state.read_loop_body() {
                    ControlFlow::Continue(_) => (),
                    ControlFlow::Break(Some(y)) => {
                        state.err(y);
                        break;
                    }
                    ControlFlow::Break(None) => break,
                }
            }
        }
        let base = BaseIETLogProvider::new(listener, config, worker);
        Self(base)
    }
}
impl LogProvider for RemoteLogProvider {
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
        self.0.frame_callback()
    }

    fn len(&self) -> usize {
        self.0.len()
    }
}
