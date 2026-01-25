use std::{
    cell::RefMut,
    f32::consts::PI,
    ops::{Deref, Range},
};

use egui::{
    Color32, Rect, RichText, Sense, Shape, Stroke, Ui, UiBuilder, epaint::RectShape, pos2, vec2,
};
use entrace_core::{LogProvider, MetadataRefContainer, display_error_context};
use tracing::{debug, info, warn};

use crate::{
    LevelRepr, TraceReader, benchmarkers::SamplingBenchmark, enbitvec::EnBitVec, rect, row_height,
    search::LocatingState,
};
#[derive(Debug)]
pub enum Row {
    SpanHeader(u32),
    MetaHeader(u32),
    Text(String),
    Attr(String),
    Err(String),
}
pub struct TreeContext<'t, 'o, 'l> {
    pub log_reader: &'t TraceReader<'t>,
    pub open_reader: &'o EnBitVec,
    pub meta_open_reader: &'o EnBitVec,
    pub locating_state: Option<&'l mut LocatingState>,
}
pub struct TreeContextMut<'t, 'l, 'o> {
    pub log_reader: &'t TraceReader<'t>,
    pub open_writer: &'o mut EnBitVec,
    pub meta_open_writer: &'o mut EnBitVec,
    pub locating_state: Option<RefMut<'l, LocatingState>>,
}

#[derive(Debug)]
pub struct TreeView {
    pub cache_valid: bool,
    pub rows: Vec<Row>,
    pub row_depths: Vec<u32>,
    stack: Vec<(u32, u32)>,
}
impl Default for TreeView {
    fn default() -> Self {
        Self::new()
    }
}
impl TreeView {
    pub fn new() -> Self {
        Self { rows: vec![], row_depths: vec![], stack: vec![], cache_valid: false }
    }
    pub fn invalidate(&mut self) {
        self.cache_valid = false;
    }
    pub fn get_tree_non_cached<'t, 'o, 'l, Q: Iterator<Item = u32>>(
        &mut self, initial_spans: Q, ctx: TreeContext<'t, 'o, 'l>,
    ) {
        self.stack.clear();
        self.rows.clear();
        self.row_depths.clear();
        self.stack.extend(initial_spans.map(|x| (x, 0)));
        while let Some((this, depth)) = self.stack.pop() {
            if let Some(LocatingState::ScrollTo { target, target_row_offset, .. }) =
                ctx.locating_state
                && this == *target
            {
                *target_row_offset = Some(self.rows.len());
            }
            self.add_span(ctx.log_reader, &ctx.open_reader, &ctx.meta_open_reader, this, depth);
            let open = ctx.open_reader.get(this as usize).unwrap_or(false);
            if open {
                let children = match ctx.log_reader.children(this) {
                    Ok(x) => x,
                    Err(y) => {
                        warn!("Failed to get children of {this}: {y}");
                        continue;
                    }
                };
                let children_it = children
                    .iter()
                    .rev()
                    .copied()
                    .zip(std::iter::repeat_n(depth + 1, children.len()));
                self.stack.extend(children_it);
            }
        }
    }

    pub fn update_tree<'t, 'o, 'l, Q: Iterator<Item = u32>, const N: u8>(
        &mut self, benchmark: Option<&mut SamplingBenchmark<N>>, initial_spans: Q,
        ctx: TreeContext<'t, 'o, 'l>,
    ) {
        if self.cache_valid {
            return;
        }
        if let Some(benchmark) = benchmark {
            benchmark.start_pass();
            self.get_tree_non_cached(initial_spans, ctx);
            benchmark.end_pass();
        } else {
            self.get_tree_non_cached(initial_spans, ctx);
        }

        self.cache_valid = true;
    }
    pub fn add_row(&mut self, content: Row, depth: u32) {
        self.rows.push(content);
        self.row_depths.push(depth);
    }
    pub fn add_text(&mut self, text: String, depth: u32) {
        self.add_row(Row::Text(text), depth);
    }
    pub fn add_attr(&mut self, text: String, depth: u32) {
        self.add_row(Row::Attr(text), depth);
    }
    pub fn add_err(&mut self, text: String, depth: u32) {
        self.add_row(Row::Err(text), depth);
    }

    /// Helper for splitting text that may be multiline to multiple rows.
    /// This is generic over the [adder] so you can have [add_text], [add_err], and so on in one
    /// impl.
    pub fn add_multiline<T: Into<String>, F: FnMut(&mut Self, String, u32)>(
        &mut self, text: T, depth: u32, mut adder: F,
    ) {
        let text_s = text.into();
        let mut start = 0;
        // TODO: unnecessary allocation if there is just one line.
        for idx in memchr::memchr_iter(b'\n', text_s.as_bytes()) {
            adder(self, text_s[start..idx].to_string(), depth);
            start = idx + 1;
        }
        if start <= text_s.len() {
            adder(self, text_s[start..].to_string(), depth);
        }
    }

    pub fn add_span(
        &mut self, log_reader: &TraceReader, open_reader: &impl Deref<Target = EnBitVec>,
        meta_open_reader: &impl Deref<Target = EnBitVec>, id: u32, span_depth: u32,
    ) {
        self.add_row(Row::SpanHeader(id), span_depth);
        if open_reader.get(id as usize).unwrap_or(false) {
            match log_reader.attrs(id) {
                Ok(attrs) => {
                    for (name, val) in attrs {
                        let f = format!("{name}: {val}");
                        self.add_multiline(f, span_depth + 1, Self::add_attr);
                    }
                }
                Err(y) => self.add_multiline(y.to_string(), span_depth + 1, Self::add_err),
            }
            self.add_row(Row::MetaHeader(id), span_depth + 1);
            if meta_open_reader.get(id as usize).unwrap_or(false) {
                let m_depth = span_depth + 2;
                match log_reader.meta(id) {
                    Ok(MetadataRefContainer { name, target, level, module_path, file, line }) => {
                        self.add_multiline(format!("name: {name}"), m_depth, Self::add_text);
                        self.add_multiline(format!("target: {target}"), m_depth, Self::add_text);
                        self.add_multiline(
                            format!("module_path: {module_path:?}"),
                            m_depth,
                            Self::add_text,
                        );
                        self.add_multiline(format!("file: {file:?}"), m_depth, Self::add_text);
                        self.add_multiline(format!("line: {line:?}"), m_depth, Self::add_text);
                        self.add_multiline(format!("level: {level:?}"), m_depth, Self::add_text);
                    }
                    Err(y) => self.add_row(Row::Err(y.to_string()), m_depth),
                }
            }
        }
    }
}

pub fn tree_view<'t, 'o, 'l>(
    ui: &mut Ui, tree: &mut TreeView, row_range: Range<usize>, mut ctx: TreeContextMut<'t, 'o, 'l>,
) {
    if tree.rows.is_empty() {
        return;
    }
    if let Some(LocatingState::ScrollTo { target_row_offset, .. }) = ctx.locating_state.as_deref() {
        let row_height = row_height(ui);
        if let Some(target_row_offset) = target_row_offset {
            // these are only rough approximations, but we scroll to the rect once we see it
            // anyways.
            // we have to use *relative* offsets to scroll here, because of some magic that
            // ScrollArea::show_rows does sets the zero to the logical (viewport) zero.
            // XXX: we rely on the visible hook catching the scroll and scrolling to the proper X offset here.
            let row_diff = *target_row_offset as f64 - row_range.start as f64;
            let min = pos2(0.0, row_diff as f32 * row_height);
            ui.scroll_to_rect(rect!(min, min + vec2(0.0, row_height)), Some(egui::Align::Min));
            debug!(min = ?min, row_diff, target_row_offset, "Scrolling to");
        } else {
            warn!("Would scroll, but don't know target offset yet");
        }
    }
    let mut invalidate = false;
    for (row, depth) in tree.rows[row_range.clone()].iter().zip(tree.row_depths[row_range].iter()) {
        let Rect { min: original_min, max: original_max } = ui.available_rect_before_wrap();
        let left_pad = *depth as f32 * ui.spacing().indent;
        let padded_rect = rect!(original_min + vec2(left_pad, 0.0), pos2(f32::MAX, original_max.y));
        let scope_resp = ui
            .scope_builder(UiBuilder::new().max_rect(padded_rect), |ui| match row {
                Row::SpanHeader(id) => {
                    let header = match ctx.log_reader.header(*id) {
                        Ok(header) => header,
                        Err(y) => {
                            let f = display_error_context(&y);
                            ui.label(format!("Failed to get header for {id}: {f}"));
                            return;
                        }
                    };

                    let level_repr = header.level.repr(ui.ctx().theme());
                    let mut _elided_header = false;
                    let header_text_orig = if let Some(message) = header.message {
                        format!("{}: {}", level_repr.0, message)
                    } else if *id == 0 {
                        "root".to_string()
                    } else {
                        header.name.into()
                    };
                    let header_text =
                        if let Some(nlp) = memchr::memchr(b'\n', header_text_orig.as_bytes()) {
                            _elided_header = true;
                            format!("{}...", &header_text_orig[..nlp])
                        } else {
                            header_text_orig.clone()
                        };

                    let is_open = ctx.open_writer.get(*id as usize).unwrap_or(false);
                    let size = vec2(ui.spacing().icon_width, ui.spacing().icon_width);
                    ui.horizontal(|ui| {
                        let available_rect = ui.available_rect_before_wrap();
                        let (_icon_id, icon_rect) = ui.allocate_space(size);
                        let ui_header = egui::Label::new(
                            RichText::new(header_text).background_color(level_repr.1),
                        )
                        .sense(Sense::hover());
                        let label_resp = ui.add(ui_header);
                        //let label_resp = label_resp.on_hover_ui(|ui| {
                        //    ui.label(header_text_orig);
                        //});
                        let interact_id = ui.id().with(id);
                        let interact_rect =
                            label_resp.rect.with_min_x(0.0).with_max_x(available_rect.max.x);
                        let interact = ui.interact(interact_rect, interact_id, Sense::click());
                        if interact.clicked() {
                            ctx.open_writer.toggle(*id as usize);
                            invalidate = true;
                        }
                        if interact.clicked_by(egui::PointerButton::Secondary) {
                            info!(span_id = id, interact_rect=%interact_rect, "Right clicked");
                        }
                        let visuals = ui.style().interact(&interact);
                        // adapted from `egui::containers::collapsing_header::paint_default_icon`
                        let rect = Rect::from_center_size(icon_rect.center(), size * 0.5);
                        let rect = rect.expand(visuals.expansion);
                        let mut points =
                            vec![rect.left_top(), rect.right_top(), rect.center_bottom()];
                        if !is_open {
                            let rotation = egui::emath::Rot2::from_angle(PI * 1.5);
                            for p in &mut points {
                                *p = rect.center() + rotation * (*p - rect.center());
                            }
                        }
                        ui.painter().add(Shape::convex_polygon(
                            points,
                            visuals.fg_stroke.color,
                            Stroke::NONE,
                        ));
                        if let Some(LocatingState::Highlight(target)) =
                            ctx.locating_state.as_deref()
                            && target == id
                        {
                            let highlight_rect_min = icon_rect.min - ui.spacing().item_spacing;
                            let highlight_rect_max =
                                label_resp.rect.max + ui.spacing().item_spacing;
                            let highlight_rect = rect!(highlight_rect_min, highlight_rect_max);
                            let interact = ui.style().noninteractive();
                            ui.painter().add(RectShape::filled(
                                highlight_rect,
                                interact.corner_radius,
                                interact.bg_fill.gamma_multiply_u8(30),
                            ));
                            ui.painter().add(RectShape::stroke(
                                highlight_rect,
                                interact.corner_radius,
                                interact.bg_stroke,
                                egui::StrokeKind::Middle,
                            ));
                        }

                        // hover effect
                        if interact.hovered() {
                            let color = Color32::GRAY.gamma_multiply_u8(24);
                            ui.painter().add(Shape::rect_filled(interact_rect, 0, color));
                        }
                        if let Some(ref mut q) = ctx.locating_state
                            && let LocatingState::ScrollTo { target, .. } = &**q
                            && *target == *id
                        {
                            ui.scroll_to_rect(label_resp.rect, None);
                            info!(target, rect = %label_resp.rect, "Reached target");
                            **q = LocatingState::Highlight(*id);
                        };
                    });
                }
                Row::MetaHeader(id) => {
                    let is_open = ctx.meta_open_writer.get(*id as usize).unwrap_or(false);
                    ui.horizontal(|ui| {
                        let i_size = vec2(ui.spacing().icon_width, ui.spacing().icon_width);
                        let available_rect = ui.available_rect_before_wrap();
                        let (_icon_id, icon_rect) = ui.allocate_space(i_size);
                        let ui_header = egui::Label::new("META").sense(Sense::hover());

                        let label_resp = ui.add(ui_header);
                        let interact_id = ui.id().with("meta_toggle").with(id);
                        let interact_rect =
                            label_resp.rect.with_min_x(0.0).with_max_x(available_rect.max.x);
                        let interact = ui.interact(interact_rect, interact_id, Sense::click());
                        if interact.clicked() {
                            ctx.meta_open_writer.toggle(*id as usize);
                            invalidate = true;
                        }
                        let visuals = ui.style().interact(&interact);

                        // adapted from `egui::containers::collapsing_header::paint_default_icon`
                        let rect = Rect::from_center_size(icon_rect.center(), i_size * 0.5);
                        let rect = rect.expand(visuals.expansion);
                        let mut points =
                            vec![rect.left_top(), rect.right_top(), rect.center_bottom()];
                        if !is_open {
                            let rotation = egui::emath::Rot2::from_angle(PI * 1.5);
                            for p in &mut points {
                                *p = rect.center() + rotation * (*p - rect.center());
                            }
                        }
                        ui.painter().add(Shape::convex_polygon(
                            points,
                            visuals.fg_stroke.color,
                            Stroke::NONE,
                        ));
                        if interact.hovered() {
                            let color = Color32::GRAY.gamma_multiply_u8(24);
                            ui.painter().add(Shape::rect_filled(interact_rect, 0, color));
                        }
                    });
                }
                Row::Text(x) | Row::Attr(x) => {
                    ui.add(egui::Label::new(x).wrap_mode(egui::TextWrapMode::Extend));
                }
                Row::Err(x) => {
                    ui.label(x);
                }
            })
            .response;
        // indent line
        let indented_rect = scope_resp.rect;
        let spacing = ui.spacing().item_spacing.y;
        for depth in (1..*depth + 1).rev() {
            let left = indented_rect.left() - depth as f32 * ui.spacing().indent
                + ui.spacing().icon_width * 0.5;

            let color = Color32::GRAY.gamma_multiply_u8(127);
            let width = 1.0;
            let rect_min = pos2(left, indented_rect.min.y);
            let rect_max = pos2(left + width, indented_rect.max.y + spacing);
            ui.painter().rect_filled(rect!(rect_min, rect_max), 0, color);
        }
    }
    if invalidate {
        tree.invalidate();
    }
}
