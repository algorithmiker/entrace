pub mod query_window;
pub mod segmented_button;
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

use crate::{
    ApiDocsState, LogState, TraceProvider, icon_colored,
    notifications::draw_x,
    rect,
    search::{query_window::PaginatedResults, segmented_button::SegmentedIconButtons},
    spawn_task,
};
use crossbeam::channel::Receiver;
use egui::{
    Color32, CornerRadius, Margin, Pos2, Rect, Response, Sense, TextEdit, Ui, epaint::RectShape,
    pos2, vec2,
};
use entrace_core::LogProviderError;
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
    pub text: String,
    pub queries: Vec<Query>,
    pub last_id: u16,
    pub query_window_open: Vec<bool>,
    pub query_timing: Vec<QueryTiming>,
}
impl SearchState {
    pub fn new_query(&mut self, trace_provider: Arc<RwLock<TraceProvider>>) {
        let (tx, rx) = crossbeam::channel::bounded(1);
        let new_id = self.last_id + 1;
        self.last_id += 1;
        self.queries.push(Query::Loading { id: new_id, rx });
        self.query_window_open.push(true);
        self.query_timing.push(QueryTiming::Loading(Instant::now()));
        let text_arc: Arc<str> = Arc::from(self.text.as_str());
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
            text: String::new(),
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

pub fn search_settings_dialog(ui: &mut Ui, search_state: &mut SearchState) {
    if let QuerySettingsDialogData::Open { settings_button_rect, ref mut position } =
        search_state.settings.data
    {
        let window = egui::Window::new("Query settings")
            .collapsible(false)
            .title_bar(false)
            .resizable(false)
            .pivot(egui::Align2::RIGHT_BOTTOM);
        if position.is_none() {
            *position = Some(settings_button_rect.right_top() - vec2(0.0, 10.0));
        }
        let pos = position.unwrap();
        window.current_pos(pos).show(ui.ctx(), |ui| {
            let (space_id, space_rect) = ui.allocate_space(vec2(18.0, 18.0));
            let space_center = space_rect.center();
            let interact = ui.interact(space_rect, space_id, Sense::click());
            let interact_style = ui.style().interact(&interact);
            draw_x(ui, space_center, 9.0, interact_style.text_color(), 1.0);
            if interact.clicked() {
                search_state.settings.data = QuerySettingsDialogData::Closed;
            }
            ui.horizontal(|ui| {
                ui.label("Number of threads: ");
                ui.add(
                    egui::DragValue::new(&mut search_state.settings.num_threads)
                        .speed(0.1)
                        .range(1..=255),
                );
            });
        });
        if let Some(rect) = ui.memory(|x| x.area_rect("Query settings"))
            && let QuerySettingsDialogData::Open { ref mut position, .. } =
                search_state.settings.data
        {
            *position = Some(rect.max);
        }
    }
}
pub fn bottom_panel_ui(
    ui: &mut Ui, search_state: &mut SearchState, api_docs_state: &mut ApiDocsState,
    log_state: &LogState, text_field_margin: Margin,
) {
    let text_edit = TextEdit::multiline(&mut search_state.text)
        .desired_width(f32::INFINITY)
        .desired_rows(2)
        .frame(false)
        .margin(text_field_margin)
        .hint_text("Query")
        .code_editor();
    let search_response = ui.add_sized(ui.available_size(), text_edit);
    if search_response.has_focus()
        && ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.ctrl)
    {
        search_state.new_query(log_state.trace_provider.clone());
    }

    let avail = ui.ctx().available_rect();
    let resize_width = ui.style().visuals.widgets.noninteractive.fg_stroke.width;
    let total_top_padding = resize_width + text_field_margin.topf();
    let search_rect = search_response.rect;
    let search_rect = search_rect.with_min_y(search_rect.min.y - total_top_padding);

    let icon_size = 20.0;
    let rect_top_left = pos2(avail.max.x - (3.0 * icon_size), search_rect.min.y);
    let rect_bottom_right = pos2(avail.max.x, search_rect.min.y + icon_size);
    let rect2 = rect![rect_top_left, rect_bottom_right];
    let bg_corner_radius = CornerRadius { nw: 0, ne: 0, sw: 2, se: 0 };
    let color = match ui.ctx().theme() {
        egui::Theme::Dark => Color32::DARK_GRAY,
        egui::Theme::Light => Color32::LIGHT_GRAY,
    };

    fn paint_label<L, O>(
        ui: &mut Ui, bg_rect: Rect, bg_corner_radius: CornerRadius, inner_rect: Rect,
        label_callback: impl FnOnce(&mut Ui, Color32) -> L, on_click: impl FnOnce(Response) -> O,
        hover_text: &str,
    ) {
        let mut resp = ui.allocate_rect(inner_rect, Sense::click());
        resp = resp.on_hover_text(hover_text);
        if resp.hovered() {
            ui.painter().rect_filled(bg_rect, bg_corner_radius, Color32::GRAY.gamma_multiply(0.5));
        }

        let interact_style = ui.style().interact(&resp);
        label_callback(ui, interact_style.text_color());
        if resp.clicked() {
            on_click(resp);
        }
    }
    let inner_to_bg_rect =
        |inner: Rect| rect![pos2(inner.min.x, rect2.min.y), pos2(inner.max.x, rect2.max.y)];
    SegmentedIconButtons::new(RectShape::filled(rect2, bg_corner_radius, color))
        .separator_y_padding([3.0, 1.0])
        .with_contents(|ui, rects: [Rect; 3]| {
            paint_label(
                ui,
                inner_to_bg_rect(rects[0]).with_min_x(rect2.min.x),
                bg_corner_radius,
                rects[0],
                |ui, clr| ui.put(rects[0], icon_colored!("../../vendor/icons/play_arrow.svg", clr)),
                |_| search_state.new_query(log_state.trace_provider.clone()),
                "Run (Ctrl+Enter)",
            );
            paint_label(
                ui,
                inner_to_bg_rect(rects[1]),
                CornerRadius::ZERO,
                rects[1],
                |ui, clr| ui.put(rects[1], icon_colored!("../../vendor/icons/docs.svg", clr)),
                |_| api_docs_state.open = true,
                "Lua API Docs",
            );
            paint_label(
                ui,
                inner_to_bg_rect(rect![rects[2].min, rect2.max]),
                CornerRadius::ZERO,
                rects[2],
                |ui, clr| ui.put(rects[2], icon_colored!("../../vendor/icons/settings.svg", clr)),
                |_| {
                    info!(settings_btn_rect = ?rects[2], "Query settings icon clicked");
                    search_state.settings.data = QuerySettingsDialogData::Open {
                        settings_button_rect: rects[2],
                        position: None,
                    }
                },
                "Settings",
            );
        })
        .show(ui);
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
    pub fn start_locating(target: u32, trace_provider: &Arc<RwLock<TraceProvider>>) -> Self {
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
