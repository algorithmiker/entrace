use crate::{
    search::LocatingState,
    tree::{TreeContextMut, tree_view},
};
use mimalloc::MiMalloc;
use tracing::info;

// improves performance of large queries by around 20%
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

use std::{
    cell::RefCell,
    path::PathBuf,
    sync::{Arc, RwLock, RwLockReadGuard},
    time::{Duration, Instant},
};

use egui::{
    CollapsingHeader, Color32, Response, RichText, ScrollArea, TextStyle, Theme, Ui,
    epaint::RectShape, vec2,
};
use entrace_core::{LevelContainer, display_error_context};

mod app;
mod log;
pub use app::*;
pub use log::*;
pub mod benchmarkers;
pub mod cmdline;
pub mod connection_dialog;
pub mod convert_dialog;
pub mod enbitvec;
pub mod ephemeral_settings;
pub mod frame_time;
pub mod notifications;
pub mod search;
pub mod self_tracing;
pub mod settings;
pub mod tree;

#[macro_export]
macro_rules! rect {
    ($a:expr, $b:expr) => {
        egui::Rect::from_min_max($a, $b)
    };
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native("entrace", native_options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

#[derive(Default)]
pub enum FilePickerState {
    #[default]
    NoPick,
    Picking(Arc<RwLock<Option<PathBuf>>>),
}

pub fn time<T>(f: impl FnOnce() -> T) -> (Duration, T) {
    let start = Instant::now();
    let t = f();
    (start.elapsed(), t)
}
pub fn time_print<T>(tag: &str, f: impl FnOnce() -> T) -> T {
    let timed = time(f);
    println!("{tag} took {:?}", timed.0);
    timed.1
}
pub fn time_trace<T>(tag: &str, f: impl FnOnce() -> T) -> T {
    let timed = time(f);
    info!(task=tag,"type"="time_trace", took=?timed.0);
    timed.1
}

fn center(ui: &mut Ui, app: &mut App) {
    match app.log_status {
        LogStatus::Ready(ref mut state) => {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
                ui.label("file:");
                if app.ephemeral_settings.demo_mode {
                    ui.label("demo.et");
                } else {
                    ui.label(state.file_path.display().to_string());
                }
            });
            let len_before = { state.trace_provider.read().unwrap().len() };
            state.on_frame(&app.notifier);
            let delta = { state.trace_provider.read().unwrap().len().saturating_sub(len_before) };
            state.is_open.extend(std::iter::repeat_n(false, delta));
            state.meta_open.extend(std::iter::repeat_n(false, delta));
            if delta != 0 {
                state.tree_view.invalidate();
            }
            state.update_tree(&mut app.benchmarks.get_tree);
            let row_height = row_height(ui);
            let trace_reader = state.trace_provider.read().unwrap();
            let tree_ctx = TreeContextMut {
                log_reader: &trace_reader,
                open_writer: &mut state.is_open,
                meta_open_writer: &mut state.meta_open,
                locating_state: Some(state.locating_state.borrow_mut()),
            };
            ScrollArea::new([true; 2]).auto_shrink([false; 2]).show_rows(
                ui,
                row_height,
                state.tree_view.rows.len(),
                |ui, rows| {
                    tree_view(ui, &mut state.tree_view, rows, tree_ctx);
                },
            );
        }
        LogStatus::NoFileOpened => {
            ui.label("No trace loaded. Open a file, or set up a server with the File menu.");
        }
        LogStatus::Loading(ref rx) => {
            if let Ok(y) = rx.try_recv() {
                app.log_status = y;
            }
            ui.spinner();
        }
        LogStatus::Error(ref error) => {
            ui.label(format!("Error:\n{error:?}"));
        }
    }
}
pub trait LevelRepr {
    fn repr(&self, theme: egui::Theme) -> (&'static str, Color32);
    fn index(&self) -> u8;
}
impl LevelRepr for LevelContainer {
    fn repr(&self, theme: egui::Theme) -> (&'static str, Color32) {
        let symbol = match self {
            LevelContainer::Trace => "[T]",
            LevelContainer::Debug => "[D]",
            LevelContainer::Info => "[I]",
            LevelContainer::Warn => "[W]",
            LevelContainer::Error => "[E]",
        };
        // https://tailwindcolor.com/
        let color = match (self, theme) {
            (LevelContainer::Trace, Theme::Dark) => Color32::DARK_GRAY,
            (LevelContainer::Trace, Theme::Light) => Color32::LIGHT_GRAY,
            (LevelContainer::Debug, Theme::Dark) => Color32::DARK_GREEN,
            (LevelContainer::Debug, Theme::Light) => Color32::LIGHT_GREEN,
            (LevelContainer::Info, Theme::Dark) => Color32::from_rgb(0, 89, 138), // sky 800
            (LevelContainer::Info, Theme::Light) => Color32::from_rgb(184, 230, 254), // sky 200
            (LevelContainer::Warn, Theme::Dark) => Color32::from_rgb(137, 75, 0), // yellow 800
            (LevelContainer::Warn, Theme::Light) => Color32::from_rgb(255, 240, 133), // yellow 200
            (LevelContainer::Error, Theme::Dark) => Color32::DARK_RED,
            (LevelContainer::Error, Theme::Light) => Color32::LIGHT_RED,
        };
        (symbol, color)
    }

    fn index(&self) -> u8 {
        match self {
            LevelContainer::Trace => 1,
            LevelContainer::Debug => 2,
            LevelContainer::Info => 3,
            LevelContainer::Warn => 4,
            LevelContainer::Error => 5,
        }
    }
}
struct SpanResponse {
    header_response: Option<Response>,
    #[allow(dead_code)]
    body_response: Option<Response>,
}
impl SpanResponse {
    fn none() -> Self {
        Self { header_response: None, body_response: None }
    }
}
type TraceReader<'a> = RwLockReadGuard<'a, TraceProvider>;
enum SpanContext<'a> {
    QueryResults {
        locating_state: &'a RefCell<LocatingState>,
        trace_provider: Arc<RwLock<TraceProvider>>,
    },
}

fn span(
    ui: &mut Ui, ctx: &mut SpanContext<'_>, trace_reader: &TraceReader, id: u32,
) -> SpanResponse {
    let row_height = row_height(ui);

    let header = match trace_reader.header(id) {
        Ok(header) => header,
        Err(y) => {
            let ae = display_error_context(&y);
            ui.label(format!("Failed to get header for {id}: {ae}"));
            return SpanResponse::none();
        }
    };

    let level_repr = header.level.repr(ui.ctx().theme());
    let header_text: String;
    if let Some(message) = header.message {
        header_text = format!("{}: {}", level_repr.0, message);
    } else if id == 0 {
        header_text = "root".to_string();
    } else {
        header_text = header.name.into();
    };
    let ui_header =
        CollapsingHeader::new(RichText::new(header_text).background_color(level_repr.1))
            .id_salt(id);

    let body = |ui: &mut Ui, ctx: &mut SpanContext<'_>| {
        if id != 0 {
            CollapsingHeader::new("META").show(ui, |ui| {
                let meta = match trace_reader.meta(id) {
                    Ok(meta) => meta,
                    Err(y) => {
                        ui.label(display_error_context(&y));
                        return;
                    }
                };
                ui.label(format!("name: {}", meta.name));
                ui.label(format!("target: {}", meta.target));
                ui.label(format!("module_path: {:?}", meta.module_path));
                ui.label(format!("file: {:?}", meta.file));
                ui.label(format!("line: {:?}", meta.line));
                ui.label(format!("level: {:?}", meta.level));
            });
            //ui.label("ATTRS:");
            let span_data = match trace_reader.attrs(id) {
                Ok(span_data) => span_data,
                Err(y) => {
                    ui.label(display_error_context(&y));
                    return;
                }
            };
            for (x, y) in span_data {
                ui.label(format!("{x}: {y}",));
            }
        }
        let children = match trace_reader.children(id) {
            Ok(children) => children,
            Err(y) => {
                ui.label(display_error_context(&y));
                return;
            }
        };
        if children.is_empty() {
            return;
        }
        let clip_rect = ui.clip_rect();
        #[derive(Debug)]
        enum Region {
            Visible,
            AfterVisible,
        }
        use Region::*;
        let mut region = Visible;
        let mut after_rows = 0u64;
        for subspan in children.iter() {
            match region {
                Visible => {
                    let child_resp = span(ui, ctx, trace_reader, *subspan);
                    if let Some(resp) = child_resp.header_response
                        && resp.rect.min.y > clip_rect.max.y
                    {
                        region = AfterVisible;
                    }
                }
                AfterVisible => {
                    after_rows += 1;
                }
            }
        }
        ui.allocate_space(vec2(10.0, after_rows as f32 * row_height));
    };
    let header_res = ui_header.show(ui, |ui| body(ui, ctx));

    if header_res.header_response.clicked_by(egui::PointerButton::Secondary) {
        info!("Right-clicked {id}");
    }
    header_res.header_response.context_menu(|ui| {
        #[allow(irrefutable_let_patterns)]
        if let SpanContext::QueryResults { locating_state, trace_provider } = ctx {
            let enabled = locating_state.borrow().can_start_new();
            let btn = egui::Button::new("Locate in main tree");
            if ui.add_enabled(enabled, btn).clicked() {
                info!("Will locate {id}");
                *locating_state.borrow_mut() = LocatingState::start_locating(id, trace_provider);
            };
        }
        if ui.button("Close this menu").clicked() {
            ui.close();
        }
    });
    if header_res.header_response.hovered() {
        let rect = header_res.header_response.rect.expand2(vec2(ui.available_width(), 0.0));
        let shape = RectShape::filled(rect, 0, Color32::GRAY.gamma_multiply_u8(24));
        ui.painter().add(shape);
    }
    SpanResponse {
        header_response: Some(header_res.header_response),
        body_response: header_res.body_response,
    }
}

pub fn row_height(ui: &mut Ui) -> f32 {
    ui.fonts(|x| x.row_height(&TextStyle::Body.resolve(ui.style())))
    //ui.fonts(|x| x.row_height(&FontId::default()))
}
