use std::{
    cell::RefCell,
    fmt::Display,
    path::PathBuf,
    sync::{Arc, RwLock},
};

use entrace_core::{
    LogProvider, display_error_context,
    remote::{IETEvent, Notify, NotifyExt},
};
use tracing::{info, trace};

use crate::{
    benchmarkers::SamplingBenchmark,
    enbitvec::EnBitVec,
    search::LocatingState,
    tree::{TreeContext, TreeView},
};

// we aren't storing multiple of these, so it's fine
#[allow(clippy::large_enum_variant)]
pub enum LogStatus {
    NoFileOpened,
    Error(anyhow::Error),
    Loading(crossbeam::channel::Receiver<LogStatus>),
    Ready(LogState),
}
impl Display for LogStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogStatus::NoFileOpened => write!(f, "LogStatus::NoFileOpened"),
            LogStatus::Error(error) => write!(f, "LogStatus::Error {error}"),
            LogStatus::Loading(_rx) => write!(f, "LogStatus::Loading"),
            LogStatus::Ready(_l_state) => write!(f, "LogStatus::Ready"),
        }
    }
}
/// TODO: try to remove this box by making LogProvider Sized
pub type TraceProvider = Box<dyn LogProvider + Send + Sync>;
pub struct LogState {
    pub file_path: PathBuf,
    pub trace_provider: Arc<RwLock<TraceProvider>>,
    /// Used for culling.
    pub is_open: EnBitVec,
    pub meta_open: EnBitVec,
    pub locating_state: RefCell<LocatingState>,
    pub tree_view: TreeView,
    pub event_rx: Option<crossbeam::channel::Receiver<IETEvent>>,
}
impl LogState {
    pub fn update_tree<const N: u8>(&mut self, tree_benchmark: &mut SamplingBenchmark<N>) {
        let locating_writer = self.locating_state.get_mut();

        match locating_writer {
            LocatingState::None => (),
            LocatingState::Started(started) => {
                if let Some(q) = started.poll() {
                    *locating_writer = LocatingState::ScrollTo {
                        target: started.target,
                        path: q,
                        target_row_offset: None,
                        opened_path: false,
                    }
                }
            }
            LocatingState::ScrollTo { path, opened_path, .. } if !*opened_path => {
                for component in path {
                    self.is_open.set(*component as usize, true);
                }
                self.tree_view.invalidate();
                info!("Opened everything on the locate path");
                *opened_path = true;
            }
            LocatingState::ScrollTo { .. } => (),
            LocatingState::Highlight(_) => (),
        }
        let log_reader = self.trace_provider.read().unwrap();
        let ctx = TreeContext {
            log_reader: &log_reader,
            open_reader: &self.is_open,
            meta_open_reader: &self.meta_open,
            locating_state: Some(locating_writer),
        };
        self.tree_view.update_tree(Some(tree_benchmark), std::iter::once(0), ctx);
    }
    /// Returns the delta in the trace provider's item count
    pub fn on_frame(&self, notifier: &impl Notify) -> usize {
        let mut delta = 0;
        if let Ok(mut q) = self.trace_provider.try_write() {
            let len0 = q.len();
            q.frame_callback();
            delta = q.len().saturating_sub(len0);
        } else {
            trace!(
                "Can't acquire write lock on trace provider, next frame_callback will be delayed"
            )
        }
        if let Some(ref rx) = self.event_rx {
            while let Ok(y) = rx.try_recv() {
                match y {
                    IETEvent::Error(err) => notifier.error(display_error_context(&err)),
                    IETEvent::Info(i) => notifier.info(i.to_string()),
                }
            }
        }

        delta
    }
}
