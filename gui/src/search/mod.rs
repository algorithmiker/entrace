mod bottom_panel;
pub mod query_window;
pub mod segmented_button;
pub use bottom_panel::*;
use std::{
    cell::RefCell,
    collections::HashMap,
    fmt::Debug,
    num::NonZero,
    ops::{Deref, RangeInclusive},
    rc::Rc,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use crate::{search::query_window::PaginatedResults, spawn_task};
use crossbeam::channel::Receiver;
use egui::{Pos2, Rect};

use entrace_core::{LogProvider, LogProviderError, LogProviderImpl};
use entrace_query::{
    QueryError,
    lua_api::{JoinCtx, LuaEvalState, setup_lua_on_arc_rwlock},
};
use mlua::{FromLua, Lua, Value};
use tracing::{error, info};
#[derive(Debug, Clone)]
pub struct PartialQueryResult {
    pub ids: Vec<u32>,
}
#[derive(Debug)]
pub struct QueryResult {
    pub ids: Vec<u32>,
    pub pages: PaginatedResults,
}
#[derive(Debug)]
pub enum Query {
    Loading {
        id: u16,
        rx: crossbeam::channel::Receiver<(Result<QueryResult, QueryError>, Duration)>,
    },
    Completed {
        id: u16,
        result: Result<QueryResult, QueryError>,
    },
}
impl Query {
    pub fn id(&self) -> u16 {
        match self {
            Query::Loading { id, .. } => *id,
            Query::Completed { id, .. } => *id,
        }
    }
}
pub enum QueryTiming {
    Loading(Instant),
    Finished(Duration),
}
impl QueryTiming {
    pub fn unwrap(&self) -> &Duration {
        match self {
            QueryTiming::Loading(_instant) => {
                panic!("QueryTiming::Unwrap called on a Loading value")
            }
            QueryTiming::Finished(duration) => duration,
        }
    }
}

enum QuerySettingsDialogData {
    Closed,
    Open { settings_button_rect: Rect, position: Option<Pos2> },
}
pub struct QuerySettings {
    data: QuerySettingsDialogData,
    num_threads: u8,
}

impl QuerySettings {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let num_cpus =
            std::thread::available_parallelism().unwrap_or_else(|_| NonZero::new(2).unwrap());
        let num_cpus = num_cpus.get() as u8;
        QuerySettings { num_threads: num_cpus, data: QuerySettingsDialogData::Closed }
    }
    pub fn is_open(&self) -> bool {
        match self.data {
            QuerySettingsDialogData::Closed => false,
            QuerySettingsDialogData::Open { .. } => true,
        }
    }
}

pub struct SearchState {
    pub settings: QuerySettings,
    pub text: SearchTextState,
    pub queries: Vec<Query>,
    pub last_id: u16,
    pub query_window_open: Vec<bool>,
    pub query_timing: Vec<QueryTiming>,
}
impl SearchState {
    pub fn new_query(&mut self, trace_provider: Arc<RwLock<LogProviderImpl>>) {
        let (tx, rx) = crossbeam::channel::bounded(1);
        let new_id = self.last_id + 1;
        self.last_id += 1;
        self.queries.push(Query::Loading { id: new_id, rx });
        self.query_window_open.push(true);
        self.query_timing.push(QueryTiming::Loading(Instant::now()));
        let text_arc: Arc<str> = Arc::from(self.text.text.as_str());
        let tp = trace_provider.clone();
        let mut threads = self.settings.num_threads as u32;
        std::thread::spawn(move || {
            let query_start = Instant::now();
            // Controller thread
            let spans_len = { trace_provider.read().unwrap().len() } as u32;
            let mut items_per_thread = spans_len / threads;
            info!(
                "spans_len: {spans_len}, threads: {threads} -> items per thread: {items_per_thread}"
            );
            if items_per_thread == 0 {
                threads = 1;
                items_per_thread = spans_len;
                info!("Less items to query than threads, setting threads=1");
            }
            let mut ranges: Vec<RangeInclusive<u32>> = (0u32..threads)
                .map(|x| (x * items_per_thread)..=(x + 1) * items_per_thread - 1)
                .collect();
            if let Some(last) = ranges.last_mut() {
                *last = *last.start()..=spans_len.saturating_sub(1); // exclusive range
            }
            info!("Ranges for jobs: {ranges:?}");

            let join_ctx = JoinCtx::from_thread_count(threads as usize);
            let join_ctx_arc = Arc::new(join_ctx);

            let rv = std::iter::repeat_with(|| None).take(threads as usize).collect();
            #[allow(clippy::type_complexity)]
            let results: Arc<
                RwLock<Vec<Option<Result<PartialQueryResult, QueryError>>>>,
            > = Arc::new(RwLock::new(rv));
            std::thread::scope(|f| {
                for i in 0..threads {
                    let ta = text_arc.clone();
                    let trace_provider = tp.clone();
                    let range = ranges[i as usize].clone();
                    let results2 = results.clone();
                    let join_ctx_local = join_ctx_arc.clone();
                    f.spawn(move || {
                        let finder_cache = Rc::new(RefCell::new(HashMap::new()));
                        let mut lua = Lua::new();
                        let lua_state = LuaEvalState::new(join_ctx_local, range, finder_cache);
                        if let Err(y) = setup_lua_on_arc_rwlock(&mut lua, trace_provider, lua_state)
                        {
                            let mut rw = results2.write().unwrap();
                            rw[i as usize] = Some(Err(QueryError::LuaError(y)));
                        }

                        let start = Instant::now();
                        let loaded: Result<Value, _> =
                            lua.load(&*ta).set_name("search query").eval();
                        info!(elapsed = ?start.elapsed(), "Thread {i} done");
                        match loaded {
                            Ok(x) => {
                                let ids: Result<_, _> = Vec::from_lua(x, &lua)
                                    .map_err(QueryError::FailedToCoerce)
                                    .map(|x| PartialQueryResult { ids: x });
                                let mut rw = results2.write().unwrap();
                                rw[i as usize] = Some(ids);
                            }
                            Err(y) => {
                                let mut rw = results2.write().unwrap();
                                if let mlua::Error::CallbackError { ref cause, .. } = y
                                    && let mlua::Error::ExternalError(ext) = cause.deref()
                                    && let Some(LogProviderError::JoinShutdown) = ext.downcast_ref()
                                {
                                    // this is not a true error; therefore ignored.
                                    // see JoinShutdown docs.
                                    rw[i as usize] = Some(Ok(PartialQueryResult { ids: vec![] }));
                                } else {
                                    rw[i as usize] = Some(Err(QueryError::LuaError(y)));
                                }
                            }
                        }
                    });
                }
            });
            let elapsed = query_start.elapsed();

            // reconcile partial results
            let Ok(rr) = results.read() else {
                tx.send((Err(QueryError::QueryDied), elapsed)).ok();
                return;
            };
            let mut total_ids = vec![];
            for partial in rr.iter() {
                match partial {
                    Some(Ok(y)) => {
                        total_ids.extend(&y.ids);
                    }
                    Some(Err(x)) => {
                        tx.send((Err(x.clone()), elapsed)).ok();
                        return;
                    }
                    _ => unreachable!(),
                }
            }
            let ids_len = total_ids.len();
            let qr = QueryResult { ids: total_ids, pages: PaginatedResults::new(ids_len) };
            tx.send((Ok(qr), elapsed)).ok();
        });
    }
    pub fn new() -> Self {
        Self {
            text: SearchTextState::default(),
            settings: QuerySettings::new(),
            queries: vec![],
            last_id: 0,
            query_window_open: vec![],
            query_timing: vec![],
        }
    }
}
impl Default for SearchState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct LocatingStarted {
    pub target: u32,
    pub path_rx: Receiver<Vec<u32>>,
}
impl LocatingStarted {
    pub fn poll(&self) -> Option<Vec<u32>> {
        self.path_rx.try_recv().ok()
    }
}
pub enum LocatingState {
    None,
    Started(LocatingStarted),
    ScrollTo { target: u32, target_row_offset: Option<usize>, path: Vec<u32>, opened_path: bool },
    Highlight(u32),
}
impl LocatingState {
    pub fn can_start_new(&self) -> bool {
        match self {
            LocatingState::None => true,
            LocatingState::Started { .. } => false,
            LocatingState::ScrollTo { .. } => false,
            LocatingState::Highlight(_) => true,
        }
    }
}
impl LocatingState {
    pub fn start_locating(target: u32, trace_provider: &Arc<RwLock<LogProviderImpl>>) -> Self {
        let tc = trace_provider.clone();
        let (tx, path_rx) = crossbeam::channel::bounded(1);
        spawn_task(move || {
            let tc = tc.read().unwrap();
            let mut path = Vec::<u32>::with_capacity(4);
            let mut cur_idx = target;
            loop {
                path.push(cur_idx);
                if cur_idx == 0 {
                    break;
                }
                let Ok(parent) = tc.parent(cur_idx) else {
                    error!(id = cur_idx, "cannot resolve parent, breaking search at last known");
                    break;
                };
                cur_idx = parent;
            }
            path.reverse();
            info!("Path for {target}: {path:?}");
            tx.send(path).unwrap();
        });
        LocatingState::Started(LocatingStarted { target, path_rx })
    }
}
