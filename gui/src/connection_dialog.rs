use std::{net::TcpListener, path::PathBuf};

use egui::Context;
use entrace::{
    IETPresentationConfig, LogProviderImpl,
    remote::{IETEvent, RemoteLogProvider},
};
use tracing::info;

use crate::{
    App, LogState, LogStatus, benchmarkers::SamplingBenchmark, notifications::RefreshToken,
    search::SearchState, tiles::Pane,
};
#[derive(Default)]
pub enum ConnectionDialogState {
    #[default]
    NotOpen,
    SetupConnection,
    SetupError(String),
}
#[derive(Default)]
pub struct ConnectionDialog {
    pub connect_url: String,
    pub state: ConnectionDialogState,
}
impl ConnectionDialog {
    pub fn new_connection() -> Self {
        Self { connect_url: "localhost:8000".into(), state: ConnectionDialogState::SetupConnection }
    }
    pub fn is_some(&self) -> bool {
        !matches!(self.state, ConnectionDialogState::NotOpen)
    }
    pub fn connect_server(
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

    pub fn connect_client(
        &mut self, context: &Context, event_tx: Option<crossbeam::channel::Sender<IETEvent>>,
    ) -> RemoteLogProvider {
        let ctx = context.clone();
        let iht_config = IETPresentationConfig { refresher: RefreshToken(ctx), event_tx };
        RemoteLogProvider::connect(&self.connect_url, iht_config)
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
            egui::Window::new("Server").open(&mut open).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("URL ");
                    egui::TextEdit::singleline(&mut app.connect_dialog.connect_url)
                        .desired_width(0.0)
                        .clip_text(false)
                        .show(ui);
                });
                ui.columns_const(|[left, right]| {
                    left.vertical(|ui| {
                        ui.label("Server mode");
                        ui.small(
                            "In this mode, the GUI acts as the server, and the traced program \
                             connects as a client",
                        );
                    });
                    right.vertical(|ui| {
                        ui.label("Client mode");
                        ui.small(
                            "In this mode, the GUI connects to the traced program, which acts as \
                             the server",
                        );
                    });
                });
                // this is a separate .columns container so that the start buttons are aligned
                // vertically, regardless of how the text above is wrapped
                ui.columns_const(|[left, right]| {
                    left.vertical(|ui| {
                        if ui.button("Start server").clicked() {
                            let (event_tx, event_rx) = crossbeam::channel::unbounded();
                            if let Some(provider) =
                                app.connect_dialog.connect_server(ui.ctx(), Some(event_tx))
                            {
                                let name = format!("Server({})", app.connect_dialog.connect_url);
                                let mut state = LogState::from_log_provider(
                                    PathBuf::from(&name),
                                    LogProviderImpl::Remote(provider),
                                );
                                state.event_rx = Some(event_rx);
                                app.add_tree_tab(Pane::Tree {
                                    name,
                                    log: LogStatus::Ready(state),
                                    get_tree_bench: SamplingBenchmark::new("remote", false),
                                    search_state: SearchState::with_autocomplete_enabled(true),
                                });
                                info!("Connect clicked");
                                should_close = true;
                            }
                        }
                    });
                    right.vertical(|ui| {
                        if ui.button("Start client").clicked() {
                            let (event_tx, event_rx) = crossbeam::channel::unbounded();
                            let provider =
                                app.connect_dialog.connect_client(ui.ctx(), Some(event_tx));
                            let name = format!("Client({})", app.connect_dialog.connect_url);
                            let mut state = LogState::from_log_provider(
                                PathBuf::from(&name),
                                LogProviderImpl::Remote(provider),
                            );
                            state.event_rx = Some(event_rx);
                            app.add_tree_tab(Pane::Tree {
                                name,
                                log: LogStatus::Ready(state),
                                get_tree_bench: SamplingBenchmark::new("remote", false),
                                search_state: SearchState::with_autocomplete_enabled(true),
                            });
                            info!("Connect clicked");
                            should_close = true;
                        }
                    })
                });
            });
        }
    }
    if !open || should_close {
        app.connect_dialog.state = ConnectionDialogState::NotOpen;
    }
}
