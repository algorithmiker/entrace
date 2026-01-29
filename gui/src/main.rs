use egui::{Color32, FontId, TextStyle, Theme, Ui};
use entrace_core::{LevelContainer, LogProviderImpl};
use mimalloc::MiMalloc;
use std::{
    sync::RwLockReadGuard,
    time::{Duration, Instant},
};
use tracing::info;

// improves performance of large queries by around 20%
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

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
pub mod homepage;
pub mod notifications;
pub mod search;
pub mod self_tracing;
pub mod settings;
pub mod tree;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    #[cfg(feature = "dev")]
    {
        dioxus_devtools::connect_subsecond();
    }
    #[cfg(feature = "dev")]
    {
        return subsecond::call(|| {
            eframe::run_native(
                "entrace",
                eframe::NativeOptions::default(),
                Box::new(|cc| Ok(Box::new(App::new(cc)))),
            )
        });
    }
    eframe::run_native(
        "entrace",
        eframe::NativeOptions::default(),
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

type TraceReader<'a> = RwLockReadGuard<'a, LogProviderImpl>;

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
#[macro_export]
macro_rules! rect {
    ($a:expr, $b:expr) => {
        egui::Rect::from_min_max($a, $b)
    };
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

pub fn row_height(ui: &mut Ui) -> f32 {
    ui.fonts_mut(|x| x.row_height(&TextStyle::Body.resolve(ui.style())))
    //ui.fonts(|x| x.row_height(&FontId::default()))
}

pub fn row_height_from_ctx(ctx: &egui::Context) -> f32 {
    // HACK: we can't use &TextStyle::Body.resolve(&ctx.style()) here, as it seems to run into a
    // deadlock.
    // Not sure what's the best way around this.
    ctx.fonts_mut(|x| x.row_height(&FontId::default()))
}

/// Return an icon image colored with the inactive text color.
/// To avoid having to have two icons for dark and light themes, this works by
/// taking a WHITE (#ffffff) icon, and tinting it when needed.
///
/// Use this with `ui.put`, eg. `ui.put(rect, icon!(ui, "settings.svg"))`.
#[macro_export]
macro_rules! icon {
    ($ui:expr, $src:literal) => {{
        let visuals = &$ui.style().visuals;
        let fg_stroke = visuals.widgets.inactive.fg_stroke.color;
        $crate::icon_colored!($src, fg_stroke)
    }};
}
/// Return an icon image colored with the specified colors.
/// To avoid having to have two icons for dark and light themes, this works by
/// taking a WHITE (#ffffff) icon, and tinting it when needed.
///
/// Use this with `ui.put`, eg. `ui.put(rect, icon!("settings.svg", Color32::WHITE))`.
#[macro_export]
macro_rules! icon_colored {
    ($src:literal, $color:expr) => {{
        let img = egui::Image::new(egui::include_image!($src));
        img.tint($color)
    }};
}
