use std::{
    cell::{LazyCell, RefCell},
    cmp::max,
    env,
    path::{Path, PathBuf},
    sync::{Arc, RwLock, atomic::Ordering},
};

use anyhow::Context;
use clap::Parser;
use egui::{
    Color32, Margin, Pos2, Rect, RichText, Stroke, Theme, Ui,
    epaint::text::{FontInsert, InsertFontFamily},
};
use entrace_core::{
    IETLoadConfig, IETPresentationConfig, LoadConfig,
    remote::{FileWatchConfig, NotifyExt},
};
use rfd::FileDialog;
use tracing::info;

use crate::{
    LogState, LogStatus,
    benchmarkers::BenchmarkManager,
    cmdline::Cmdline,
    connection_dialog::{ConnectionDialog, connect_dialog},
    convert_dialog::{self, ConvertDialogState},
    enbitvec::EnBitVec,
    ephemeral_settings::EphemeralSettings,
    frame_time::{FrameTimeTracker, TrackFrameTime, us_to_human},
    homepage::center,
    notifications::{self, NotificationHandle, RefreshToken},
    row_height_from_ctx,
    search::{self, LocatingState, SearchState, query_window::query_windows},
    self_tracing::SelfTracingState,
    settings::{self, SettingsDialogState, SettingsState, apply_settings},
    time_print,
    tree::TreeView,
};
pub struct App {
    pub file_picker_state: FilePickerState,
    pub connect_dialog: ConnectionDialog,
    pub log_status: LogStatus,
    pub search_state: SearchState,
    pub notifier: NotificationHandle,
    pub frame_time_tracker: FrameTimeTracker,
    pub self_tracing_state: SelfTracingState,
    pub settings: SettingsState,
    pub settings_dialog: SettingsDialogState,
    pub convert_dialog: ConvertDialogState,
    pub ephemeral_settings: EphemeralSettings,
    pub benchmarks: BenchmarkManager,
    pub about_state: AboutState,
}
impl Default for App {
    fn default() -> Self {
        Self {
            file_picker_state: FilePickerState::NoPick,
            log_status: LogStatus::NoFileOpened,
            search_state: SearchState::new(),
            connect_dialog: ConnectionDialog::not_open(),
            notifier: NotificationHandle::default(),
            self_tracing_state: SelfTracingState::default(),
            frame_time_tracker: FrameTimeTracker::Dummy,
            settings: SettingsState::None,
            settings_dialog: SettingsDialogState::default(),
            convert_dialog: ConvertDialogState::default(),
            ephemeral_settings: EphemeralSettings::default(),
            benchmarks: BenchmarkManager::default(),
            about_state: AboutState::new(),
        }
    }
}
impl App {
    // called before the first frame
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        #[cfg(feature = "dev")]
        {
            let ctx = cc.egui_ctx.clone();
            subsecond::register_handler(Arc::new(move || ctx.request_repaint()));
        }

        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.
        cc.egui_ctx.options_mut(|x| x.fallback_theme = Theme::Light);
        cc.egui_ctx.style_mut_of(Theme::Light, |style| {
            style.visuals.window_stroke = Stroke::new(0.5, Color32::BLACK);
            style.visuals.panel_fill = Color32::WHITE;
            style.visuals.widgets.inactive.bg_stroke = Stroke::new(0.4, Color32::BLACK);
            //println!("{:?}", style.visuals.widgets.noninteractive.bg_stroke);
            // style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(0.7, Color32::DARK_GRAY);
        });
        cc.egui_ctx.style_mut_of(Theme::Dark, |style| {
            style.visuals.window_stroke = Stroke::new(0.7, Color32::WHITE);
        });
        let mut app = App { ..Default::default() };
        let args = time_print("parsing args", Cmdline::parse);
        if let Some(x) = args.file_path {
            let path = PathBuf::from(x);
            app.open_file(path, cc.egui_ctx.clone());
        }
        // somewhat hacky override mechanism: if there are cli overrides, we pretend they are lines at
        // the end of the config file.
        // this simplifies parsing.
        let overrides = args.option_overrides.join("\n");
        let (tx, rx) = crossbeam::channel::bounded(1);
        let nc = cc.egui_ctx.clone();
        spawn_task(move || {
            time_print("loading settings", || {
                tx.send(SettingsState::init(RefreshToken(nc), overrides)).ok()
            });
        });
        app.settings = SettingsState::Loading(rx);

        egui_extras::install_image_loaders(&cc.egui_ctx);

        cc.egui_ctx.add_font(FontInsert::new(
            "JetBrainsMono",
            egui::FontData::from_static(include_bytes!("../vendor/JetBrainsMono-Regular.ttf")),
            vec![InsertFontFamily {
                family: egui::FontFamily::Monospace,
                priority: egui::epaint::text::FontPriority::Highest,
            }],
        ));
        cc.egui_ctx.add_font(FontInsert::new(
            "Adwaita Sans",
            egui::FontData::from_static(include_bytes!("../vendor/AdwaitaSans-Regular.ttf")),
            vec![InsertFontFamily {
                family: egui::FontFamily::Proportional,
                priority: egui::epaint::text::FontPriority::Highest,
            }],
        ));
        app
    }

    pub fn open_file(&mut self, path: impl AsRef<Path> + Send + 'static, ctx: egui::Context) {
        let path_clone = path.as_ref().to_path_buf();
        let (tx, rx) = crossbeam::channel::bounded(1);
        self.log_status = LogStatus::Loading(rx);
        info!("set log status to loading");
        spawn_task(move || {
            let (event_tx, event_rx) = crossbeam::channel::unbounded();
            let presentation =
                IETPresentationConfig { event_tx: Some(event_tx), refresher: RefreshToken(ctx) };
            let load_config = LoadConfig {
                iht: IETLoadConfig {
                    watch: FileWatchConfig::Watch(path.as_ref().to_path_buf()),
                    presentation,
                },
            };
            let trace = time_print("loading trace", || unsafe {
                entrace_core::load_trace(path, load_config)
            });
            match trace {
                Ok(x) => {
                    let cap = max(x.len(), 1);
                    let has_open_children = EnBitVec::repeat(false, cap);
                    tx.send(LogStatus::Ready(LogState {
                        file_path: path_clone,
                        trace_provider: Arc::new(RwLock::new(x)),
                        is_open: has_open_children,
                        meta_open: EnBitVec::repeat(false, cap),
                        locating_state: RefCell::new(LocatingState::None),
                        tree_view: TreeView::default(),
                        event_rx: Some(event_rx),
                    }))
                    .unwrap();
                }
                Err(x) => tx.send(LogStatus::Error(x.into())).unwrap(),
            }
        });
    }

    pub fn update_inner(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame_time_tracker.start_frame();
        match self.settings {
            SettingsState::None => (),
            SettingsState::Loading(ref rx) => {
                if let Ok(q) = rx.try_recv() {
                    match q.context("Failed to load settings") {
                        Ok(x) => {
                            self.settings = SettingsState::Loaded(x);
                            apply_settings(ctx, self);
                        }
                        Err(y) => {
                            let txt = format!("{y:?}");
                            println!("{txt}");
                            self.notifier.error(txt);
                        }
                    }
                }
            }
            SettingsState::Loaded(ref mut inner) => {
                if inner.need_refresh.load(Ordering::Relaxed) {
                    inner.need_refresh.store(false, Ordering::Relaxed);
                    if let Err(x) = inner.reload().context("Failed to reload settings") {
                        let f = format!("{x:?}");
                        println!("{f}");
                        self.notifier.error(f);
                    } else {
                        apply_settings(ctx, self);
                        info!("Reloaded settings");
                        self.notifier.info("Reloaded settings");
                    }
                }
            }
        }
        if let FilePickerState::Picking(ref x) = self.file_picker_state {
            // x.read() will borrow the value, so we can't really just set it here in an if.
            let path = if let Some(ref path) = *x.read().unwrap() {
                let path2 = path.clone();
                Some(path2)
            } else {
                None
            };
            if let Some(path1) = path {
                self.open_file(path1, ctx.clone());
                self.file_picker_state = FilePickerState::NoPick;
            }
        }

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open").clicked() {
                        let mut files = FileDialog::new();
                        if let Ok(x) = env::current_dir() {
                            files = files.set_directory(x)
                        }
                        if let Some(picked) = files.pick_file() {
                            self.open_file(picked, ui.ctx().clone());
                        }
                    };
                    if ui.button("Remote").clicked() {
                        self.connect_dialog = ConnectionDialog::new_connection();
                    };
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Tools", |ui| {
                    if ui.button("Convert").clicked() {
                        self.convert_dialog = ConvertDialogState::Open(Default::default());
                    }
                });
                if ui.button("Settings").clicked() {
                    match &self.settings {
                        SettingsState::None | SettingsState::Loading(..) => {
                            self.settings_dialog = SettingsDialogState::Loading;
                        }
                        SettingsState::Loaded(inner) => {
                            self.settings_dialog = SettingsDialogState::Some {
                                settings_clone: inner.settings.clone(),
                                settings_path: LazyCell::new(|| {
                                    settings::get_settings_path()
                                        .map(|x| x.to_string_lossy().into_owned())
                                        .unwrap_or("unknown".into())
                                }),
                            };
                        }
                    }
                };
                ui.menu_button("About", |ui| {
                    ui.label(format!("ENTRACE GUI {}", env!("CARGO_PKG_VERSION")));
                    if ui.button("Third-party licenses").clicked() {
                        self.about_state.open = true;
                    }
                });
                ui.add_space(16.0);
                if self.ephemeral_settings.fps_in_menu
                    && let Some(avg_time_us) = self.frame_time_tracker.get_average_us()
                {
                    ui.horizontal(|ui| {
                        ui.label(us_to_human(avg_time_us));
                        let fps = (1000000.0 / avg_time_us as f64) as u64;
                        ui.label(format!("{fps} FPS"));
                    });
                }
            });
        });
        if let LogStatus::Ready(log_state) = &self.log_status {
            let font_size = row_height_from_ctx(ctx);
            let text_field_margin = Margin::symmetric(4, 2);
            let text_field_size =
                font_size * 2.0 + text_field_margin.topf() + text_field_margin.bottomf();

            egui::TopBottomPanel::bottom("bottom_panel")
                .min_height(text_field_size)
                .resizable(true)
                .show(ctx, |ui| {
                    search::bottom_panel_ui(
                        ui,
                        &mut self.search_state,
                        log_state,
                        text_field_margin,
                    );
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            settings::settings_dialog(ctx, self);
            connect_dialog(ctx, self);
            convert_dialog::convert_dialog(ui, self);
            query_windows(ui, ctx, self);
            about_dialog(ctx, self);
            let available_rect = ui.available_rect_before_wrap();
            let notification_area = Rect::from_min_max(
                Pos2::new(available_rect.right() - 200.0, available_rect.top()),
                available_rect.right_bottom(),
            );
            center(ui, self);
            ui.put(notification_area, |ui: &mut Ui| {
                notifications::notifications(ui, self).response
            });
        });
        self.frame_time_tracker.end_frame();
    }
}
// simple right now, but might get replaced by a thread pool later.
pub fn spawn_task(f: impl FnOnce() + Send + 'static) {
    std::thread::spawn(f);
}
impl eframe::App for App {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {}
    fn save(&mut self, _storage: &mut dyn eframe::Storage) {}

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        #[cfg(feature = "dev")]
        {
            return subsecond::call(|| {
                self.update_inner(ctx, frame);
            });
        }
        self.update_inner(ctx, frame);
    }
}

#[derive(Default)]
pub enum FilePickerState {
    #[default]
    NoPick,
    Picking(Arc<RwLock<Option<PathBuf>>>),
}

pub struct AboutState {
    pub open: bool,
    pub text: LazyCell<Arc<RichText>>,
}
impl Default for AboutState {
    fn default() -> Self {
        Self::new()
    }
}

impl AboutState {
    pub fn new() -> Self {
        Self {
            text: LazyCell::new(|| {
                let t = RichText::new(include_str!("../../docs/thirdparty.txt")).monospace();
                Arc::new(t)
            }),
            open: false,
        }
    }
}
fn about_dialog(ctx: &egui::Context, app: &mut App) {
    egui::Window::new("About ENTRACE").open(&mut app.about_state.open).scroll([true; 2]).show(
        ctx,
        |ui| {
            ui.label((*app.about_state.text).clone());
        },
    );
}
