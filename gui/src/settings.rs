use anyhow::Context as _;
use crossbeam::channel::Receiver;
use directories::ProjectDirs;
use egui::{
    Color32, Context, DragValue, InnerResponse, Margin, RichText, TextStyle, ThemePreference, Ui,
    epaint::AlphaFromCoverage, pos2, vec2,
};
use entrace_core::remote::{NotifyExt, Refresh};
use notify::{RecommendedWatcher, Watcher};
use std::{
    cell::LazyCell,
    cmp::min,
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tracing::{error, info};

use crate::{
    App,
    frame_time::{
        FrameTimeTracker, SamplingFrameTracker, TrackFrameTime, us_to_human, us_to_human_u64,
    },
    rect,
    self_tracing::{SelfTracingLevel, SelfTracingState},
    time_print,
};
pub enum SettingsMessage {
    ReloadSettings,
}
pub enum SettingsState {
    None,
    Loading(Receiver<Result<SettingsStateInner, LoadSettingsError>>),
    Loaded(SettingsStateInner),
}

impl SettingsState {
    pub fn ui_scale(&self) -> f32 {
        match self {
            SettingsState::None => 1.0,
            SettingsState::Loading(..) => 1.0,
            SettingsState::Loaded(settings_state_inner) => settings_state_inner.settings.ui_scale,
        }
    }
}

impl SettingsState {
    pub fn init(
        refresher: impl Refresh + Send + 'static, overrides: String,
    ) -> Result<SettingsStateInner, LoadSettingsError> {
        let path = get_settings_path()?;
        let settings = load_settings(&path, &overrides);
        use LoadSettingsError::*;
        match settings {
            Ok(settings) => {
                let (need_refresh, watcher) =
                    time_print("watching settings", || watch_settings(&path, refresher));
                Ok(SettingsStateInner { settings, need_refresh, watcher, overrides })
            }
            Err(CannotOpenSettings { .. } | CannotReadSettings { .. }) => {
                ensure_settings_exist(&path)?;
                let settings = load_settings(&path, &overrides)?;
                let (need_refresh, watcher) = watch_settings(&path, refresher);
                Ok(SettingsStateInner { settings, need_refresh, watcher, overrides })
            }
            Err(y) => Err(y),
        }
    }
}

pub struct SettingsStateInner {
    pub settings: Settings,
    pub need_refresh: Arc<AtomicBool>,
    pub watcher: RecommendedWatcher,
    pub overrides: String,
}
impl SettingsStateInner {
    pub fn reload(&mut self) -> Result<(), LoadSettingsError> {
        let path = get_settings_path()?;
        let settings = load_settings(&path, &self.overrides)?;
        self.settings = settings;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TextGamma {
    DarkSpecial,
    Gamma(f32),
}

impl TextGamma {
    pub fn write_ini(&self, buf: &mut impl std::fmt::Write) -> std::fmt::Result {
        match self {
            TextGamma::DarkSpecial => buf.write_str("\"dark-special\""),
            TextGamma::Gamma(y) => write!(buf, "{y}"),
        }
    }
    pub fn to_ini(&self) -> String {
        let mut buf = String::new();
        self.write_ini(&mut buf).unwrap();
        buf
    }
}
impl From<&TextGamma> for AlphaFromCoverage {
    fn from(value: &TextGamma) -> Self {
        match value {
            TextGamma::DarkSpecial => AlphaFromCoverage::TwoCoverageMinusCoverageSq,
            TextGamma::Gamma(1.0) => AlphaFromCoverage::Linear,
            TextGamma::Gamma(x) => AlphaFromCoverage::Gamma(*x),
        }
    }
}

// How to add a new setting:
// - define it here
// - add it to to_ini()
// - add it to parse_line()
// - add it to apply_settings()
#[derive(Debug, Clone)]
pub struct Settings {
    pub ui_scale: f32,
    pub self_tracing: SelfTracingLevel,
    pub save_self_trace: bool,
    pub theme: egui::ThemePreference,
    pub light_text_gamma: TextGamma,
    pub dark_text_gamma: TextGamma,
}

impl Settings {
    pub fn to_ini(&self) -> String {
        let Settings {
            ui_scale,
            self_tracing,
            theme,
            save_self_trace,
            light_text_gamma,
            dark_text_gamma,
        } = self;
        let theme = match theme {
            ThemePreference::Dark => "dark",
            ThemePreference::Light => "light",
            ThemePreference::System => "auto",
        };
        let self_tracing = self_tracing.repr_first_low();
        let (light_text_gamma, dark_text_gamma) =
            (light_text_gamma.to_ini(), dark_text_gamma.to_ini());
        format!(
            "ui_scale = {ui_scale:.1}
self_tracing = \"{self_tracing}\"
save_self_trace = {save_self_trace}
theme = \"{theme}\"
light_text_gamma = {light_text_gamma}
dark_text_gamma = {dark_text_gamma}"
        )
    }
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            ui_scale: 1.0,
            self_tracing: SelfTracingLevel::Disabled,
            theme: ThemePreference::System,
            save_self_trace: true,
            light_text_gamma: TextGamma::Gamma(1.0),
            dark_text_gamma: TextGamma::DarkSpecial,
        }
    }
}
// TODO: maybe borrow from the input instead of cloning here
#[derive(thiserror::Error, Debug)]
pub enum LoadSettingsError {
    #[error("Cannot get the path of my settings directory")]
    UnknownSettingsDir,
    #[error("Cannot create settings directory at {0}")]
    CannotCreateSettingsDir(String),
    #[error("Cannot open the settings file at {path}")]
    CannotOpenSettings {
        path: Box<Path>,
        #[source]
        inner: std::io::Error,
    },
    #[error("Cannot read the settings file at {path}")]
    CannotReadSettings {
        path: Box<Path>,
        #[source]
        inner: std::io::Error,
    },
    #[error("Cannot write to the settings file at {path}")]
    CannotWriteSettings {
        path: Box<Path>,
        #[source]
        inner: std::io::Error,
    },
    #[error("No key")]
    NoKey,
    #[error("No value")]
    NoValue,
    #[error("Bad value: `{value}`")]
    BadValue {
        value: String,
        #[source]
        inner: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("Bad theme value. Valid themes are: `dark`, `light`, `auto`")]
    BadTheme,
    #[error(
        "Bad value for self-tracing level. Valid values are: `disabled`, `trace`, `debug`, \
         `info`, `warn`, `error`"
    )]
    BadSelfTracingLevel,
    #[error("Unknown key `{0}`")]
    UnknownKey(String),
    #[error("Expected {0} but got {1}")]
    BadTag(String, String),
    #[error("Failed to parse line {0}")]
    BadLine(usize, #[source] Box<LoadSettingsError>),
    #[error("Failed to parse command line setting override {0}")]
    BadOverride(usize, #[source] Box<LoadSettingsError>),
}

/// Get the path of the settings file
pub fn get_settings_path() -> Result<PathBuf, LoadSettingsError> {
    let log_dir = ProjectDirs::from("org", "entrace", "entrace")
        .ok_or(LoadSettingsError::UnknownSettingsDir)?;
    let mut pb = log_dir.config_local_dir().to_path_buf();
    pb.push("config.ini");
    Ok(pb)
}

/// Recursively create the settings directory and write the default settings to the config file
pub fn ensure_settings_exist(path: impl AsRef<Path>) -> Result<(), LoadSettingsError> {
    let basedir = path.as_ref().parent().unwrap();
    std::fs::create_dir_all(basedir)
        .map_err(|_| LoadSettingsError::CannotCreateSettingsDir(basedir.display().to_string()))?;
    let Ok(mut file) = OpenOptions::new().create_new(true).write(true).open(&path) else {
        // file already exists, or if it cannot be opened, will error later.
        return Ok(());
    };
    let default_settings = Settings::default().to_ini();
    file.write_all(default_settings.as_bytes()).map_err(|x| {
        LoadSettingsError::CannotWriteSettings { path: path.as_ref().into(), inner: x }
    })?;
    Ok(())
}

pub fn load_settings(
    path: impl AsRef<Path>, overrides: &str,
) -> Result<Settings, LoadSettingsError> {
    use LoadSettingsError::*;
    let pb = || path.as_ref().into();
    let mut settings_file =
        File::open(&path).map_err(|inner| CannotOpenSettings { path: pb(), inner })?;

    let mut contents = String::new();
    settings_file
        .read_to_string(&mut contents)
        .map_err(|inner| CannotReadSettings { path: pb(), inner })?;
    let mut settings = parse_settings(&contents)?;
    for (idx, line) in overrides.lines().enumerate() {
        parse_line(line, &mut settings).map_err(|x| BadOverride(idx + 1, Box::new(x)))?;
    }
    Ok(settings)
}
pub fn write_settings(settings: &Settings) -> Result<(), LoadSettingsError> {
    let path = get_settings_path()?;
    write_settings_to(path, settings)
}
pub fn write_settings_to(
    path: impl AsRef<Path>, settings: &Settings,
) -> Result<(), LoadSettingsError> {
    let mut file = OpenOptions::new().write(true).truncate(true).open(&path).map_err(|inner| {
        LoadSettingsError::CannotOpenSettings { path: path.as_ref().into(), inner }
    })?;
    let default_settings = settings.to_ini();
    file.write_all(default_settings.as_bytes()).map_err(|x| {
        LoadSettingsError::CannotWriteSettings { path: path.as_ref().into(), inner: x }
    })?;
    Ok(())
}
pub fn watch_settings(
    path: impl AsRef<Path>, refresher: impl Refresh + Send + 'static,
) -> (Arc<AtomicBool>, RecommendedWatcher) {
    let need_refresh = Arc::new(AtomicBool::new(false));
    let nc = need_refresh.clone();
    use notify::{Error, Event, EventKind, event::ModifyKind};
    let mut watcher = notify::recommended_watcher(move |x: Result<Event, Error>| match x {
        Ok(x) => {
            //println!("Settings watcher event {x:?}");
            if let EventKind::Modify(ModifyKind::Data(_)) = x.kind {
                info!("settings file watcher fired");
                need_refresh.store(true, Ordering::Relaxed);
                refresher.refresh();
            }
        }
        Err(y) => tracing::error!("file watcher got error: {y}"),
    })
    .unwrap();
    watcher.watch(path.as_ref(), notify::RecursiveMode::NonRecursive).ok();
    (nc, watcher)
}

pub fn parse_settings(inp: &str) -> Result<Settings, LoadSettingsError> {
    let mut settings = Settings::default();
    use LoadSettingsError::*;
    for (idx, line) in inp.lines().enumerate() {
        parse_line(line, &mut settings).map_err(|x| BadLine(idx + 1, Box::new(x)))?;
    }
    Ok(settings)
}
pub fn parse_line(line: &str, settings: &mut Settings) -> Result<(), LoadSettingsError> {
    use LoadSettingsError::*;
    if line.is_empty() {
        return Ok(());
    }
    let mut splits = line.split("=");
    let key = splits.next().ok_or(NoKey)?.trim();
    match key {
        "ui_scale" => {
            let value = splits.next().ok_or(NoValue)?.trim();
            let value: f32 =
                value.parse().map_err(|x| BadValue { inner: Box::new(x), value: value.into() })?;
            settings.ui_scale = value;
        }
        "self_tracing" => {
            let value = splits.next().ok_or(NoValue)?.trim();
            let value = expect_tag("\"")(value)?;
            let (value, parsed) = parse_tracing_level(value)?;
            expect_tag("\"")(value)?;
            settings.self_tracing = parsed;
        }
        "save_self_trace" => {
            let value = splits.next().ok_or(NoValue)?.trim();
            let parsed = str::parse::<bool>(value)
                .map_err(|x| BadValue { value: value.into(), inner: Box::new(x) })?;
            settings.save_self_trace = parsed;
        }
        "theme" => {
            let value = splits.next().ok_or(NoValue)?.trim();
            let value = expect_tag("\"")(value)?;
            let (value, theme) = parse_theme(value)?;
            expect_tag("\"")(value)?;
            settings.theme = theme;
        }
        "light_text_gamma" => {
            let value = splits.next().ok_or(NoValue)?.trim();
            settings.light_text_gamma = parse_text_gamma(value)?;
        }
        "dark_text_gamma" => {
            let value = splits.next().ok_or(NoValue)?.trim();
            settings.dark_text_gamma = parse_text_gamma(value)?;
        }

        x => return Err(UnknownKey(x.into())),
    }
    Ok(())
}
pub fn expect_tag(tag: &str) -> impl FnOnce(&str) -> Result<&str, LoadSettingsError> {
    move |s: &str| {
        if let Some(rem) = s.strip_prefix(tag) {
            return Ok(rem);
        }
        let end = min(tag.len(), s.len());
        Err(LoadSettingsError::BadTag(tag.into(), s[0..end].into()))
    }
}
pub fn parse_theme(value: &str) -> Result<(&str, ThemePreference), LoadSettingsError> {
    use LoadSettingsError::*;
    if let Some(s) = value.strip_prefix("dark") {
        return Ok((s, ThemePreference::Dark));
    }
    if let Some(s) = value.strip_prefix("light") {
        return Ok((s, ThemePreference::Light));
    }
    if let Some(s) = value.strip_prefix("auto") {
        return Ok((s, ThemePreference::System));
    }
    Err(BadValue { value: value.into(), inner: Box::new(BadTheme) })
}
pub fn parse_text_gamma(value: &str) -> Result<TextGamma, LoadSettingsError> {
    // can be an f32 or "dark-special" (with quotes)
    if let Some(res) = value.strip_prefix("\"") {
        let res = expect_tag("dark-special")(res)?;
        let _res = expect_tag("\"")(res)?;
        Ok(TextGamma::DarkSpecial)
    } else {
        let gamma = str::parse::<f32>(value)
            .map_err(|x| LoadSettingsError::BadValue { value: value.into(), inner: Box::new(x) })?;
        Ok(TextGamma::Gamma(gamma))
    }
}
pub fn parse_tracing_level(value: &str) -> Result<(&str, SelfTracingLevel), LoadSettingsError> {
    use LoadSettingsError::*;
    if let Some(s) = value.strip_prefix("disabled") {
        return Ok((s, SelfTracingLevel::Disabled));
    }
    if let Some(s) = value.strip_prefix("trace") {
        return Ok((s, SelfTracingLevel::Trace));
    }
    if let Some(s) = value.strip_prefix("debug") {
        return Ok((s, SelfTracingLevel::Debug));
    }
    if let Some(s) = value.strip_prefix("info") {
        return Ok((s, SelfTracingLevel::Info));
    }
    if let Some(s) = value.strip_prefix("warn") {
        return Ok((s, SelfTracingLevel::Warn));
    }
    if let Some(s) = value.strip_prefix("error") {
        return Ok((s, SelfTracingLevel::Warn));
    }
    Err(BadValue { value: value.into(), inner: Box::new(BadSelfTracingLevel) })
}

pub fn apply_settings(ctx: &Context, app: &mut App) {
    if let SettingsState::Loaded(ref inner) = app.settings {
        ctx.set_pixels_per_point(inner.settings.ui_scale);
        ctx.set_theme(inner.settings.theme);
        ctx.style_mut_of(egui::Theme::Light, |x| {
            x.visuals.text_alpha_from_coverage = (&inner.settings.light_text_gamma).into()
        });
        ctx.style_mut_of(egui::Theme::Dark, |x| {
            x.visuals.text_alpha_from_coverage = (&inner.settings.dark_text_gamma).into()
        });
        match app.self_tracing_state {
            SelfTracingState::Disabled => {
                if !matches!(inner.settings.self_tracing, SelfTracingLevel::Disabled) {
                    app.self_tracing_state = SelfTracingState::start_tracing(
                        inner.settings.self_tracing.clone(),
                        inner.settings.save_self_trace,
                    );
                }
            }
            SelfTracingState::Enabled(ref tracing_inner) => {
                info!(
                    "Want to change tracing level from {} to {}",
                    tracing_inner.level, inner.settings.self_tracing
                );
                if inner.settings.self_tracing != tracing_inner.level {
                    app.notifier.info(
                        "The self-tracing level changed.\nChanging to a different level while \
                         running is not possible,\nso your change will only apply when you \
                         restart entrace.",
                    );
                }
            }
        }
        if let SettingsDialogState::Some { ref mut settings_clone, .. } = app.settings_dialog {
            *settings_clone = inner.settings.clone();
        }
    }
}
#[derive(Default)]
pub enum SettingsDialogState {
    #[default]
    None,
    Loading,
    Some {
        settings_clone: Settings,
        settings_path: LazyCell<String>,
    },
}
impl SettingsDialogState {
    pub fn settings_open(&self) -> bool {
        matches!(self, SettingsDialogState::Some { .. } | SettingsDialogState::Loading)
    }
}

pub fn settings_dialog(ctx: &Context, app: &mut App) {
    let mut settings_open = app.settings_dialog.settings_open();
    // if not done this way, then when we set SettingsDialogState::None upon closing,
    // there will be some jitter because the window switches from the actual dialog to an empty
    // dialog
    if settings_open {
        egui::Window::new("Settings").open(&mut settings_open).show(ctx, |ui| {
            match app.settings_dialog {
                SettingsDialogState::Loading => {
                    ui.spinner();
                }
                SettingsDialogState::None => (),
                SettingsDialogState::Some { .. } => settings_dialog_inner(ui, app),
            }
        });
        if !settings_open {
            app.settings_dialog = SettingsDialogState::None;
        };
    }
}

fn settings_dialog_inner(ui: &mut Ui, app: &mut App) {
    let SettingsDialogState::Some { ref mut settings_clone, ref settings_path } =
        app.settings_dialog
    else {
        unreachable!()
    };
    egui::warn_if_debug_build(ui);
    let padding = 4.0;
    let body_size = TextStyle::Body.resolve(ui.style()).size;
    ui.label(RichText::new("Saved settings").size(body_size * 1.2));
    ui.label(RichText::new(format!(
        "These settings are saved to the configuration file at\n`{}`",
        **settings_path
    )));
    ui.allocate_space(vec2(2.0, padding));
    let theme_resp = ui
        .horizontal(|ui| {
            ui.label("Theme: ");
            theme_preference_buttons(ui, &mut settings_clone.theme)
        })
        .inner
        .response;
    ui.horizontal(|ui| {
        ui.label("UI scale: ");
        ui.style_mut().spacing.slider_width = theme_resp.rect.width() - 52.0;
        ui.add(egui::Slider::new(&mut settings_clone.ui_scale, 1.0..=5.0));
    });
    ui.label("Self-Tracing: ");
    left_stroke_frame(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label("Save self-trace: ");
            ui.checkbox(&mut settings_clone.save_self_trace, ());
        });
        ui.horizontal(|ui| {
            ui.label("Level: ");
            egui::ComboBox::from_id_salt("self_tracing_level")
                .selected_text(format!("{:?}", settings_clone.self_tracing))
                .show_ui(ui, |ui| {
                    use SelfTracingLevel::*;
                    for value in [Disabled, Trace, Debug, Info, Warn, Error] {
                        let repr = value.repr_first_up();
                        ui.selectable_value(&mut settings_clone.self_tracing, value, repr);
                    }
                });
        });
        if let SelfTracingState::Enabled(ref x) = app.self_tracing_state {
            ui.horizontal(|ui| {
                ui.label("Currently saving trace?: ");
                ui.code(x.saving.to_string())
            });
            if let Some(ref y) = *x.path.read().unwrap() {
                ui.horizontal(|ui| {
                    ui.label("Path: ");
                    ui.code(y);
                });
                if ui.button("Open in new window").clicked() {
                    info!("Opening self-trace in new window");
                    if let Some(argv0) = std::env::args().next() {
                        if let Err(x) = Command::new(argv0)
                            .args([y, "--option", "save_self_trace=false"])
                            .spawn()
                        {
                            app.notifier.error(format!("Failed to spawn new instance: {x}"));
                        }
                    } else {
                        app.notifier.error("Cannot open self-trace, argv[0] is not set.");
                    }
                };
            }
        }
    });

    ui.label("Text rendering:");
    left_stroke_frame(ui, |ui| {
        pub fn text_gamma_ui(ui: &mut Ui, tg: &mut TextGamma) {
            let initial_dark_special = *tg == TextGamma::DarkSpecial;
            let mut dark_special = initial_dark_special;
            ui.checkbox(&mut dark_special, "Dark-mode special");
            if dark_special != initial_dark_special {
                *tg = if dark_special { TextGamma::DarkSpecial } else { TextGamma::Gamma(1.0) };
            }
            if let TextGamma::Gamma(gamma) = tg {
                ui.add(DragValue::new(gamma).speed(0.01).range(0.1..=4.0).prefix("Gamma: "));
            }
        }
        ui.horizontal(|ui| {
            ui.label("Light mode:");
            text_gamma_ui(ui, &mut settings_clone.light_text_gamma);
        });
        ui.horizontal(|ui| {
            ui.label("Dark mode:");
            text_gamma_ui(ui, &mut settings_clone.dark_text_gamma);
        });
    });
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
        if ui.button("Save").clicked()
            && let Err(x) = write_settings(settings_clone).context("Failed to write settings")
        {
            app.notifier.error(format!("{x:?}"));
        }
    });
    ui.separator();
    ui.label(RichText::new("Ephemeral settings").size(body_size * 1.2));
    ui.label(RichText::new(
        "These settings only affect the current instance of the program, and are not saved",
    ));
    ui.allocate_space(vec2(2.0, padding));
    let mut temp = app.frame_time_tracker.is_some();
    let checkbox_resp = ui.checkbox(&mut temp, "Track FPS");
    if checkbox_resp.changed() {
        if temp {
            info!("Enabled frame time tracking");
            app.frame_time_tracker = FrameTimeTracker::Culled(SamplingFrameTracker::new())
        } else {
            info!("Disabled frame time tracking");
            app.frame_time_tracker = FrameTimeTracker::Dummy
        }
    }
    if app.frame_time_tracker.is_some() {
        ui.checkbox(&mut app.ephemeral_settings.fps_in_menu, "FPS in menu");
    }
    if let Some(avg_time_us) = app.frame_time_tracker.get_average_us() {
        ui.horizontal(|ui| {
            ui.label("Average frame time:");
            ui.label(us_to_human(avg_time_us));
        });
        ui.horizontal(|ui| {
            ui.label("Calculated FPS: ");
            let fps = (1000000.0 / avg_time_us as f64) as u64;
            ui.label(format!("{fps} FPS"));
        });
    }
    ui.separator();
    ui.collapsing("Developer tools", |ui| {
        if ui.button("Send long notification").clicked() {
            let mut q = "Hello\n".repeat(5);
            q.pop();
            app.notifier.error(q);
        }
        ui.checkbox(&mut app.ephemeral_settings.demo_mode, "Demo mode");

        #[allow(clippy::single_element_loop)]
        for benchmark in [&mut app.benchmarks.get_tree] {
            ui.checkbox(&mut benchmark.enabled, format!("Benchmark {}", benchmark.name));
            if benchmark.enabled
                && let Some(avg_time) = benchmark.get_average_us()
            {
                ui.horizontal(|ui| {
                    ui.label(format!("Average {} time: ", benchmark.name));
                    ui.label(us_to_human_u64(avg_time));
                });
            }
        }
    });
}

/// Used as a general indicator for a group of settings.
pub fn left_stroke_frame<Q>(
    ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> Q,
) -> InnerResponse<Q> {
    let m = Margin { left: 8, right: 0, top: 0, bottom: 0 };
    let frame = egui::Frame::new().outer_margin(m).show(ui, add_contents);
    let interact = ui.style().interact(&frame.response);
    let stroke_rect_min = frame.response.rect.min;
    let stroke_rect_max =
        pos2(stroke_rect_min.x + interact.bg_stroke.width, frame.response.rect.max.y);

    ui.painter().rect_filled(
        rect!(stroke_rect_min, stroke_rect_max),
        0.0,
        interact.bg_stroke.color.lerp_to_gamma(Color32::BLACK, 0.25),
    );
    frame
}

/// theme_preference.show_radio_buttons, but we capture the response
pub fn theme_preference_buttons(
    ui: &mut Ui, theme_preference: &mut ThemePreference,
) -> InnerResponse<()> {
    ui.horizontal(|ui| {
        ui.selectable_value(theme_preference, ThemePreference::Light, "☀ Light");
        ui.selectable_value(theme_preference, ThemePreference::Dark, "🌙 Dark");
        ui.selectable_value(theme_preference, ThemePreference::System, "💻 System");
    })
}
