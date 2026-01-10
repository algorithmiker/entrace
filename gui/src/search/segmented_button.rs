use crate::rect;
use egui::{Color32, Rect, Sense, Separator, Ui, Vec2, epaint::RectShape, pos2, vec2};

pub struct SegmentedIconButtons<const N: usize, F = ()> {
    shape: RectShape,
    /// controls the spacing between the segments inside the container. 3.0 by default.
    inner_spacing: f32,
    /// the area for the inner rect. by default rect![shape.rect.min + spacing2, shape.rect.max - spacing2]
    inner_rect: Rect,
    add_contents: F,
    separator_padding_y: [f32; 2],
}

impl<const N: usize> SegmentedIconButtons<N, ()> {
    pub fn new(shape: RectShape) -> Self {
        let rect = shape.rect;
        let spacing2 = Vec2::splat(3.0);
        let inner_rect = rect![rect.min + spacing2, rect.max - spacing2];
        Self {
            shape,
            inner_spacing: 3.0,
            add_contents: (),
            inner_rect,
            separator_padding_y: [0.0; 2],
        }
    }
}

impl<const N: usize, F> SegmentedIconButtons<N, F> {
    pub fn inner_spacing(mut self, spacing: f32) -> Self {
        self.inner_spacing = spacing;
        self
    }
    pub fn inner_rect(mut self, inner_rect: Rect) -> Self {
        self.inner_rect = inner_rect;
        self
    }

    pub fn with_contents<R, G>(self, add_contents: G) -> SegmentedIconButtons<N, G>
    where
        G: FnOnce(&mut Ui, [Rect; N]) -> R,
    {
        SegmentedIconButtons {
            shape: self.shape,
            inner_spacing: self.inner_spacing,
            add_contents,
            inner_rect: self.inner_rect,
            separator_padding_y: self.separator_padding_y,
        }
    }

    pub fn separator_y_padding(mut self, separator_y_padding: [f32; 2]) -> Self {
        self.separator_padding_y = separator_y_padding;
        self
    }
}

impl<const N: usize, R, F> SegmentedIconButtons<N, F>
where
    F: FnOnce(&mut Ui, [Rect; N]) -> R,
{
    pub fn show(self, ui: &mut Ui) {
        if N == 0 {
            return;
        }

        let rect = self.shape.rect;
        ui.painter().add(self.shape);
        let mut rects = [Rect::NOTHING; N];

        let total_width = self.inner_rect.width();
        let segment_width =
            (total_width - ((N - 1) as f32 * self.inner_spacing)) / (N as f32).max(1.0);
        let segment_size = vec2(segment_width, self.inner_rect.height());

        let (topy, bottomy, inner) = (
            rect.min.y + self.separator_padding_y[0],
            rect.max.y - self.separator_padding_y[1],
            &self.inner_rect,
        );
        let mut last_x = self.inner_rect.min.x;
        for (i, rect) in rects.iter_mut().enumerate() {
            *rect = if i == N - 1 {
                rect![pos2(last_x, inner.min.y), inner.max]
            } else {
                Rect::from_min_size(pos2(last_x, inner.min.y), segment_size)
            };

            if i > 0 {
                let sep_rect =
                    rect![pos2(last_x - self.inner_spacing, topy), pos2(last_x, bottomy)];
                ui.put(sep_rect, Separator::default().vertical());
            }
            last_x = rect.max.x + self.inner_spacing;
        }

        let _inner = (self.add_contents)(ui, rects);
    }
}

pub fn demo(ui: &mut Ui) {
    let (rect, _) = ui.allocate_at_least(vec2(150.0, 24.0), Sense::hover());
    let mut shape = RectShape::filled(rect, 4.0, ui.visuals().widgets.inactive.bg_fill);
    shape.stroke = ui.style().visuals.widgets.inactive.bg_stroke;

    SegmentedIconButtons::new(shape.clone())
        .inner_rect(shape.rect)
        .with_contents(|ui, rects: [Rect; 5]| {
            for (i, r) in rects.into_iter().enumerate() {
                let label = (i + 1).to_string();
                let resp = ui.put(r, egui::Button::new(label).frame(false));

                if resp.hovered() {
                    ui.painter().rect_filled(r, 0.0, Color32::GRAY.gamma_multiply_u8(24));
                }

                if resp.clicked() {
                    println!("Clicked segment {}", i + 1);
                }
            }
        })
        .show(ui);
}
