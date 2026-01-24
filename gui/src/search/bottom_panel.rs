use egui::{
    Color32, CornerRadius, Id, Key, Margin, Modifiers, Rect, Response, Sense, TextEdit, Ui,
    epaint::RectShape,
    pos2,
    text::{CCursor, CCursorRange},
    vec2,
};
use nucleo_matcher::{
    Matcher, Utf32Str,
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
};
use tracing::info;

use crate::{
    ApiDocsState, LogState, icon_colored,
    notifications::draw_x,
    rect,
    search::{QuerySettingsDialogData, SearchState, segmented_button::SegmentedIconButtons},
};
#[derive(Default)]
pub struct SearchTextState {
    pub text: String,
    pub matcher: Option<nucleo_matcher::Matcher>,
    pub autocomplete_results: Vec<(&'static str, u32)>,

    pub nucleo_buf: Vec<char>,
    pub force_focus: bool,
    pub selected_idx: Option<usize>,
    pub cursor_range: Option<CCursorRange>,
}
impl SearchTextState {
    pub fn recalculate_matches(&mut self, cursor_range: Option<CCursorRange>) {
        if let Some(range) = cursor_range {
            self.cursor_range = Some(range);
        }

        let cursor_index = self.cursor_range.map(|r| r.primary.index).unwrap_or(0);

        let byte_pos =
            self.text.char_indices().nth(cursor_index).map(|(i, _)| i).unwrap_or(self.text.len());
        let text_to_check = &self.text[..byte_pos];
        let last_word = get_current_word(text_to_check);

        let old_is_empty = self.autocomplete_results.is_empty();
        if last_word.is_empty() {
            self.autocomplete_results.clear();
            self.selected_idx = None;
            return;
        }
        if self.matcher.is_none() {
            self.matcher = Some(Matcher::default());
        }
        let matcher = self.matcher.as_mut().unwrap();
        let pattern =
            Pattern::new(last_word, CaseMatching::Ignore, Normalization::Smart, AtomKind::Fuzzy);
        self.nucleo_buf.clear();
        self.autocomplete_results.clear();
        let results = entrace_query::lua_api_docs::LUA_FN_NAMES.iter().filter_map(|item| {
            pattern
                .score(Utf32Str::new(item, &mut self.nucleo_buf), matcher)
                .map(|score| (*item, score))
        });
        self.autocomplete_results.extend(results);
        self.autocomplete_results.sort_by_key(|(_, score)| std::cmp::Reverse(*score));
        self.autocomplete_results.truncate(5);

        if old_is_empty != self.autocomplete_results.is_empty() {
            self.force_focus = true;
            self.selected_idx = None;
        }
    }
    pub fn cycle_or_start_selection(&mut self) {
        let len = self.autocomplete_results.len();
        self.selected_idx = Some(match self.selected_idx {
            Some(i) => (i + 1) % len,
            None => 0,
        });
    }
    pub fn accept_selection(&mut self, selected: usize) {
        let cursor_index = self.cursor_range.map(|r| r.primary.index).unwrap_or(0);
        let byte_cursor_pos =
            self.text.char_indices().nth(cursor_index).map(|(i, _)| i).unwrap_or(self.text.len());
        let text_to_check = &self.text[..byte_cursor_pos];
        let last_word_len = get_current_word(text_to_check).len();

        let result = self.autocomplete_results[selected].0;
        let start = byte_cursor_pos - last_word_len;
        self.text.replace_range(start..byte_cursor_pos, result);
        let new_cursor_pos = self.text[..start + result.len()].chars().count();
        self.cursor_range = Some(CCursorRange::one(CCursor::new(new_cursor_pos)));
        self.selected_idx = None;
        self.force_focus = true;
        self.recalculate_matches(None);
    }
}

pub fn bottom_panel_ui(
    ui: &mut Ui, search_state: &mut SearchState, api_docs_state: &mut ApiDocsState,
    log_state: &LogState, text_field_margin: Margin,
) {
    let text_edit_id = Id::new("bottom-search-text-edit");
    if ui.memory(|m| m.has_focus(text_edit_id))
        && !search_state.text.autocomplete_results.is_empty()
    {
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab)) {
            search_state.text.cycle_or_start_selection();
        }

        if let Some(idx) = search_state.text.selected_idx
            && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter))
        {
            search_state.text.accept_selection(idx);
        }
    }
    // by displaying the autocomplete area, we steal the focus from the text field, breaking
    // typing. so we need to steal it back.
    if search_state.text.force_focus {
        ui.memory_mut(|x| x.request_focus(text_edit_id));
        let mut state = egui::TextEdit::load_state(ui.ctx(), text_edit_id).unwrap_or_default();
        state.cursor.set_char_range(search_state.text.cursor_range);
        state.store(ui.ctx(), text_edit_id);
        search_state.text.force_focus = false;
    }

    let text_edit = TextEdit::multiline(&mut search_state.text.text)
        .desired_width(f32::INFINITY)
        .desired_rows(2)
        .frame(false)
        .id(text_edit_id)
        .margin(text_field_margin)
        .hint_text("Query")
        .code_editor();

    let search_response = ui.add_sized(ui.available_size(), text_edit);

    if !search_state.text.autocomplete_results.is_empty() && search_response.has_focus() {
        let popup_id = Id::new("bottom-search-autocomplete-popup");
        let mut pos = search_response.rect.left_top();
        pos.y -= 4.0;

        egui::Area::new(popup_id)
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .pivot(egui::Align2::LEFT_BOTTOM)
            .show(ui.ctx(), |ui| {
                ui.set_max_width(search_response.rect.width());
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (i, result) in search_state.text.autocomplete_results.iter().enumerate()
                        {
                            let mut btn = egui::Button::new(result.0)
                                .sense(Sense::focusable_noninteractive());
                            if search_state.text.selected_idx == Some(i) {
                                btn = btn.fill(ui.visuals().selection.bg_fill);
                            }
                            ui.add(btn);
                        }
                    });
                });
            });
    }

    if search_response.changed() || search_response.has_focus() {
        let cursor_range = if search_state.text.force_focus {
            search_state.text.cursor_range
        } else {
            egui::TextEdit::load_state(ui.ctx(), text_edit_id).and_then(|s| s.cursor.char_range())
        };
        search_state.text.recalculate_matches(cursor_range);
    }

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

fn get_current_word(s: &str) -> &str {
    let start = s
        .char_indices()
        .rev()
        .find(|&(_, c)| !(c.is_alphanumeric() || c == '_'))
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    &s[start..]
}
