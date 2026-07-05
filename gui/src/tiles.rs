use egui::{Id, Margin};
use egui_tiles::SimplificationOptions;

use crate::{
    ApiDocsState, LogStatus,
    benchmarkers::SamplingBenchmark,
    homepage::paint_log,
    notifications::NotificationHandle,
    row_height_from_ctx,
    search::{self, SearchState, query_window::query_windows},
};

pub enum Pane {
    Tree {
        name: String,
        log: LogStatus,
        get_tree_bench: SamplingBenchmark<1>,
        search_state: SearchState,
    },
}
pub struct Behaviour<'a> {
    pub demo_mode: bool,
    pub notifier: NotificationHandle,
    pub api_docs_state: &'a mut ApiDocsState,
}

impl<'a> egui_tiles::Behavior<Pane> for Behaviour<'a> {
    fn pane_ui(
        &mut self, ui: &mut egui::Ui, tile_id: egui_tiles::TileId, pane: &mut Pane,
    ) -> egui_tiles::UiResponse {
        match pane {
            Pane::Tree { name: _, log, get_tree_bench, search_state } => {
                if let LogStatus::Ready(log_state) = &log {
                    let font_size = row_height_from_ctx(ui.ctx());
                    let text_field_margin = Margin::symmetric(4, 2);
                    let text_field_size =
                        font_size * 2.0 + text_field_margin.topf() + text_field_margin.bottomf();
                    egui::Panel::bottom(Id::from("bottom").with(tile_id))
                        .min_size(text_field_size)
                        .resizable(true)
                        .show(ui, |ui| {
                            search::bottom_panel_ui(
                                ui,
                                search_state,
                                self.api_docs_state,
                                log_state,
                                text_field_margin,
                                Id::new("bottom-panel-query").with(tile_id),
                            );
                        });
                }
                egui::CentralPanel::default().show(ui, |ui| {
                    query_windows(ui, log, search_state);
                    paint_log(ui, log, self.demo_mode, &self.notifier, get_tree_bench);
                });
            }
        }
        egui_tiles::UiResponse::None
    }

    fn tab_title_for_pane(&mut self, pane: &Pane) -> egui::WidgetText {
        match pane {
            Pane::Tree { name, .. } => name.into(),
        }
    }
    fn simplification_options(&self) -> egui_tiles::SimplificationOptions {
        SimplificationOptions {
            all_panes_must_have_tabs: true, // optimally this would be false but I am lazy for now
            ..Default::default()
        }
    }

    fn is_tab_closable(
        &self, _tiles: &egui_tiles::Tiles<Pane>, _tile_id: egui_tiles::TileId,
    ) -> bool {
        true
    }
}
