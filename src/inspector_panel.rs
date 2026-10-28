//! Ribbon panel selector, floating inspector, and resizable pinned panel.
use super::{
    EditorState,
    inspector_widgets::{self, Glyph},
};
use eframe::egui::{self, CursorIcon, Id, Key, Pos2, Rect, Sense, Stroke, Vec2};
use serde::{Deserialize, Serialize};

pub(super) const SELECTOR_WIDTH: f32 = 104.;
const MIN_WIDTH: f32 = 260.;
const MAX_WIDTH: f32 = 560.;
const PANEL_MARGIN: i8 = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Panel {
    #[default]
    Styles,
    Layers,
    Presets,
    Effects,
}

impl Panel {
    const ALL: [Self; 4] = [Self::Styles, Self::Layers, Self::Presets, Self::Effects];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Styles => "Styles",
            Self::Layers => "Layers",
            Self::Presets => "Presets",
            Self::Effects => "Quick Effects",
        }
    }

    fn glyph(self) -> Glyph {
        use crate::appearance::Icon;
        match self {
            Self::Styles => Glyph::App(Icon::Settings),
            Self::Layers => Glyph::Stack,
            Self::Presets => Glyph::App(Icon::Favorite),
            Self::Effects => Glyph::App(Icon::Effects),
        }
    }
}

/// Workspace preferences are separate from document edits and tool appearances.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub panel: Panel,
    pub pinned: bool,
    pub expanded: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct State {
    pub preferences: Preferences,
    changed: bool,
}

impl State {
    pub fn is(&self, panel: Panel) -> bool {
        self.preferences.panel == panel
    }

    pub fn select(&mut self, panel: Panel) {
        self.changed |= self.preferences.panel != panel;
        self.preferences.panel = panel;
    }

    pub fn show(&mut self, panel: Panel) {
        self.select(panel);
        self.changed |= !self.preferences.expanded;
        self.preferences.expanded = true;
    }

    pub fn close(&mut self) {
        self.changed |= self.preferences.expanded;
        self.preferences.expanded = false;
    }

    pub fn toggle_pin(&mut self) {
        self.preferences.pinned = !self.preferences.pinned;
        self.changed = true;
    }

    #[cfg(test)]
    pub(super) fn pinned(panel: Panel) -> Self {
        Self {
            preferences: Preferences {
                panel,
                expanded: true,
                pinned: true,
            },
            changed: false,
        }
    }
}

impl EditorState {
    pub fn set_inspector_preferences(&mut self, preferences: Preferences) {
        self.inspector = State {
            preferences,
            changed: false,
        };
        self.inspector.select(Panel::Styles);
    }

    pub fn take_inspector_preferences_change(&mut self) -> Option<Preferences> {
        std::mem::take(&mut self.inspector.changed).then_some(self.inspector.preferences)
    }
}

/// The primary action always opens Styles; the chevron exposes all sections.
/// `true` means the primary action can also collapse an already-open Styles panel.
pub(super) fn selector(ui: &mut egui::Ui, state: State) -> Option<(Panel, bool)> {
    let mut action = None;
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 2.;
        let palette = crate::appearance::palette(ui.ctx());
        let menu = ui.add_sized([24., 30.], egui::Button::new("").corner_radius(6));
        menu.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), "Choose panel")
        });
        let center = menu.rect.center();
        ui.painter().line(
            vec![
                center + Vec2::new(-4., -2.),
                center + Vec2::new(0., 2.),
                center + Vec2::new(4., -2.),
            ],
            Stroke::new(1.7, palette.ink),
        );
        let menu = menu.on_hover_text("Styles, Layers, Presets and Quick Effects");
        egui::Popup::menu(&menu).show(|ui| {
            ui.set_min_width(168.);
            for panel in Panel::ALL {
                let selected = state.preferences.expanded && state.is(panel);
                let response = ui.add_sized([168., 30.], egui::Button::new("").selected(selected));
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::WidgetType::SelectableLabel,
                        ui.is_enabled(),
                        selected,
                        panel.label(),
                    )
                });
                inspector_widgets::paint_glyph(
                    ui.painter(),
                    response.rect.left_center() + Vec2::new(15., 0.),
                    panel.glyph(),
                    palette.ink,
                    18.,
                );
                ui.painter().text(
                    response.rect.left_center() + Vec2::new(32., 0.),
                    egui::Align2::LEFT_CENTER,
                    panel.label(),
                    egui::FontId::proportional(13.),
                    palette.ink,
                );
                if response.clicked() {
                    action = Some((panel, false));
                    ui.close();
                }
            }
        });
        let selected = state.preferences.expanded && state.is(Panel::Styles);
        let styles = ui.add_sized(
            [78., 30.],
            egui::Button::new("").selected(selected).corner_radius(6),
        );
        styles.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::Button,
                ui.is_enabled(),
                selected,
                "Styles",
            )
        });
        crate::appearance::icon(
            ui.painter(),
            styles.rect.left_center() + Vec2::new(15., 0.),
            crate::appearance::Icon::Settings,
            palette.ink,
            16.,
        );
        ui.painter().text(
            styles.rect.left_center() + Vec2::new(29., 0.),
            egui::Align2::LEFT_CENTER,
            "Styles",
            egui::FontId::proportional(13.),
            palette.ink,
        );
        if styles
            .on_hover_text(if selected {
                "Close Styles"
            } else {
                "Open Styles"
            })
            .clicked()
        {
            action = Some((Panel::Styles, true));
        }
    });
    action
}

pub(super) enum HeaderAction {
    Close,
    TogglePin,
}

pub(super) fn header(ui: &mut egui::Ui, preferences: Preferences) -> Option<HeaderAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        ui.strong(preferences.panel.label());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::appearance::icon_button(
                ui,
                "Close properties",
                crate::appearance::Icon::Close,
            )
            .clicked()
            {
                action = Some(HeaderAction::Close);
            }
            let pin = ui.add_sized(
                [24., 24.],
                egui::Button::new("").selected(preferences.pinned),
            );
            let label = if preferences.pinned {
                "Unpin panel"
            } else {
                "Pin panel"
            };
            pin.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::Button,
                    ui.is_enabled(),
                    preferences.pinned,
                    label,
                )
            });
            crate::appearance::icon(
                ui.painter(),
                pin.rect.center(),
                if preferences.pinned {
                    crate::appearance::Icon::Unpin
                } else {
                    crate::appearance::Icon::Pin
                },
                crate::appearance::palette(ui.ctx()).ink,
                16.,
            );
            if pin
                .on_hover_text(if preferences.pinned {
                    "Unpin panel · Float over the canvas"
                } else {
                    "Pin panel · Dock beside the canvas"
                })
                .clicked()
            {
                action = Some(HeaderAction::TogglePin);
            }
        });
    });
    ui.separator();
    action
}

pub(super) fn overlay_rect(ui: &egui::Ui) -> Rect {
    let workspace = ui.available_rect_before_wrap();
    let width = ui
        .ctx()
        .data(|d| d.get_temp::<WidthPreference>(Id::new("annotation-inspector-width")))
        .unwrap_or_default()
        .0
        .unwrap_or(MIN_WIDTH);
    let width = width.clamp(MIN_WIDTH, MAX_WIDTH).min(workspace.width());
    Rect::from_min_max(
        Pos2::new(workspace.right() - width, workspace.top()),
        workspace.max,
    )
}

pub(super) fn overlay<R>(ui: &egui::Ui, rect: Rect, contents: impl FnOnce(&mut egui::Ui) -> R) {
    let palette = crate::appearance::palette(ui.ctx());
    let enabled = ui.is_enabled();
    let opacity = ui.opacity();
    egui::Area::new(Id::new("inspector-overlay"))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .default_size(rect.size())
        .movable(false)
        .sense(Sense::hover())
        .constrain_to(rect)
        .show(ui.ctx(), |ui| {
            if !enabled {
                ui.disable();
            }
            ui.set_opacity(opacity);
            ui.set_clip_rect(rect);
            egui::Frame::NONE
                .fill(palette.paper)
                .inner_margin(PANEL_MARGIN)
                .show(ui, |ui| {
                    ui.set_min_size(
                        (rect.size() - Vec2::splat(2. * f32::from(PANEL_MARGIN))).max(Vec2::ZERO),
                    );
                    ui.set_max_size(
                        (rect.size() - Vec2::splat(2. * f32::from(PANEL_MARGIN))).max(Vec2::ZERO),
                    );
                    contents(ui);
                });
            ui.painter().line_segment(
                [rect.left_top(), rect.left_bottom()],
                Stroke::new(1., palette.border),
            );
        });
}

#[derive(Clone, Copy, Default)]
struct WidthPreference(Option<f32>);

pub(super) fn dock<R>(
    ui: &mut egui::Ui,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let id = Id::new("annotation-inspector-width");
    let divider_id = id.with("divider");
    let workspace = ui.available_rect_before_wrap();
    // Leave a useful canvas even in a small window or at a large UI scale.
    let max = (workspace.width() - 240.).clamp(MIN_WIDTH, MAX_WIDTH);
    let min = MIN_WIDTH;
    let fit = min;
    let mut preference = ui
        .ctx()
        .data(|d| d.get_temp::<WidthPreference>(id))
        .unwrap_or_default();

    // Consume this frame's interaction before laying out either side, so
    // the canvas and inspector move together without a frame of overlap.
    if let Some(response) = ui
        .ctx()
        .read_response(divider_id)
        .filter(|r| r.enabled() && ui.is_enabled())
    {
        // A native drag can be delivered entirely between two repaints.
        // egui then sees a release without an ongoing `dragged` response.
        let completed_drag = if egui::Popup::is_any_open(ui.ctx()) {
            None
        } else {
            ui.input(|i| {
                let mut origin = None;
                let mut end = None;
                for event in &i.events {
                    if let egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        ..
                    } = event
                    {
                        if *pressed {
                            origin = response.rect.contains(*pos).then_some(*pos);
                        } else if let Some(start) = origin.take()
                            && pos.distance(start) > 3.
                        {
                            end = Some(*pos);
                        }
                    }
                }
                end
            })
        };
        if let Some(pointer) = completed_drag {
            preference.0 = Some((workspace.right() - pointer.x).clamp(min, max));
            response.request_focus();
        } else if response.double_clicked() {
            preference.0 = None;
        } else if (response.dragged() || response.drag_stopped())
            && let Some(pointer) = response.interact_pointer_pos()
        {
            preference.0 = Some((workspace.right() - pointer.x).clamp(min, max));
        }
        if response.has_focus() {
            for (key, direction) in [(Key::ArrowLeft, 1.), (Key::ArrowRight, -1.)] {
                let step = ui.input_mut(|i| {
                    if i.consume_key(egui::Modifiers::SHIFT, key) {
                        40.
                    } else if i.consume_key(egui::Modifiers::NONE, key) {
                        10.
                    } else {
                        0.
                    }
                });
                if step != 0. {
                    preference.0 =
                        Some((preference.0.unwrap_or(fit) + direction * step).clamp(min, max));
                }
            }
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Home)) {
                preference.0 = None;
            }
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(id, preference));
    let width = preference.0.unwrap_or(fit).clamp(min, max);
    let panel = egui::SidePanel::right("annotation-inspector")
        .resizable(false) // The divider below also supports reset and keyboard input.
        .exact_width(width)
        .show_separator_line(false)
        .frame(
            egui::Frame::side_top_panel(ui.style())
                .fill(crate::appearance::palette(ui.ctx()).paper)
                .inner_margin(PANEL_MARGIN),
        )
        .show_inside(ui, contents);

    let edge = panel.response.rect.left();
    let grab = Rect::from_min_max(
        Pos2::new(edge - 5., workspace.top()),
        Pos2::new(edge + 5., workspace.bottom()),
    );
    let response = ui
        .interact(grab, divider_id, Sense::click_and_drag())
        .on_hover_cursor(CursorIcon::ResizeHorizontal)
        .on_hover_text("Drag to resize · Double-click to fit\nArrow keys resize · Home resets");
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            "Resize properties panel; double-click to fit",
        )
    });
    if response.clicked() || response.drag_started() {
        response.request_focus();
    }
    if response.has_focus() {
        ui.memory_mut(|memory| {
            memory.set_focus_lock_filter(
                divider_id,
                egui::EventFilter {
                    horizontal_arrows: true,
                    ..Default::default()
                },
            )
        });
    }
    let palette = crate::appearance::palette(ui.ctx());
    let active = response.hovered() || response.dragged() || response.has_focus();
    ui.painter().line_segment(
        [
            Pos2::new(edge, workspace.top()),
            Pos2::new(edge, workspace.bottom()),
        ],
        Stroke::new(
            if active { 2. } else { 1. },
            if active {
                palette.accent
            } else {
                palette.border
            },
        ),
    );
    // A subtle grip makes resizing discoverable without adding a toolbar control.
    ui.painter().rect_filled(
        Rect::from_center_size(Pos2::new(edge, workspace.center().y), Vec2::new(3., 28.)),
        2.,
        if active {
            palette.accent
        } else {
            palette.border
        },
    );
    panel
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        document::{AnnotationKind, Document, EditHistory},
        editor::{EditorState, Tool},
    };
    use egui::Color32;

    #[test]
    fn divider_resizes_and_resets_without_drawing_on_the_canvas() {
        let ctx = egui::Context::default();
        crate::appearance::apply(&ctx);
        let texture = ctx.load_texture(
            "inspector-test",
            egui::ColorImage::filled([1, 1], Color32::WHITE),
            Default::default(),
        );
        let mut editor = EditorState {
            tool: Tool::Pen,
            inspector: State::pinned(Panel::Styles),
            ..Default::default()
        };
        let mut document = Document {
            image: std::sync::Arc::new(image::RgbaImage::new(900, 600)),
            annotations: vec![],
            geometry: Default::default(),
        };
        let before = document.edit_state();
        let mut history = EditHistory::new(before.clone());
        let mut preview = Default::default();
        let mut time = 0.;
        let mut frame = |events, size: Vec2| {
            time += 0.05;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        editor.ui(ui, &mut document, &mut history, &texture, &mut preview);
                    });
                },
            );
            egui::containers::panel::PanelState::load(&ctx, Id::new("annotation-inspector"))
                .unwrap()
                .rect
        };
        let size = Vec2::new(1100., 720.);
        for _ in 0..3 {
            frame(vec![], size);
        }
        let fitted = frame(vec![], size);
        let at = fitted.left_center();
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![egui::Event::PointerMoved(at), button(at, true)], size);
        let to = at - Vec2::new(140., 0.);
        frame(vec![egui::Event::PointerMoved(to)], size);
        frame(vec![], size);
        frame(vec![button(to, false)], size);
        let wider = frame(vec![], size);
        assert!(
            (wider.width() - fitted.width() - 140.).abs() < 2.,
            "{fitted:?} -> {wider:?}"
        );
        // A smaller window constrains the width; enlarging it restores the user's choice.
        let small = frame(vec![], Vec2::new(700., 720.));
        assert!(small.left() >= 240.);
        assert!((frame(vec![], size).width() - wider.width()).abs() < 1.);
        let at = wider.left_center();
        for _ in 0..2 {
            frame(vec![egui::Event::PointerMoved(at), button(at, true)], size);
            frame(vec![button(at, false)], size);
        }
        let reset = frame(vec![], size);
        assert!(
            (reset.width() - fitted.width()).abs() < 1.,
            "{reset:?} vs {fitted:?}"
        );
        // Rapid input may arrive between repaints, including the final release.
        let at = reset.left_center();
        let to = at - Vec2::new(80., 0.);
        frame(
            vec![
                egui::Event::PointerMoved(at),
                button(at, true),
                egui::Event::PointerMoved(to),
                button(to, false),
            ],
            size,
        );
        let rapid = frame(vec![], size);
        assert!((rapid.width() - reset.width() - 80.).abs() < 2.);
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        let keyboard = frame(vec![key(Key::ArrowLeft, egui::Modifiers::SHIFT)], size);
        assert!((keyboard.width() - rapid.width() - 40.).abs() < 2.);
        let keyboard = frame(vec![key(Key::ArrowLeft, egui::Modifiers::NONE)], size);
        assert!((keyboard.width() - rapid.width() - 50.).abs() < 2.);
        let home = frame(vec![key(Key::Home, egui::Modifiers::NONE)], size);
        assert!((home.width() - fitted.width()).abs() < 1.);
        assert_eq!(document.edit_state(), before);
        assert!(!history.can_undo());
        assert!(editor.gesture.is_none());
    }
    struct Workspace {
        ctx: egui::Context,
        editor: EditorState,
        document: Document,
        history: EditHistory,
        texture: egui::TextureHandle,
        preview: crate::privacy_preview::PrivacyPreview,
        time: f64,
        size: Vec2,
    }

    impl Workspace {
        fn new() -> Self {
            let ctx = egui::Context::default();
            crate::appearance::apply(&ctx);
            ctx.enable_accesskit();
            let texture = ctx.load_texture(
                "inspector-source",
                egui::ColorImage::filled([1, 1], Color32::WHITE),
                Default::default(),
            );
            let document = Document {
                image: std::sync::Arc::new(image::RgbaImage::new(1200, 800)),
                annotations: vec![],
                geometry: Default::default(),
            };
            let history = EditHistory::new(document.edit_state());
            let mut this = Self {
                ctx,
                editor: EditorState::default(),
                document,
                history,
                texture,
                preview: Default::default(),
                time: 0.,
                size: Vec2::new(1100., 740.),
            };
            this.editor.activate_tool(Tool::Arrow);
            this.settle();
            this
        }

        fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
            self.time += 0.03;
            self.ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
                    time: Some(self.time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::TopBottomPanel::top("test-ribbon").show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            self.editor
                                .toolbar(ui, &mut self.document, &mut self.history)
                        });
                    });
                    egui::CentralPanel::default().show(ctx, |ui| {
                        self.editor.ui(
                            ui,
                            &mut self.document,
                            &mut self.history,
                            &self.texture,
                            &mut self.preview,
                        );
                    });
                },
            )
        }

        fn settle(&mut self) -> egui::FullOutput {
            self.frame(vec![]);
            self.frame(vec![]);
            self.frame(vec![])
        }

        fn click(&mut self, label: &str) {
            let output = self.settle();
            let nodes = &output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes;
            let at = nodes
                .iter()
                .find_map(|(_, node)| {
                    (node.label() == Some(label))
                        .then(|| node.bounds())
                        .flatten()
                        .map(|r| {
                            egui::pos2(((r.x0 + r.x1) * 0.5) as f32, ((r.y0 + r.y1) * 0.5) as f32)
                        })
                })
                .unwrap_or_else(|| panic!("Missing control {label}"));
            for pressed in [true, false] {
                self.frame(vec![egui::Event::PointerMoved(at), pointer(at, pressed)]);
            }
            self.settle();
        }

        fn right_click(&mut self, label: &str) {
            let output = self.settle();
            let bounds = output
                .platform_output
                .accesskit_update
                .unwrap()
                .nodes
                .iter()
                .find_map(|(_, n)| (n.label() == Some(label)).then(|| n.bounds()).flatten())
                .unwrap();
            let at = egui::pos2(
                ((bounds.x0 + bounds.x1) * 0.5) as f32,
                ((bounds.y0 + bounds.y1) * 0.5) as f32,
            );
            for pressed in [true, false] {
                self.frame(vec![
                    egui::Event::PointerMoved(at),
                    egui::Event::PointerButton {
                        pos: at,
                        pressed,
                        button: egui::PointerButton::Secondary,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]);
            }
            self.settle();
        }

        fn choose_panel(&mut self, panel: Panel) {
            if panel != Panel::Styles {
                self.click("Choose panel");
            }
            self.click(panel.label());
        }

        fn source_quad(&mut self) -> Vec<(Pos2, Pos2)> {
            let output = self.settle();
            self.ctx
                .tessellate(output.shapes, output.pixels_per_point)
                .into_iter()
                .filter_map(|primitive| match primitive.primitive {
                    egui::epaint::Primitive::Mesh(mesh) if mesh.texture_id == self.texture.id() => {
                        Some(
                            mesh.vertices
                                .into_iter()
                                .map(|v| (v.pos, v.uv))
                                .collect::<Vec<_>>(),
                        )
                    }
                    _ => None,
                })
                .flatten()
                .collect()
        }
    }

    fn pointer(pos: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn ribbon_panels_pin_and_unpin_without_changing_artwork_or_overlay_canvas_geometry() {
        let mut workspace = Workspace::new();
        let before = workspace.document.edit_state();
        let original = workspace.source_quad();
        assert_eq!(original.len(), 4);
        let zoom = workspace.editor.zoom;
        assert!(
            egui::containers::panel::PanelState::load(&workspace.ctx, Id::new("inspector-rail"))
                .is_none()
        );
        assert!(!workspace.editor.inspector.preferences.expanded);
        for panel in Panel::ALL {
            workspace.choose_panel(panel);
            assert!(workspace.editor.inspector.preferences.expanded);
            assert!(workspace.editor.inspector.is(panel));
            assert_eq!(
                workspace.source_quad(),
                original,
                "{} moved/rescaled the canvas",
                panel.label()
            );
            assert_eq!(workspace.editor.zoom, zoom);
        }
        workspace.click("Pin panel");
        assert!(workspace.editor.inspector.preferences.pinned);
        assert_ne!(workspace.source_quad(), original);
        workspace.click("Unpin panel");
        assert!(!workspace.editor.inspector.preferences.pinned);
        assert_eq!(workspace.source_quad(), original);
        workspace.click("Close properties");
        assert!(!workspace.editor.inspector.preferences.expanded);
        assert!(workspace.editor.inspector.is(Panel::Effects));
        assert_eq!(workspace.source_quad(), original);
        assert_eq!(workspace.document.edit_state(), before);
        assert!(!workspace.history.can_undo());
    }

    #[test]
    fn overlay_controls_and_fast_pen_input_do_not_reach_the_image_underneath() {
        let mut workspace = Workspace::new();
        workspace.click("Styles");
        let output = workspace.settle();
        let mut swatches: Vec<_> = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .iter()
            .filter_map(|(_, n)| {
                n.label()
                    .filter(|label| label.ends_with("quick style"))
                    .and_then(|_| n.bounds())
            })
            .collect();
        swatches.sort_by(|a, b| a.y0.total_cmp(&b.y0).then_with(|| a.x0.total_cmp(&b.x0)));
        assert_eq!(swatches.len(), 4);
        assert!(
            swatches[..4]
                .iter()
                .all(|r| (r.y0 - swatches[0].y0).abs() < 1.)
        );
        assert!(swatches.iter().all(|r| (r.y1 - r.y0 - 36.).abs() < 1.));
        let overlay = workspace
            .ctx
            .memory(|m| m.area_rect(Id::new("inspector-overlay")))
            .unwrap();
        assert!(
            swatches
                .iter()
                .all(|r| r.x0 >= f64::from(overlay.left()) && r.x1 <= f64::from(overlay.right()))
        );
        let before = workspace.document.edit_state();
        workspace.click("Arrow · Blue quick style");
        assert_eq!(workspace.editor.color, [40, 117, 219, 255]);
        assert_eq!(workspace.editor.stroke_width, 4.);
        assert_eq!(workspace.document.edit_state(), before);
        workspace.editor.activate_tool(Tool::Pen);
        workspace.settle();
        let overlay = workspace
            .ctx
            .memory(|memory| memory.area_rect(Id::new("inspector-overlay")))
            .unwrap();
        let start = overlay.min + Vec2::new(16., 16.);
        let end = start + Vec2::new(70., 0.);
        workspace.frame(vec![
            egui::Event::PointerMoved(start),
            pointer(start, true),
            egui::Event::PointerMoved(end),
            pointer(end, false),
        ]);
        assert_eq!(workspace.document.edit_state(), before);
        assert!(workspace.editor.gesture.is_none());
        assert!(!workspace.history.can_undo());
        workspace.editor.area = Some(Rect::from_min_size(egui::pos2(20., 20.), Vec2::splat(60.)));
        workspace.frame(vec![egui::Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert!(!workspace.editor.inspector.preferences.expanded);
        assert!(
            workspace.editor.area.is_some(),
            "closing properties must not also deselect pixels"
        );
    }

    #[test]
    fn quick_style_tabs_save_apply_update_and_clear_without_resizing_the_canvas() {
        let mut workspace = Workspace::new();
        workspace.click("Styles");
        let image = workspace.source_quad();
        let artwork = workspace.document.edit_state();
        workspace.click("Quick styles 2");
        workspace.click("Arrow · Green quick style");
        let green = workspace.editor.color;
        let output = workspace.settle();
        let nodes = output.platform_output.accesskit_update.unwrap().nodes;
        assert_eq!(
            nodes
                .iter()
                .filter(|(_, n)| n.label().is_some_and(|l| l.ends_with("quick style")))
                .count(),
            4
        );
        assert!(
            !nodes
                .iter()
                .any(|(_, n)| n.label() == Some("Arrow · Red quick style"))
        );
        workspace.click("Quick styles 3");
        for slot in 1..=4 {
            workspace.click(&format!("Save current Arrow style {slot}"));
        }
        workspace.click("Quick styles 1");
        workspace.click("Arrow · Red quick style");
        let red = workspace.editor.color;
        workspace.click("Quick styles 3");
        workspace.click("Apply custom Arrow style 1");
        assert_eq!(workspace.editor.color, green);
        workspace.click("Quick styles 1");
        workspace.click("Arrow · Red quick style");
        workspace.click("Quick styles 3");
        workspace.right_click("Apply custom Arrow style 1");
        workspace.click("Update from current style");
        workspace.click("Apply custom Arrow style 2");
        assert_eq!(workspace.editor.color, green);
        workspace.click("Apply custom Arrow style 1");
        assert_eq!(workspace.editor.color, red);
        workspace.right_click("Apply custom Arrow style 1");
        workspace.click("Clear saved style");
        let output = workspace.settle();
        assert!(
            output
                .platform_output
                .accesskit_update
                .unwrap()
                .nodes
                .iter()
                .any(|(_, n)| n.label() == Some("Save current Arrow style 1"))
        );
        assert!(
            workspace
                .editor
                .custom_quick_styles(crate::document::AnnotationKind::Arrow)[0]
                .is_none()
        );
        assert!(workspace.editor.take_tool_preferences_change().is_some());
        assert_eq!(workspace.source_quad(), image);
        assert_eq!(workspace.document.edit_state(), artwork);
        assert!(!workspace.history.can_undo());
    }

    #[test]
    fn styles_default_on_restore_and_document_reset_preserving_pin_and_tool_appearances() {
        let mut workspace = Workspace::new();
        workspace.click("Styles");
        workspace.click("Arrow · Blue quick style");
        workspace.choose_panel(Panel::Layers);
        workspace.click("Pin panel");
        let tools = workspace.editor.take_tool_preferences_change().unwrap();
        let inspector = workspace
            .editor
            .take_inspector_preferences_change()
            .unwrap();
        assert_eq!(
            inspector,
            Preferences {
                panel: Panel::Layers,
                pinned: true,
                expanded: true
            }
        );
        let settings = crate::storage::Settings {
            editor_tools: tools.clone(),
            editor_inspector: inspector,
            ..Default::default()
        };
        let restored: crate::storage::Settings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        let mut editor = EditorState::default();
        editor.set_tool_preferences(restored.editor_tools);
        editor.set_inspector_preferences(restored.editor_inspector);
        editor.reset();
        assert_eq!(
            editor.inspector.preferences,
            Preferences {
                panel: Panel::Styles,
                ..inspector
            }
        );
        assert_eq!(editor.tool_preferences, tools);
        editor.activate_tool(Tool::Arrow);
        assert_eq!(editor.color, [40, 117, 219, 255]);
        assert_eq!(editor.stroke_width, 4.);
        editor.inspector.close();
        editor.reset();
        assert_eq!(
            editor.take_inspector_preferences_change(),
            Some(Preferences {
                panel: Panel::Styles,
                expanded: false,
                ..inspector
            })
        );
        let legacy: crate::storage::Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.editor_inspector, Preferences::default());
    }

    #[test]
    fn each_panel_stays_inside_a_small_workspace_at_every_text_size() {
        for scale in [1., 1.15, 1.3] {
            let mut workspace = Workspace::new();
            // UI points shrink as the native text/UI scale grows.
            workspace.size = Vec2::new(980., 640.) / scale;
            for panel in Panel::ALL {
                workspace.editor.inspector.show(panel);
                workspace.settle();
                let rect = workspace
                    .ctx
                    .memory(|memory| memory.area_rect(Id::new("inspector-overlay")))
                    .unwrap();
                assert!(
                    Rect::from_min_size(Pos2::ZERO, workspace.size).contains_rect(rect),
                    "{panel:?}, {scale}: {rect:?}"
                );
                let output = workspace.settle();
                let nodes = output.platform_output.accesskit_update.unwrap().nodes;
                assert!(
                    nodes
                        .iter()
                        .any(|(_, n)| n.label() == Some("Close properties"))
                );
                assert!(nodes.iter().any(|(_, n)| n.label() == Some("Pin panel")));
            }
        }
    }
    #[test]
    fn narrow_ribbon_keeps_overflow_tools_and_all_panels_reachable() {
        let mut workspace = Workspace::new();
        workspace.size = Vec2::new(400., 640.);
        let output = workspace.settle();
        let nodes = output.platform_output.accesskit_update.unwrap().nodes;
        let bounds = |label| {
            nodes
                .iter()
                .find_map(|(_, n)| (n.label() == Some(label)).then(|| n.bounds()).flatten())
                .unwrap()
        };
        assert!(bounds("Styles").x0 >= bounds("More tools").x1);
        assert!(bounds("Choose panel").x1 <= f64::from(workspace.size.x));
        assert!(!nodes.iter().any(|(_, n)| n.label() == Some("Fill")));
        workspace.click("More tools");
        workspace.click("Fill");
        assert_eq!(workspace.editor.tool, Tool::Fill);
        for (label, tool) in [("Pen", Tool::Pen), ("Magnify", Tool::Magnify)] {
            let output = workspace.settle();
            assert!(
                !output
                    .platform_output
                    .accesskit_update
                    .unwrap()
                    .nodes
                    .iter()
                    .any(|(_, n)| n.label() == Some(label))
            );
            workspace.click("More tools");
            workspace.click(label);
            assert_eq!(workspace.editor.tool, tool);
        }
        for panel in [Panel::Layers, Panel::Presets, Panel::Effects] {
            workspace.choose_panel(panel);
            assert!(workspace.editor.inspector.is(panel));
            assert!(workspace.editor.inspector.preferences.expanded);
        }
        workspace.click("Styles");
        assert!(workspace.editor.inspector.is(Panel::Styles));
        assert!(workspace.editor.inspector.preferences.expanded);
        workspace.click("Close properties");
        workspace.click("Styles");
        assert!(workspace.editor.inspector.is(Panel::Styles));
        assert!(workspace.editor.inspector.preferences.expanded);
    }

    #[test]
    fn floating_annotation_controls_do_not_cover_the_open_inspector() {
        let mut workspace = Workspace::new();
        workspace.click("Styles");
        let mut arrow = workspace.editor.new_annotation(
            crate::document::AnnotationKind::Arrow,
            crate::document::Point { x: 950., y: 250. },
        );
        arrow.end = crate::document::Point { x: 1150., y: 400. };
        workspace.document.annotations.push(arrow);
        workspace.editor.select(Some(0), &workspace.document);
        workspace.settle();
        let overlay = workspace
            .ctx
            .memory(|m| m.area_rect(Id::new("inspector-overlay")))
            .unwrap();
        let bar = workspace.ctx.memory(|m| {
            m.area_rect(
                Id::new("selected-object-properties")
                    .with(crate::document::AnnotationKind::Arrow as u8)
                    .with(1_usize),
            )
        });
        assert!(
            bar.is_none_or(|bar| !bar.intersects(overlay)),
            "Floating controls {bar:?} cover inspector {overlay:?}"
        );
    }
    #[test]
    fn object_styles_shortcut_opens_styles_without_editing_or_deselecting() {
        for multiple in [false, true] {
            let mut w = Workspace::new();
            let mut arrow = w.editor.new_annotation(
                crate::document::AnnotationKind::Arrow,
                crate::document::Point { x: 200., y: 280. },
            );
            arrow.end = crate::document::Point { x: 500., y: 400. };
            w.document.annotations = vec![arrow.clone(), arrow];
            w.editor
                .set_selection(if multiple { vec![0, 1] } else { vec![0] }, &w.document);
            w.editor.inspector.select(Panel::Layers);
            w.editor.inspector.close();
            let before = w.document.edit_state();
            let selected = w.editor.selected_indices(&w.document);
            let color = w.editor.color;
            w.click("Open Styles");
            assert!(w.editor.inspector.is(Panel::Styles));
            assert!(w.editor.inspector.preferences.expanded);
            assert_eq!(w.editor.selected_indices(&w.document), selected);
            assert_eq!(w.editor.color, color);
            assert_eq!(w.document.edit_state(), before);
            assert!(!w.history.can_undo());
        }
    }

    #[test]
    fn customizer_reorders_unpinned_tools_and_overflow_uses_that_order() {
        let mut w = Workspace::new();
        w.editor.set_toolbar_order(Vec::new());
        w.click("More tools");
        w.click("Customize toolbar…");
        w.click("Move Connected Arrows later");
        assert!(
            !w.editor
                .toolbar_pins
                .iter()
                .any(|v| v == "Connected Arrows")
        );
        w.editor.toolbar_customizer = false;
        w.click("More tools");
        let output = w.settle();
        let nodes = output.platform_output.accesskit_update.unwrap().nodes;
        let top = |label| {
            nodes
                .iter()
                .find_map(|(_, n)| (n.label() == Some(label)).then(|| n.bounds()).flatten())
                .unwrap()
                .y0
        };
        assert!(top("Smart Erase / Repair") < top("Connected Arrows"));
        w.click("Connected Arrows");
        assert_eq!(w.editor.tool, Tool::ConnectedArrow);
    }
    #[test]
    fn canvas_resize_menu_repeats_steps_presets_and_opens_detailed_dialog_with_shortcut() {
        fn open_menu(w: &mut Workspace) {
            w.frame(vec![egui::Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]);
            w.settle();
            let quad = w.source_quad();
            let rect = quad.iter().fold(Rect::NOTHING, |r, (p, _)| {
                r.union(Rect::from_min_max(*p, *p))
            });
            let at = rect.center();
            for pressed in [true, false] {
                w.frame(vec![
                    egui::Event::PointerMoved(at),
                    egui::Event::PointerButton {
                        pos: at,
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]);
            }
            w.settle();
        }
        let mut w = Workspace::new();
        w.editor.inspector.close();
        open_menu(&mut w);
        w.click("Increase image size by 10%");
        assert_eq!(w.document.output_size(), [1320, 880]);
        w.click("Increase image size by 10%");
        assert_eq!(w.document.output_size(), [1452, 968]);
        w.click("Decrease image size by 10%");
        assert_eq!(w.document.output_size(), [1307, 871]);
        for (label, size) in [
            ("25%", [300, 200]),
            ("50%", [600, 400]),
            ("125%", [1500, 1000]),
            ("150%", [1800, 1200]),
            ("Set to original", [1200, 800]),
        ] {
            open_menu(&mut w);
            let menu = w.settle();
            assert!(
                !menu
                    .platform_output
                    .accesskit_update
                    .unwrap()
                    .nodes
                    .iter()
                    .any(|(_, node)| node.label() == Some("Resize presets"))
            );
            w.click("Resize image…");
            let before = w.document.edit_state();
            w.click(label);
            assert_eq!(w.editor.resize.as_ref().unwrap().size, size);
            assert_eq!(w.document.edit_state(), before);
            w.click("Apply resize");
            assert!(w.editor.resize.is_none());
            assert_eq!(w.document.output_size(), size);
        }
        open_menu(&mut w);
        w.click("Resize image…");
        assert!(w.editor.resize.is_some());
        let before_cancel = w.document.edit_state();
        w.click("25%");
        w.click("Cancel");
        assert!(w.editor.resize.is_none());
        assert_eq!(w.document.edit_state(), before_cancel);
        // The icon groups share rows and retain their original actions.
        open_menu(&mut w);
        let output = w.settle();
        let nodes = output.platform_output.accesskit_update.unwrap().nodes;
        let bounds = |label| {
            nodes
                .iter()
                .find_map(|(_, n)| (n.label() == Some(label)).then(|| n.bounds()).flatten())
                .unwrap()
        };
        for labels in [
            vec!["Rotate left", "Rotate right", "Rotate 180°"],
            vec!["Flip horizontal", "Flip vertical"],
            vec!["Select all", "Select all annotations", "Deselect"],
        ] {
            let row = bounds(labels[0]);
            for label in &labels[1..] {
                assert!((bounds(label).y0 - row.y0).abs() < 1.0);
            }
        }
        for (label, turns) in [("Rotate left", 3), ("Rotate right", 0), ("Rotate 180°", 2)] {
            open_menu(&mut w);
            w.click(label);
            assert_eq!(w.document.geometry.quarter_turns, turns);
        }
        for label in ["Flip horizontal", "Flip vertical"] {
            open_menu(&mut w);
            let before = w.document.geometry;
            w.click(label);
            assert_ne!(w.document.geometry, before);
            assert!(w.history.undo(&mut w.document));
            assert_eq!(w.document.geometry, before);
        }
        let quad = w.source_quad();
        let rect = quad.iter().fold(Rect::NOTHING, |r, (p, _)| {
            r.union(Rect::from_min_max(*p, *p))
        });
        let at = rect.center();
        for pressed in [true, false] {
            w.frame(vec![egui::Event::PointerMoved(at), pointer(at, pressed)]);
        }
        let before = w.document.edit_state();
        w.frame(vec![egui::Event::Key {
            key: Key::R,
            physical_key: Some(Key::R),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                shift: true,
                ..egui::Modifiers::COMMAND
            },
        }]);
        assert!(w.editor.resize.is_some());
        assert_eq!(w.document.edit_state(), before);
        w.click("Cancel");
        w.document.annotations = vec![
            crate::markup_tests::object(AnnotationKind::Rectangle, [10., 10.], [60., 60.]),
            crate::markup_tests::object(AnnotationKind::Arrow, [80., 80.], [180., 160.]),
        ];
        w.history = EditHistory::new(w.document.edit_state());
        let before_selection = w.document.edit_state();
        open_menu(&mut w);
        w.click("Select all");
        assert!(w.editor.area.is_some());
        open_menu(&mut w);
        w.click("Select all annotations");
        assert_eq!(w.editor.selected_indices(&w.document), vec![0, 1]);
        assert!(w.editor.area.is_none());
        open_menu(&mut w);
        w.click("Deselect");
        assert!(w.editor.selected_indices(&w.document).is_empty());
        assert!(w.editor.area.is_none());
        assert_eq!(w.document.edit_state(), before_selection);
        assert!(!w.history.can_undo());
    }
}
