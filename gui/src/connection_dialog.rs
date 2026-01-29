use std::{
    cell::RefCell,
    net::TcpListener,
    path::PathBuf,
    sync::{Arc, RwLock},
};

use egui::Context;
use entrace_core::{
    IETPresentationConfig, LogProviderImpl,
    remote::{IETEvent, RemoteLogProvider},
};
use tracing::info;

use crate::{
    App, LogState, LogStatus, enbitvec::EnBitVec, notifications::RefreshToken,
    search::LocatingState, tree::TreeView,
};

pub enum ConnectionDialogState {
    NotOpen,
    SetupConnection,
    SetupError(String),
}
pub struct ConnectionDialog {
    pub connect_url: String,
    pub state: ConnectionDialogState,
}
impl ConnectionDialog {
    pub fn not_open() -> Self {
        Self { connect_url: String::new(), state: ConnectionDialogState::NotOpen }
    }
    pub fn new_connection() -> Self {
        Self { connect_url: "localhost:8000".into(), state: ConnectionDialogState::SetupConnection }
    }
    pub fn is_some(&self) -> bool {
        !matches!(self.state, ConnectionDialogState::NotOpen)
    }
    pub fn connect(
        &mut self, context: &Context, event_tx: Option<crossbeam::channel::Sender<IETEvent>>,
    ) -> Option<RemoteLogProvider> {
        let tcp_listener = match TcpListener::bind(&self.connect_url) {
            Ok(tcp_listener) => tcp_listener,
            Err(x) => {
                self.state = ConnectionDialogState::SetupError(x.to_string());
                return None;
            }
        };
        let ctx = context.clone();
        let iht_config = IETPresentationConfig { refresher: RefreshToken(ctx), event_tx };
        let provider = RemoteLogProvider::new(tcp_listener, iht_config);
        Some(provider)
    }
}

pub fn connect_dialog(ctx: &Context, app: &mut App) {
    let mut open = app.connect_dialog.is_some();
    let mut should_close = false;
    match &app.connect_dialog.state {
        ConnectionDialogState::NotOpen => (),
        ConnectionDialogState::SetupError(x) => {
            egui::Window::new("Server").open(&mut open).show(ctx, |ui| {
                ui.label(format!("Error while setting up connection: {x}"));
            });
        }

        ConnectionDialogState::SetupConnection => {
            let dialog = &mut app.connect_dialog;
            egui::Window::new("Server").open(&mut open).show(ctx, |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
                    ui.label("Server URL: ");
                    egui::TextEdit::singleline(&mut dialog.connect_url)
                        .desired_width(0.0)
                        .clip_text(false)
                        .show(ui);
                    if ui.button("Start").clicked() {
                        let (event_tx, event_rx) = crossbeam::channel::unbounded();
                        if let Some(provider) = dialog.connect(ui.ctx(), Some(event_tx)) {
                            let is_open = EnBitVec::repeat(false, 1);
                            let meta_open = EnBitVec::repeat(false, 1);
                            app.log_status = LogStatus::Ready(LogState {
                                file_path: PathBuf::from(&dialog.connect_url),
                                trace_provider: Arc::new(RwLock::new(LogProviderImpl::Remote(
                                    provider,
                                ))),
                                is_open,
                                meta_open,
                                locating_state: RefCell::new(LocatingState::None),
                                tree_view: TreeView::new(),
                                event_rx: Some(event_rx),
                            });
                        }
                        info!("Connect clicked");
                        should_close = true;
                    }
                })
            });
        }
    }
    if !open || should_close {
        app.connect_dialog.state = ConnectionDialogState::NotOpen;
    }
}
