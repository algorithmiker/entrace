use std::{
    collections::VecDeque,
    sync::{Arc, LockResult, RwLock, RwLockReadGuard},
    time::{Duration, Instant},
};

use egui::{Color32, FontId, Pos2, Sense, Stroke, TextStyle, UiBuilder, pos2, vec2};
use entrace_core::{
    LevelContainer,
    remote::{Notify, Refresh},
};

use crate::{App, LevelRepr, rect};

#[derive(Default, Clone)]
pub struct NotificationHandle(pub Arc<RwLock<NotificationState>>);
impl Notify for NotificationHandle {
    fn add_notification(&self, severity: LevelContainer, text: String, duration: Duration) {
        let Ok(mut selfw) = self.0.write() else {
            return;
        };
        selfw.add_notification(severity, text, duration);
    }
    fn remove_notification(&self, idx: usize) {
        let Ok(mut selfw) = self.0.write() else {
            return;
        };
        selfw.remove_notification(idx);
    }
}
impl NotificationHandle {
    pub fn read(&'_ self) -> LockResult<RwLockReadGuard<'_, NotificationState>> {
        self.0.read()
    }
}
pub struct NotificationState {
    pub epoch: Instant,
    pub notis: VecDeque<Notification>,
}
#[derive(Debug)]
pub struct Notification {
    pub severity: LevelContainer,
    pub start: Instant,
    pub duration: Duration,
    pub text: String,
}
impl Notification {
    pub fn is_expired(&self, current_time: Instant) -> bool {
        (current_time - self.start) >= self.duration
    }
}
impl NotificationState {
    pub fn new() -> Self {
        let epoch = Instant::now();
        Self { epoch, notis: VecDeque::new() }
    }
    pub fn remove_notification(&mut self, idx: usize) {
        self.notis.remove(idx);
    }
    pub fn add_notification(&mut self, severity: LevelContainer, text: String, duration: Duration) {
        self.notis.push_back(Notification { severity, start: Instant::now(), duration, text });
    }
    pub fn recycle(&mut self) -> Option<Duration> {
        let now = Instant::now();
        // TODO: slow
        let mut idx = 0;
        while idx < self.notis.len() {
            if self.notis[idx].is_expired(now) {
                self.notis.remove(idx);
            } else {
                idx += 1;
            }
        }
        self.notis.front().map(|x| x.duration)
    }
}

impl Default for NotificationState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn notifications(ui: &mut egui::Ui, app: &mut App) -> egui::InnerResponse<()> {
    let mut handle = app.notifier.0.write().unwrap();
    let dur = handle.recycle();
    if let Some(dur) = dur {
        // this is a dumb check to prevent a dumb overflow
        if dur < Duration::from_secs(24 * 60 * 60) {
            ui.ctx().request_repaint_after(dur);
        }
    }
    drop(handle);
    let handle = app.notifier.0.read().unwrap();
    // TODO: this allocates each frame. we could skip it
    // TODO: ideally we would be doing our own layout here
    let mut to_remove = vec![];
    let r = ui.with_layout(egui::Layout::bottom_up(egui::Align::Max), |ui| {
        let font_id = TextStyle::resolve(&TextStyle::Body, ui.style());
        fn paint_notification(
            ui: &mut egui::Ui, notification: &Notification, idx: usize, font_id: FontId,
            to_remove: &mut Vec<usize>,
        ) {
            let repr = notification.severity.repr(ui.ctx().theme());
            let text_color = ui.visuals().noninteractive().fg_stroke.color;
            let item_spacing = ui.spacing().item_spacing;
            let item_spacing = vec2(item_spacing.x * 0.5, item_spacing.y);

            let severity_galley =
                ui.fonts_mut(|x| x.layout_no_wrap(repr.0.to_string(), font_id.clone(), text_color));
            let text_galley = ui
                .fonts_mut(|x| x.layout(notification.text.to_string(), font_id, text_color, 200.0));
            let severity_galley_size = severity_galley.size();

            let frame_width = (item_spacing.x + text_galley.size().x + item_spacing.x).max(100.0);
            let frame_height = item_spacing.y
                + severity_galley_size.y
                + item_spacing.y
                + text_galley.size().y
                + item_spacing.y;
            let frame_space = ui.allocate_space(vec2(frame_width, frame_height));
            let frame_rect = frame_space.1;
            ui.painter().rect_filled(frame_rect, 0, repr.1);

            let s_min = frame_rect.min + item_spacing;
            let s_max = s_min + severity_galley_size;
            let severity_rect = rect!(s_min, s_max);
            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(severity_rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                |ui| ui.add(egui::Label::new(repr.0)),
            );

            let text_rect_min =
                frame_rect.min + vec2(item_spacing.x, severity_galley_size.y + item_spacing.y);
            let text_rect_max = text_rect_min + text_galley.size();
            let text_rect = rect!(text_rect_min, text_rect_max);
            ui.put(text_rect, egui::Label::new(text_galley));

            let x_size = severity_galley_size.y;
            let x_min =
                pos2(frame_rect.max.x - x_size - item_spacing.x, frame_rect.min.y + item_spacing.y);
            let x_max = x_min + vec2(x_size, x_size);
            let x_bg_rect = rect!(x_min, x_max);
            let x_bg_resp = ui.allocate_rect(x_bg_rect, Sense::CLICK | Sense::HOVER);
            let thickness = if x_bg_resp.hovered() { 2.5 } else { 2.0 };
            let size = if x_bg_resp.hovered() { x_size / 1.5 } else { x_size / 2.0 };
            draw_x(ui, x_bg_rect.center(), size, ui.visuals().text_color(), thickness);
            if x_bg_resp.clicked() {
                to_remove.push(idx);
            }
            let frame_interact = ui.interact(frame_rect, frame_space.0, Sense::HOVER);
            if frame_interact.hovered() {
                ui.painter().rect_filled(frame_rect, 0, Color32::GRAY.gamma_multiply_u8(24));
            }
        }
        for (idx, notification) in handle.notis.iter().enumerate() {
            paint_notification(ui, notification, idx, font_id.clone(), &mut to_remove);
            ui.allocate_space(ui.spacing().item_spacing);
        }
    });
    drop(handle);
    let mut handle = app.notifier.0.write().unwrap();
    for x in to_remove {
        handle.remove_notification(x);
    }
    r
}
pub fn draw_x(ui: &mut egui::Ui, position: Pos2, size: f32, color: Color32, thickness: f32) {
    let half_size = size / 2.0;

    let top_left = Pos2::new(position.x - half_size, position.y - half_size);
    let bottom_right = Pos2::new(position.x + half_size, position.y + half_size);
    ui.painter().line_segment([top_left, bottom_right], Stroke::new(thickness, color));

    let top_right = Pos2::new(position.x + half_size, position.y - half_size);
    let bottom_left = Pos2::new(position.x - half_size, position.y + half_size);
    ui.painter().line_segment([top_right, bottom_left], Stroke::new(thickness, color));
}
pub struct RefreshToken(pub egui::Context);
impl Refresh for RefreshToken {
    fn refresh(&self) {
        self.0.request_repaint_after(Duration::from_millis(100));
    }
}
