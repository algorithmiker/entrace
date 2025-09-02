use egui::{Context, ScrollArea, Ui, vec2};
use entrace_core::display_error_context;
use tracing::{error, info};

use crate::{
    App, LogState, LogStatus, SpanContext, row_height,
    search::{Query, QueryError, QueryResult, QueryTiming, search_settings_dialog},
    span,
};

#[derive(Debug)]
pub enum LayoutCommand {
    Space(f32),
    Span(u32),
}
#[derive(Debug)]
pub struct QueryLayoutCache {
    /// this is a bool instead of an option, so we can re-use the same [commands](QueryLayoutCache::commands) allocation in multiple successive
    /// frames, since it is going to be similar in length
    pub is_valid: bool,
    pub commands: Vec<LayoutCommand>,
    pub last_scroll_offset: Option<f32>,
    pub last_inner_height: Option<f32>,
}

impl Default for QueryLayoutCache {
    fn default() -> Self {
        Self::new()
    }
}

impl QueryLayoutCache {
    pub fn new() -> Self {
        Self {
            is_valid: false,
            commands: Vec::with_capacity(32),
            last_scroll_offset: None,
            last_inner_height: None,
        }
    }
    /// Convenience method for [QueryLayoutCache.commands::push]
    pub fn push(&mut self, command: LayoutCommand) {
        self.commands.push(command);
    }
    pub fn invalidate(&mut self) {
        self.is_valid = false;
        self.commands.clear();
    }
}

pub fn query_windows(ui: &mut Ui, ctx: &Context, app: &mut App) {
    search_settings_dialog(ui, &mut app.search_state);
    for i in 0..app.search_state.queries.len() {
        let id = app.search_state.queries[i].id();
        let s = format!("Query {id}");
        egui::Window::new(s).open(&mut app.search_state.query_window_open[i]).show(ctx, |ui| {
            fn set_elapsed(i: usize, timing: &mut [QueryTiming]) {
                match timing[i] {
                    QueryTiming::Loading(instant) => {
                        timing[i] = QueryTiming::Finished(instant.elapsed())
                    }
                    QueryTiming::Finished(_) => unreachable!(),
                }
            }
            match app.search_state.queries[i] {
                Query::Loading { ref id, ref rx } => {
                    match rx.try_recv() {
                        Ok(q) => {
                            app.search_state.queries[i] = Query::Completed { id: *id, result: q };
                            set_elapsed(i, &mut app.search_state.query_timing);
                        }
                        Err(x) => match x {
                            crossbeam::channel::TryRecvError::Empty => (),
                            crossbeam::channel::TryRecvError::Disconnected => {
                                app.search_state.queries[i] = Query::Completed {
                                    id: *id,
                                    result: Err(QueryError::QueryDied),
                                };
                                set_elapsed(i, &mut app.search_state.query_timing);
                            }
                        },
                    }
                    ui.spinner();
                }
                Query::Completed { ref mut result, .. } => {
                    let elapsed = &app.search_state.query_timing[i];
                    ui.label(format!("Completed query in {:?}", elapsed.unwrap()));
                    ui.separator();
                    match result {
                        Ok(x) => match &mut app.log_status {
                            LogStatus::Ready(log_state) => query_result_list(ui, x, log_state),
                            _ => error!(
                                "query_windows: want to show query result but it is already \
                                 destroyed"
                            ),
                        },
                        Err(x) => {
                            ui.label("Query returned error:");
                            let formmatted = display_error_context(x);
                            ui.label(formmatted);
                        }
                    }
                }
            }
        });
    }
    let mut len = app.search_state.queries.len();
    let mut i = 0;
    while i < len {
        let id = app.search_state.queries[i].id();
        if !app.search_state.query_window_open[i] {
            info!("Will remove query with id {id}");
            app.search_state.queries.remove(i);
            app.search_state.query_window_open.remove(i);
            app.search_state.query_timing.remove(i);
            len -= 1;
        }
        i += 1;
    }
}

pub fn query_result_list(ui: &mut Ui, result: &mut QueryResult, log: &mut LogState) {
    ui.label(format!("Got {} spans.", result.ids.len()));
    let scroller = ScrollArea::new([false, true])
        .auto_shrink([false, false])
        .stick_to_bottom(false)
        .show(ui, |ui| match result.layout_cache.is_valid {
            true => result_list_cached(ui, result, log),
            false => result_list_no_cache(ui, result, log),
        });

    let cache = &mut result.layout_cache;
    let height = scroller.inner_rect.height();
    let new_offset = scroller.state.offset.y;
    if matches!(cache.last_inner_height, Some(last) if last != height)
        || matches!(cache.last_scroll_offset, Some(last) if last != new_offset)
    {
        cache.invalidate();
    }

    cache.last_inner_height = Some(height);
    cache.last_scroll_offset = Some(new_offset);
}
fn result_list_cached(ui: &mut Ui, result: &mut QueryResult, log: &mut LogState) {
    let log_reader = log.trace_provider.read().unwrap();
    let cache = &mut result.layout_cache;
    let mut ctx = SpanContext::QueryResults {
        locating_state: &log.locating_state,
        trace_provider: log.trace_provider.clone(),
    };
    let mut invalidate_cache = false;
    for command in &cache.commands {
        match command {
            LayoutCommand::Space(x) => {
                ui.allocate_space(vec2(10.0, *x));
            }
            LayoutCommand::Span(span_id) => {
                let resp = span(ui, &mut ctx, &log_reader, *span_id);
                if let Some(resp) = resp.header_response
                    && resp.clicked()
                {
                    invalidate_cache = true;
                    result.cull_open_state.toggle(*span_id as usize);
                }
            }
        }
    }
    if invalidate_cache {
        cache.invalidate();
    }
}
fn result_list_no_cache(ui: &mut Ui, result: &mut QueryResult, log: &mut LogState) {
    let log_reader = log.trace_provider.read().unwrap();
    let row_height = row_height(ui);
    let cache = &mut result.layout_cache;
    cache.invalidate();
    let mut invalidate_cache = false;
    let mut _non_culled_cnt = 0;
    //let skippable_space = ui.clip_rect().min.y - ui.cursor().min.y;
    //let skippable_rows = (skippable_space / row_height).max(0.0) as usize;
    //ui.allocate_space(vec2(ui.available_width(), row_height * skippable_rows as f32));
    //println!("Skipped rows: {skippable_rows}");
    enum Region {
        BeforeVisible(f32),
        Visible,
        AfterVisible(f32),
    }
    let mut region = Region::BeforeVisible(0.0);
    //let mut region = Region::Visible;
    let mut idx = 0;
    let mut ctx = SpanContext::QueryResults {
        locating_state: &log.locating_state,
        trace_provider: log.trace_provider.clone(),
    };
    while idx < result.ids.len() {
        let span_id = result.ids[idx];

        match region {
            Region::BeforeVisible(space_before) => {
                let span_end_lower_bound =
                    ui.cursor().min.y + space_before + row_height + ui.spacing().item_spacing.y;
                let still_before_visible = span_end_lower_bound < ui.clip_rect().min.y;
                //info!(span_end_lower_bound, clip_min = ui.clip_rect().min.y, "BeforeVisible");
                if !still_before_visible {
                    //println!("{idx} is (partially) visible. culled height before: {space_before}");
                    ui.allocate_space(vec2(10.0, space_before.max(0.0)));
                    cache.push(LayoutCommand::Space(space_before.max(0.0)));
                    region = Region::Visible;
                    continue;
                }

                if !result.cull_open_state.get(span_id as usize).unwrap_or(true) {
                    region = Region::BeforeVisible(space_before + row_height);
                } else {
                    // if this span is open, then its children should be visible eveen
                    // if the header is not. so draw it.
                    // To draw it in the correct space, we allocate the (closed) space
                    // before, then go again from 0.
                    ui.allocate_space(vec2(10.0, space_before.max(0.0)));
                    cache.push(LayoutCommand::Space(space_before.max(0.0)));
                    // println!(
                    //     "{idx} is open-culled, so allocated the current carry of {space_before} \
                    //      before"
                    // );
                    span(ui, &mut ctx, &log_reader, span_id);
                    cache.push(LayoutCommand::Span(span_id));
                    region = Region::BeforeVisible(0.0);
                }
            }
            Region::Visible => {
                //println!("Visible({idx})");
                let resp = span(ui, &mut ctx, &log_reader, span_id);
                cache.push(LayoutCommand::Span(span_id));
                _non_culled_cnt += 1;
                if let Some(resp) = resp.header_response {
                    if ui.clip_rect().max.y <= resp.rect.min.y {
                        region = Region::AfterVisible(0.0);
                        idx += 1;
                        continue;
                    }
                    if resp.clicked() {
                        invalidate_cache = true;
                        result.cull_open_state.toggle(idx);
                    }
                }
                if idx == result.ids.len().saturating_sub(1) {
                    ui.allocate_space(vec2(10.0, row_height * 3.0));
                    cache.push(LayoutCommand::Space(row_height * 3.0));
                }
            }
            Region::AfterVisible(after_space) => {
                if !result.cull_open_state.get(idx).unwrap_or(true) {
                    region = Region::AfterVisible(after_space + row_height);
                } else {
                    ui.allocate_space(vec2(10.0, after_space.max(0.0)));
                    cache.push(LayoutCommand::Space(after_space.max(0.0)));

                    //println!("{idx} is open-culled, so allocated the current carry of {after_space} before");
                    span(ui, &mut ctx, &log_reader, span_id);
                    cache.push(LayoutCommand::Span(span_id));
                    region = Region::AfterVisible(0.0);
                }
            }
        }
        idx += 1;
    }
    if let Region::AfterVisible(after_space) = region {
        ui.allocate_space(vec2(10.0, after_space));
        cache.push(LayoutCommand::Space(after_space.max(0.0)));
    }
    if invalidate_cache {
        cache.invalidate();
    }
    cache.is_valid = true;
    //println!("Not culled: {non_culled_cnt}");
}
