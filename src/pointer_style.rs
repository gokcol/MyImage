//! Input-transparent Calico cursor equivalents, with unchanged tool hotspots.
//! This UI-only layer never enters a document or export.
use eframe::egui::{self, Color32, CursorIcon, Pos2, Rect, Vec2};
use std::borrow::Cow;
#[path = "pointer_art.rs"]
mod pointer_art;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointerStyle {
    #[default]
    Standard,
    Calico,
}
impl PointerStyle {
    pub const ALL: [Self; 2] = [Self::Standard, Self::Calico];
    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::Calico => "Calico",
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Art {
    Arrow,
    Grab,
    Grabbing,
    Cursor(CursorIcon),
    Badge,
}
impl Art {
    fn for_cursor(style: PointerStyle, cursor: CursorIcon) -> Option<Self> {
        if style != PointerStyle::Calico {
            return None;
        }
        match cursor {
            CursorIcon::Default => Some(Self::Arrow),
            CursorIcon::Grab => Some(Self::Grab),
            CursorIcon::Grabbing => Some(Self::Grabbing),
            CursorIcon::None => None,
            cursor => Some(Self::Cursor(cursor)),
        }
    }
    fn hotspot(self) -> Vec2 {
        match self {
            Self::Arrow => Vec2::splat(2.),
            Self::Grab | Self::Grabbing => Vec2::splat(14.),
            Self::Cursor(cursor) => pointer_art::hotspot(cursor),
            Self::Badge => Vec2::ZERO,
        }
    }
    fn source(self) -> Cow<'static, str> {
        match self {
            Self::Arrow => include_str!("../assets/pointers/calico.svg").into(),
            Self::Grab => include_str!("../assets/pointers/calico-grab.svg").into(),
            Self::Grabbing => include_str!("../assets/pointers/calico-grabbing.svg").into(),
            Self::Cursor(cursor) => pointer_art::svg(cursor).into(),
            Self::Badge => pointer_art::badge().into(),
        }
    }
    fn texture(self, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        let name = format!("calico-pointer-{self:?}");
        let id = egui::Id::new(&name);
        if let Some(texture) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
            return Some(texture);
        }
        let image = crate::svg_stamp::raster(&self.source(), 112).ok()?;
        let texture = ctx.load_texture(
            name,
            egui::ColorImage::from_rgba_unmultiplied(
                [image.width() as usize, image.height() as usize],
                image.as_raw(),
            ),
            egui::TextureOptions::LINEAR,
        );
        ctx.data_mut(|d| d.insert_temp(id, texture.clone()));
        Some(texture)
    }
}

/// Keep a custom tool's existing geometry (e.g. the Fill bucket and its seed)
/// and request a small Calico accent beside it for this frame only.
pub fn custom_tool(ctx: &egui::Context, badge_at: Pos2) {
    let frame = ctx.cumulative_frame_nr();
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("pointer-tool-accent"), (frame, badge_at)));
}

/// Call after the viewport's controls choose their cursor.
pub fn paint(ctx: &egui::Context, style: PointerStyle) {
    if style != PointerStyle::Calico {
        return;
    }
    let Some(at) = ctx.input(|i| i.focused.then(|| i.pointer.hover_pos()).flatten()) else {
        return;
    };
    if !ctx.content_rect().contains(at) {
        return;
    }
    // Keep a small, constant screen size even with larger UI text or image zoom.
    let scale = 1. / ctx.zoom_factor();
    let cursor = ctx.output(|o| o.cursor_icon);
    let (art, rect) = if let Some(art) = Art::for_cursor(style, cursor) {
        (
            art,
            Rect::from_min_size(at - art.hotspot() * scale, Vec2::splat(28. * scale)),
        )
    } else if let Some((frame, badge_at)) =
        ctx.data(|d| d.get_temp::<(u64, Pos2)>(egui::Id::new("pointer-tool-accent")))
        && frame == ctx.cumulative_frame_nr()
    {
        (
            Art::Badge,
            Rect::from_min_size(badge_at, Vec2::splat(12. * scale)),
        )
    } else {
        return;
    };
    let Some(texture) = art.texture(ctx) else {
        return;
    };
    ctx.layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::new("pointer-artwork"),
    ))
    .image(
        texture.id(),
        rect,
        Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.)),
        Color32::WHITE,
    );
    ctx.set_cursor_icon(CursorIcon::None);
}

pub fn preview(painter: &egui::Painter, rect: Rect, style: PointerStyle) {
    if style == PointerStyle::Calico {
        if let Some(texture) = Art::Arrow.texture(painter.ctx()) {
            painter.image(
                texture.id(),
                rect,
                Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.)),
                Color32::WHITE,
            );
        }
    } else {
        let at = |x, y| rect.min + egui::vec2(x, y) * (rect.width() / 28.);
        let points = vec![
            at(5., 3.),
            at(5., 23.),
            at(10., 18.),
            at(14., 26.),
            at(18., 24.),
            at(14., 16.),
            at(22., 16.),
        ];
        // The arrow is concave, so outline its path instead of a convex fill.
        painter.add(egui::Shape::closed_line(
            points,
            egui::Stroke::new(1.5, crate::appearance::palette(painter.ctx()).ink),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn calico_has_a_renderable_equivalent_for_every_visible_cursor() {
        for cursor in CursorIcon::ALL {
            assert!(Art::for_cursor(PointerStyle::Standard, cursor).is_none());
            assert_eq!(
                Art::for_cursor(PointerStyle::Calico, cursor).is_some(),
                cursor != CursorIcon::None
            );
            if let Some(art) = Art::for_cursor(PointerStyle::Calico, cursor) {
                let pixels = crate::svg_stamp::raster(&art.source(), 112).unwrap();
                assert!(pixels.pixels().any(|p| p[3] > 0), "{cursor:?}");
                assert!(
                    Rect::from_min_size(Pos2::ZERO, Vec2::splat(28.))
                        .contains(art.hotspot().to_pos2())
                );
            }
        }
    }
    #[test]
    fn pointer_leaves_on_focus_loss_and_renders_precision_cursors() {
        let ctx = egui::Context::default();
        for (focused, position, cursor, hidden) in [
            (true, Some(egui::pos2(80., 80.)), CursorIcon::Default, true),
            (true, Some(egui::pos2(80., 80.)), CursorIcon::Grab, true),
            (true, Some(egui::pos2(80., 80.)), CursorIcon::Grabbing, true),
            (true, Some(egui::pos2(80., 80.)), CursorIcon::Text, true),
            (
                true,
                Some(egui::pos2(80., 80.)),
                CursorIcon::Crosshair,
                true,
            ),
            (
                true,
                Some(egui::pos2(80., 80.)),
                CursorIcon::ResizeNeSw,
                true,
            ),
            (true, Some(egui::pos2(80., 80.)), CursorIcon::None, false),
            (
                false,
                Some(egui::pos2(80., 80.)),
                CursorIcon::Default,
                false,
            ),
            (true, None, CursorIcon::Default, false),
        ] {
            let output = ctx.run(
                egui::RawInput {
                    focused,
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(200.))),
                    events: vec![
                        position.map_or(egui::Event::PointerGone, egui::Event::PointerMoved),
                    ],
                    ..Default::default()
                },
                |ctx| {
                    ctx.set_cursor_icon(cursor);
                    paint(ctx, PointerStyle::Calico);
                },
            );
            assert_eq!(
                output.platform_output.cursor_icon,
                if hidden { CursorIcon::None } else { cursor }
            );
            assert_eq!(output.shapes.is_empty(), !hidden);
        }
    }

    #[test]
    fn fill_accent_is_frame_scoped_and_standard_remains_native() {
        let ctx = egui::Context::default();
        for (style, marker, expected) in [
            (PointerStyle::Standard, true, false),
            (PointerStyle::Calico, true, true),
            (PointerStyle::Calico, false, false),
        ] {
            let output = ctx.run(
                egui::RawInput {
                    focused: true,
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(200.))),
                    events: vec![egui::Event::PointerMoved(egui::pos2(80., 80.))],
                    ..Default::default()
                },
                |ctx| {
                    ctx.set_cursor_icon(CursorIcon::None);
                    if marker {
                        custom_tool(ctx, egui::pos2(100., 70.));
                    }
                    paint(ctx, style);
                },
            );
            assert_eq!(output.shapes.is_empty(), !expected);
            assert_eq!(output.platform_output.cursor_icon, CursorIcon::None);
        }
    }
}
