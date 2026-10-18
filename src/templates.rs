use crate::document::{Annotation, AnnotationKind, Document, Point};
use eframe::egui::{self, Color32, Rect, Vec2};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Blank,
    Single,
    TwoSteps,
    ThreeSteps,
    FourSteps,
    BeforeAfter,
    Landscape,
}
pub const KINDS: [Kind; 7] = [
    Kind::Blank,
    Kind::Single,
    Kind::TwoSteps,
    Kind::ThreeSteps,
    Kind::FourSteps,
    Kind::BeforeAfter,
    Kind::Landscape,
];
impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Blank => "Blank canvas",
            Self::Single => "Visual overview",
            Self::TwoSteps => "Two-step guide",
            Self::ThreeSteps => "Three-step guide",
            Self::FourSteps => "Four-step guide",
            Self::BeforeAfter => "Before / after",
            Self::Landscape => "Landscape walkthrough",
        }
    }
    fn size(self) -> [u32; 2] {
        match self {
            Self::Blank => [1200, 800],
            Self::BeforeAfter | Self::Landscape => [1600, 1000],
            _ => [1000, 1400],
        }
    }
    fn slots(self) -> Vec<[f32; 4]> {
        match self {
            Self::Blank => vec![],
            Self::Single => vec![[70., 220., 860., 790.]],
            Self::TwoSteps => vec![[70., 210., 860., 420.], [70., 790., 860., 420.]],
            Self::ThreeSteps => (0..3)
                .map(|i| [70., 210. + i as f32 * 380., 500., 270.])
                .collect(),
            Self::FourSteps => (0..4)
                .map(|i| {
                    [
                        70. + (i % 2) as f32 * 450.,
                        220. + (i / 2) as f32 * 560.,
                        410.,
                        390.,
                    ]
                })
                .collect(),
            Self::BeforeAfter => vec![[70., 230., 700., 560.], [830., 230., 700., 560.]],
            Self::Landscape => (0..3)
                .map(|i| [65. + i as f32 * 510., 230., 460., 500.])
                .collect(),
        }
    }
}
#[derive(Clone)]
pub struct Spec {
    pub kind: Kind,
    pub title: String,
    pub size: [u32; 2],
    pub background: [u8; 4],
}
pub struct CreatePanel {
    kind: Kind,
    title: String,
    size: [u32; 2],
    background: [u8; 4],
}
impl Default for CreatePanel {
    fn default() -> Self {
        Self {
            kind: Kind::TwoSteps,
            title: "How to get started".into(),
            size: [1200, 800],
            background: [255; 4],
        }
    }
}
impl CreatePanel {
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<Spec> {
        let mut create = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Create an image");
            ui.weak("Start with a blank canvas or an editable visual guide.");
            ui.add_space(12.);
            let columns = ((ui.available_width() / 210.).floor() as usize).clamp(2, 4);
            egui::Grid::new("template-gallery")
                .spacing([12., 12.])
                .num_columns(columns)
                .show(ui, |ui| {
                    for (index, kind) in KINDS.into_iter().enumerate() {
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::new(190., 172.), egui::Sense::click());
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::SelectableLabel,
                                ui.is_enabled(),
                                kind == self.kind,
                                kind.name(),
                            )
                        });
                        ui.painter().rect(
                            rect,
                            8,
                            Color32::WHITE,
                            egui::Stroke::new(
                                if kind == self.kind { 2. } else { 1. },
                                if kind == self.kind {
                                    crate::appearance::ACCENT
                                } else {
                                    Color32::from_gray(215)
                                },
                            ),
                            egui::StrokeKind::Inside,
                        );
                        let [w, h] = kind.size();
                        let scale = (160. / w as f32).min(130. / h as f32);
                        let page = Rect::from_center_size(
                            rect.center_top() + Vec2::new(0., 74.),
                            Vec2::new(w as f32, h as f32) * scale,
                        );
                        ui.painter()
                            .rect_filled(page, 1, Color32::from_rgb(247, 250, 252));
                        if kind != Kind::Blank {
                            ui.painter().rect_filled(
                                Rect::from_min_size(
                                    page.min + Vec2::new(70., 65.) * scale,
                                    Vec2::new(w as f32 * 0.55, 35.) * scale,
                                ),
                                1,
                                crate::appearance::ACCENT,
                            );
                        }
                        for [x, y, w, h] in kind.slots() {
                            let slot = Rect::from_min_size(
                                page.min + Vec2::new(x, y) * scale,
                                Vec2::new(w, h) * scale,
                            );
                            ui.painter()
                                .rect_filled(slot, 2, Color32::from_rgb(218, 231, 238));
                            crate::appearance::icon(
                                ui.painter(),
                                slot.center(),
                                crate::appearance::Icon::View,
                                Color32::from_rgb(127, 155, 169),
                                slot.width().min(slot.height()) * 0.3,
                            );
                        }
                        ui.painter().text(
                            rect.center_bottom() - Vec2::new(0., 17.),
                            egui::Align2::CENTER_CENTER,
                            kind.name(),
                            egui::FontId::proportional(12.),
                            crate::appearance::INK,
                        );
                        if response.clicked() {
                            self.kind = kind;
                        }
                        if index % columns == columns - 1 {
                            ui.end_row();
                        }
                    }
                });
            ui.add_space(12.);
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Title");
                ui.add(
                    egui::TextEdit::singleline(&mut self.title)
                        .desired_width(330.)
                        .char_limit(100),
                );
            });
            if self.kind == Kind::Blank {
                ui.horizontal(|ui| {
                    ui.label("Canvas");
                    ui.add(
                        egui::DragValue::new(&mut self.size[0])
                            .range(64..=4096)
                            .suffix(" px"),
                    );
                    ui.label("×");
                    ui.add(
                        egui::DragValue::new(&mut self.size[1])
                            .range(64..=4096)
                            .suffix(" px"),
                    );
                    ui.label("Background");
                    ui.color_edit_button_srgba_unmultiplied(&mut self.background);
                });
            } else {
                let [w, h] = self.kind.size();
                ui.weak(format!(
                    "{w} × {h} px · Replace each image placeholder and edit the captions in Editor."
                ));
            }
            if crate::appearance::action_button(
                ui,
                "Create image",
                crate::appearance::Icon::Create,
                true,
                true,
            )
            .clicked()
            {
                create = Some(Spec {
                    kind: self.kind,
                    title: self.title.trim().into(),
                    size: if self.kind == Kind::Blank {
                        self.size
                    } else {
                        self.kind.size()
                    },
                    background: if self.kind == Kind::Blank {
                        self.background
                    } else {
                        [255; 4]
                    },
                });
            }
        });
        create
    }
}
fn annotation(kind: AnnotationKind, rect: [f32; 4]) -> Annotation {
    Annotation {
        kind,
        start: Point {
            x: rect[0],
            y: rect[1],
        },
        end: Point {
            x: rect[0] + rect[2],
            y: rect[1] + rect[3],
        },
        color: [30, 45, 60, 255],
        stroke_width: 3.,
        fill: true,
        text: String::new(),
        font_size: 28.,
        effect_size: 12,
        step_number: 1,
        tail: None,
        text_color: [255; 4],
        points: vec![],
        group_id: None,
        layer: Default::default(),
        group_layer: Default::default(),
        area_source: None,
        area_mask: vec![],
        stamp: None,
        fill_region: None,
        text_style: Default::default(),
        pixel_mask: None,
        arrow: Default::default(),
        studio: Default::default(),
    }
}
pub fn build(spec: &Spec) -> Result<Document, String> {
    if spec.size.iter().any(|&s| !(64..=4096).contains(&s)) {
        return Err("Choose canvas dimensions from 64 to 4096 pixels.".into());
    }
    let mut doc = Document {
        image: Arc::new(image::RgbaImage::from_pixel(
            spec.size[0],
            spec.size[1],
            image::Rgba(spec.background),
        )),
        annotations: vec![],
        geometry: Default::default(),
    };
    if spec.kind == Kind::Blank {
        return Ok(doc);
    }
    let mut title = annotation(
        AnnotationKind::Text,
        [70., 55., spec.size[0] as f32 - 140., 70.],
    );
    title.text = if spec.title.is_empty() {
        spec.kind.name().into()
    } else {
        spec.title.clone()
    };
    title.font_size = 48.;
    title.color = [0, 125, 137, 255];
    title.layer.name = "Title".into();
    doc.annotations.push(title);
    let mut subtitle = annotation(AnnotationKind::Text, [70., 135., 800., 40.]);
    subtitle.text = "Double-click to add a short introduction".into();
    subtitle.font_size = 23.;
    subtitle.color = [100, 116, 132, 255];
    doc.annotations.push(subtitle);
    for (i, rect) in spec.kind.slots().into_iter().enumerate() {
        let mut a = annotation(AnnotationKind::Stamp, rect);
        a.color = [255; 4];
        a.layer.name = format!("Image {} · replace artwork", i + 1);
        a.stamp=Some(crate::stamps::StampSource::Svg{name:format!("Image {} placeholder",i+1),prompt:String::new(),svg:Arc::new(r##"<svg xmlns="http://www.w3.org/2000/svg" width="512" height="320" viewBox="0 0 512 320"><rect width="512" height="320" rx="12" fill="#eaf1f5"/><rect x="176" y="92" width="160" height="116" rx="10" fill="none" stroke="#9aafbb" stroke-width="6"/><circle cx="286" cy="124" r="12" fill="#9aafbb"/><path d="M190 190 L235 138 L275 178 L304 156 L325 190" fill="none" stroke="#9aafbb" stroke-width="7" stroke-linejoin="round"/></svg>"##.into())});
        doc.annotations.push(a);
        let side = spec.kind == Kind::ThreeSteps;
        let (x, y) = if side {
            (610., rect[1] + 40.)
        } else {
            (rect[0], rect[1] + rect[3] + 24.)
        };
        let mut caption = annotation(AnnotationKind::Text, [x, y, rect[2], 45.]);
        caption.text = if spec.kind == Kind::BeforeAfter {
            if i == 0 {
                "Before".into()
            } else {
                "After".into()
            }
        } else {
            format!("{}. Add a step title", i + 1)
        };
        caption.font_size = 28.;
        caption.layer.name = format!("Caption {}", i + 1);
        doc.annotations.push(caption);
        let mut detail = annotation(AnnotationKind::Text, [x, y + 46., rect[2], 42.]);
        detail.text = if side {
            "Explain this step\nin a few words".into()
        } else {
            "Double-click to describe this image".into()
        };
        detail.font_size = 20.;
        detail.color = [100, 116, 132, 255];
        doc.annotations.push(detail);
    }
    crate::document::validate_document(&doc)?;
    Ok(doc)
}
