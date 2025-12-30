use crate::{
    App, LogState, LogStatus,
    homepage::{SpanContext, span},
    search::{Query, QueryError, QueryResult, QueryTiming, search_settings_dialog},
};
use egui::{Context, Layout, ScrollArea, Ui, Vec2, Widget};
use entrace_core::display_error_context;
use std::{cmp::min, fmt::Write, ops::Range};
use tracing::{error, info};

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
#[derive(Debug)]
pub struct PaginatedResults {
    cur_page: usize,
    page_size: usize,
    nr_entries: usize,
    page_entry_text: String,
}
impl PaginatedResults {
    pub fn new(nr_entries: usize) -> PaginatedResults {
        PaginatedResults {
            cur_page: 0,
            page_size: 100,
            page_entry_text: String::from("0"),
            nr_entries,
        }
    }
    pub fn cur_range(&self) -> Range<usize> {
        let start = self.cur_page * self.page_size;
        start..min(start + self.page_size, self.nr_entries)
    }
    pub fn set_page(&mut self, new: usize) {
        self.cur_page = min(new, self.page_cnt().saturating_sub(1));
        self.page_entry_text.clear();
        write!(self.page_entry_text, "{}", self.cur_page).ok();
    }
    pub fn page_cnt(&self) -> usize {
        self.nr_entries.div_ceil(self.page_size)
    }
}
pub fn result_list_pagination(ui: &mut Ui, result: &mut QueryResult) {
    ui.allocate_ui_with_layout(Vec2::ZERO, Layout::left_to_right(egui::Align::Center), |ui| {
        if ui.button("<").clicked() {
            result.pages.set_page(result.pages.cur_page.saturating_sub(1));
        }
        ui.label("Page ");
        let ed = egui::TextEdit::singleline(&mut result.pages.page_entry_text)
            .desired_width(0.0)
            .clip_text(false)
            .ui(ui);
        if ed.lost_focus()
            && let Ok(x) = str::parse::<usize>(&result.pages.page_entry_text)
        {
            result.pages.set_page(x);
        }
        ui.label(format!("/ {}", result.pages.page_cnt()));
        if ui.button(">").clicked() {
            result.pages.set_page(result.pages.cur_page.saturating_add(1));
        }
    });
}
pub fn query_result_list(ui: &mut Ui, result: &mut QueryResult, log: &mut LogState) {
    ui.label(format!("Got {} spans.", result.ids.len()));
    result_list_pagination(ui, result);
    ScrollArea::new([false, true]).auto_shrink([false, false]).stick_to_bottom(false).show(
        ui,
        |ui| {
            let result_range = result.pages.cur_range();
            let log_reader = log.trace_provider.read().unwrap();
            let mut ctx = SpanContext::QueryResults {
                locating_state: &log.locating_state,
                trace_provider: log.trace_provider.clone(),
            };
            for id in result_range {
                span(ui, &mut ctx, &log_reader, result.ids[id]);
            }
        },
    );
}
