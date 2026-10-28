//! Last explicitly chosen drawing properties, independent of document selection.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolPreferences {
    appearances: Vec<RememberedAppearance>,
    custom_styles: Vec<CustomStyles>,
    shape: AnnotationKind,
    eraser_diameter: f32,
    pub(super) fill: FillPreferences,
}

impl Default for ToolPreferences {
    fn default() -> Self {
        Self {
            appearances: Vec::new(),
            custom_styles: Vec::new(),
            shape: AnnotationKind::Rectangle,
            eraser_diameter: 32.,
            fill: Default::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct FillPreferences {
    pub color: [u8; 4],
    pub tolerance: u8,
    pub global: bool,
}

impl Default for FillPreferences {
    fn default() -> Self {
        Self {
            color: [46, 137, 221, 255],
            tolerance: 15,
            global: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct RememberedAppearance {
    kind: AnnotationKind,
    appearance: Appearance,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct CustomStyles {
    kind: AnnotationKind,
    slots: [Option<Appearance>; 4],
}

/// Content, absolute geometry, step numbers and annotation identities are never
/// preferences. In particular, a selected arrow's curve belongs to its document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Appearance {
    color: [u8; 4],
    stroke_width: f32,
    fill: bool,
    arrow: crate::arrow_style::ArrowStyle,
    font_size: f32,
    text_color: [u8; 4],
    text_style: crate::text_style::TextStyle,
    callout_style: crate::callout_style::CalloutStyle,
    effect_size: u32,
    step_diameter: f32,
    studio: crate::studio::Options,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            color: [230, 66, 55, 255],
            stroke_width: 4.,
            fill: false,
            arrow: Default::default(),
            font_size: 32.,
            text_color: [255; 4],
            text_style: Default::default(),
            callout_style: Default::default(),
            effect_size: 16,
            step_diameter: 48.,
            studio: Default::default(),
        }
    }
}

impl Appearance {
    fn for_kind(kind: AnnotationKind) -> Self {
        let mut appearance = Self::default();
        match kind {
            AnnotationKind::Callout => appearance.color = [16, 119, 108, 255],
            AnnotationKind::Highlighter => appearance.color = [255, 220, 0, 80],
            AnnotationKind::Freehand => {
                appearance.color = [35, 112, 225, 255];
                appearance.stroke_width = 8.;
            }
            AnnotationKind::Simplify => {
                appearance.color = [161, 175, 190, 255];
                appearance.studio.ellipse = false;
            }
            _ => {}
        }
        appearance
    }

    pub(super) fn from_editor(editor: &EditorState) -> Self {
        Self {
            color: editor.color,
            stroke_width: editor.stroke_width,
            fill: editor.fill,
            arrow: crate::arrow_style::ArrowStyle {
                curve: None,
                connection: None,
                ..editor.arrow.clone()
            },
            font_size: editor.font_size,
            text_color: editor.text_color,
            text_style: editor.text_style.clone(),
            callout_style: editor.callout_style,
            effect_size: editor.effect_size,
            step_diameter: editor.step_diameter,
            studio: editor.studio.clone(),
        }
    }

    pub(super) fn from_annotation(annotation: &Annotation) -> Self {
        Self {
            color: annotation.color,
            stroke_width: annotation.stroke_width,
            fill: annotation.fill,
            arrow: crate::arrow_style::ArrowStyle {
                curve: None,
                connection: None,
                ..annotation.arrow.clone()
            },
            font_size: annotation.font_size,
            text_color: annotation.text_color,
            text_style: annotation.text_style.clone(),
            callout_style: annotation.callout_style,
            effect_size: annotation.effect_size,
            studio: annotation.studio.clone(),
            ..Default::default()
        }
    }

    /// Copy appearance only. Content, masks, stamp payloads, positions, groups,
    /// numbers and arrow/lens geometry all belong to the target annotation.
    pub(super) fn apply_to_annotation(&self, annotation: &mut Annotation) {
        let kind = annotation.kind;
        if kind.is_effect() {
            annotation.effect_size = self.effect_size;
            return;
        }
        if kind == AnnotationKind::Stamp {
            annotation.color[3] = self.color[3];
            return;
        }
        annotation.color = self.color;
        annotation.stroke_width = self.stroke_width;
        annotation.fill = self.fill;
        if kind == AnnotationKind::Arrow {
            let elbow = annotation.arrow.elbow;
            let connection = annotation.arrow.connection.clone();
            let curve = annotation.arrow.curve;
            annotation.arrow = self.arrow.clone();
            annotation.arrow.curve = curve;
            annotation.arrow.connection = connection;
            annotation.arrow.elbow = elbow;
        } else if kind.is_shape() {
            annotation.arrow.pattern = self.arrow.pattern;
        }
        if matches!(kind, AnnotationKind::Text | AnnotationKind::Callout) {
            annotation.font_size = self.font_size;
            let path = annotation.text_style.path.take();
            annotation.text_style = self.text_style.clone();
            annotation.text_style.path = path;
        }
        if matches!(kind, AnnotationKind::Callout | AnnotationKind::Step) {
            annotation.text_color = self.text_color;
        }
        if kind == AnnotationKind::Callout {
            annotation.callout_style = self.callout_style;
        }
        if kind.is_studio() {
            let offset = annotation.studio.source_offset;
            annotation.studio = self.studio.clone();
            annotation.studio.source_offset = offset;
        }
    }

    pub(super) fn paint_preview(&self, annotation: &mut Annotation) {
        annotation.color = self.color;
        annotation.stroke_width = self.stroke_width.min(7.);
        annotation.fill = self.fill;
        annotation.arrow = self.arrow.clone();
        annotation.font_size = self.font_size.min(32.);
        annotation.text_color = self.text_color;
        annotation.text_style = self.text_style.clone();
        annotation.text_style.path = None;
        annotation.arrow.connection = None;
        annotation.callout_style = self.callout_style;
        annotation.effect_size = self.effect_size;
        annotation.studio = self.studio.clone();
    }

    pub(super) fn effect_size(&self) -> u32 {
        self.effect_size
    }

    fn valid(&self) -> bool {
        self.text_style
            .path
            .as_ref()
            .is_none_or(|p| p.validate().is_ok())
            && self.stroke_width.is_finite()
            && (0.1..=crate::document::MAX_STROKE_WIDTH).contains(&self.stroke_width)
            && self.font_size.is_finite()
            && (1.0..=crate::document::MAX_FONT_SIZE).contains(&self.font_size)
            && (2..=crate::document::MAX_EFFECT_SIZE).contains(&self.effect_size)
            && self.step_diameter.is_finite()
            && (1.0..=32768.).contains(&self.step_diameter)
            && self.arrow.validate().is_ok()
            && self.studio.validate().is_ok()
    }

    fn apply(self, editor: &mut EditorState) {
        editor.color = self.color;
        editor.stroke_width = self.stroke_width;
        editor.fill = self.fill;
        editor.arrow = self.arrow;
        editor.font_size = self.font_size;
        editor.text_color = self.text_color;
        editor.text_style = self.text_style;
        editor.callout_style = self.callout_style;
        editor.effect_size = self.effect_size;
        editor.step_diameter = self.step_diameter;
        editor.studio = self.studio;
    }
}

impl ToolPreferences {
    fn normalized(mut self) -> Self {
        let mut entries = Vec::<RememberedAppearance>::new();
        for mut entry in self.appearances {
            entry.appearance.arrow.curve = None;
            entry.appearance.arrow.connection = None;
            if !entry.kind.is_area_edit() && entry.appearance.valid() {
                if let Some(existing) = entries.iter_mut().find(|e| e.kind == entry.kind) {
                    *existing = entry;
                } else {
                    entries.push(entry);
                }
            }
        }
        self.appearances = entries;
        let mut custom = Vec::<CustomStyles>::new();
        for mut entry in self.custom_styles {
            if entry.kind.is_area_edit()
                || matches!(entry.kind, AnnotationKind::Stamp | AnnotationKind::Fill)
            {
                continue;
            }
            for slot in &mut entry.slots {
                if let Some(appearance) = slot {
                    appearance.arrow.curve = None;
                    appearance.arrow.connection = None;
                    if !appearance.valid() {
                        *slot = None;
                    }
                }
            }
            if let Some(existing) = custom.iter_mut().find(|e| e.kind == entry.kind) {
                *existing = entry;
            } else {
                custom.push(entry);
            }
        }
        custom.retain(|entry| entry.slots.iter().any(Option::is_some));
        self.custom_styles = custom;
        if !self.shape.is_shape() {
            self.shape = AnnotationKind::Rectangle;
        }
        if !self.eraser_diameter.is_finite() || !(1.0..=512.).contains(&self.eraser_diameter) {
            self.eraser_diameter = 32.;
        }
        self.fill.tolerance = self.fill.tolerance.min(100);
        self
    }
}

impl EditorState {
    pub fn set_tool_preferences(&mut self, preferences: ToolPreferences) {
        self.tool_preferences = preferences.normalized();
        self.shape_kind = self.tool_preferences.shape;
        self.shapes_3d = self.shape_kind.is_3d();
        self.eraser_diameter = self.tool_preferences.eraser_diameter;
        self.restore_fill_preferences();
        if let Some(kind) = self.tool.kind() {
            self.restore_appearance(kind);
        }
        self.tool_preferences_changed = false;
    }

    pub fn take_tool_preferences_change(&mut self) -> Option<ToolPreferences> {
        std::mem::take(&mut self.tool_preferences_changed).then(|| self.tool_preferences.clone())
    }

    pub(super) fn custom_quick_styles(&self, kind: AnnotationKind) -> [Option<Appearance>; 4] {
        self.tool_preferences
            .custom_styles
            .iter()
            .find(|entry| entry.kind == kind)
            .map(|entry| entry.slots.clone())
            .unwrap_or_default()
    }

    pub(super) fn save_custom_quick_style(&mut self, kind: AnnotationKind, slot: usize) {
        if slot >= 4
            || kind.is_area_edit()
            || matches!(kind, AnnotationKind::Stamp | AnnotationKind::Fill)
        {
            return;
        }
        let appearance = Appearance::from_editor(self);
        if !appearance.valid() {
            return;
        }
        if let Some(entry) = self
            .tool_preferences
            .custom_styles
            .iter_mut()
            .find(|entry| entry.kind == kind)
        {
            self.tool_preferences_changed |= entry.slots[slot].as_ref() != Some(&appearance);
            entry.slots[slot] = Some(appearance);
        } else {
            let mut slots = std::array::from_fn(|_| None);
            slots[slot] = Some(appearance);
            self.tool_preferences
                .custom_styles
                .push(CustomStyles { kind, slots });
            self.tool_preferences_changed = true;
        }
    }

    pub(super) fn clear_custom_quick_style(&mut self, kind: AnnotationKind, slot: usize) {
        if slot >= 4 {
            return;
        }
        if let Some(entry) = self
            .tool_preferences
            .custom_styles
            .iter_mut()
            .find(|entry| entry.kind == kind)
        {
            self.tool_preferences_changed |= entry.slots[slot].take().is_some();
        }
        self.tool_preferences
            .custom_styles
            .retain(|entry| entry.slots.iter().any(Option::is_some));
    }

    pub(super) fn apply_custom_quick_style(
        &mut self,
        document: &mut Document,
        history: &mut EditHistory,
        kind: AnnotationKind,
        slot: usize,
    ) {
        if self.style_kind(document) != Some(kind)
            || self.selected.is_some() && !self.can_edit_selection(document)
            || self.selected_indices(document).len() > 1
        {
            return;
        }
        let Some(appearance) = self.custom_quick_styles(kind).get(slot).cloned().flatten() else {
            return;
        };
        self.finish_annotation_gesture(document, history);
        let elbow = self.arrow.elbow;
        let connection = self.arrow.connection.clone();
        let curve = self.arrow.curve;
        let path = self.text_style.path.clone();
        appearance.apply(self);
        if self.selected.is_some() && kind == AnnotationKind::Text {
            self.text_style.path = path;
        }
        // A style changes the arrow's appearance, not the selected path's bend.
        if self.selected.is_some() && kind == AnnotationKind::Arrow {
            self.arrow.curve = curve;
            self.arrow.connection = connection;
            self.arrow.elbow = elbow;
        }
        self.apply_inspector(document, history);
    }

    pub(super) fn remember_appearance(&mut self, kind: AnnotationKind) {
        self.remember_text_content(kind);
        if kind.is_area_edit() {
            return;
        }
        let appearance = Appearance::from_editor(self);
        if !appearance.valid() {
            return;
        }
        if let Some(entry) = self
            .tool_preferences
            .appearances
            .iter_mut()
            .find(|e| e.kind == kind)
        {
            self.tool_preferences_changed |= entry.appearance != appearance;
            entry.appearance = appearance;
        } else {
            self.tool_preferences
                .appearances
                .push(RememberedAppearance { kind, appearance });
            self.tool_preferences_changed = true;
        }
    }

    pub(super) fn remember_text_content(&mut self, kind: AnnotationKind) {
        match kind {
            AnnotationKind::Text => self.plain_text.clone_from(&self.text),
            AnnotationKind::Callout => self.callout_text.clone_from(&self.text),
            _ => {}
        }
    }

    pub(super) fn restore_appearance(&mut self, kind: AnnotationKind) {
        self.tool_preferences
            .appearances
            .iter()
            .find(|e| e.kind == kind)
            .map(|e| e.appearance.clone())
            .unwrap_or_else(|| Appearance::for_kind(kind))
            .apply(self);
    }

    pub(super) fn remember_shape_choice(&mut self) {
        if self.tool_preferences.shape != self.shape_kind {
            self.tool_preferences.shape = self.shape_kind;
            self.tool_preferences_changed = true;
        }
    }

    pub(super) fn remember_eraser_diameter(&mut self) {
        if self.tool_preferences.eraser_diameter != self.eraser_diameter {
            self.tool_preferences.eraser_diameter = self.eraser_diameter;
            self.tool_preferences_changed = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrow_style::{Endpoint, Pattern};

    fn document() -> Document {
        Document {
            image: std::sync::Arc::new(image::RgbaImage::new(500, 400)),
            annotations: vec![],
            geometry: Default::default(),
        }
    }

    #[test]
    fn custom_style_slots_persist_per_tool_without_text_or_absolute_geometry() {
        let mut editor = EditorState::default();
        let mut expected = Vec::new();
        for (tool, kind) in [
            (Tool::Arrow, AnnotationKind::Arrow),
            (Tool::Text, AnnotationKind::Text),
            (Tool::Callout, AnnotationKind::Callout),
            (Tool::Rectangle, AnnotationKind::Rectangle),
            (Tool::Pen, AnnotationKind::Freehand),
            (Tool::Highlighter, AnnotationKind::Highlighter),
            (Tool::Blur, AnnotationKind::Blur),
        ] {
            editor.activate_tool(tool);
            for slot in 0..4 {
                editor.color = [30 + slot as u8, 120, 210, 180];
                editor.stroke_width = 6. + slot as f32;
                editor.arrow.pattern = Pattern::DashDot;
                editor.arrow.start = Endpoint::Circle;
                editor.arrow.end = Endpoint::Diamond;
                editor.arrow.curve = Some([Point { x: 123., y: 456. }, Point { x: 234., y: 567. }]);
                editor.text = "document-only private text".into();
                editor.font_size = 28. + slot as f32;
                editor.text_style.italic = true;
                editor.text_style.alignment = crate::text_style::Alignment::Right;
                editor.callout_style = crate::callout_style::CalloutStyle::Note;
                editor.effect_size = 24 + slot as u32;
                editor.fill = true;
                editor.save_custom_quick_style(kind, slot);
            }
            expected.push((tool, kind, editor.custom_quick_styles(kind)));
        }
        let settings = crate::storage::Settings {
            editor_tools: editor.take_tool_preferences_change().unwrap(),
            ..Default::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert!(!json.contains("document-only private text"));
        let restored: crate::storage::Settings = serde_json::from_str(&json).unwrap();
        let mut reopened = EditorState::default();
        reopened.set_tool_preferences(restored.editor_tools);
        reopened.reset();
        let mut document = document();
        let mut history = EditHistory::new(document.edit_state());
        for (tool, kind, slots) in expected {
            reopened.activate_tool(tool);
            assert_eq!(reopened.custom_quick_styles(kind), slots);
            for (slot, appearance) in slots.into_iter().enumerate() {
                assert!(appearance.as_ref().unwrap().arrow.curve.is_none());
                reopened.apply_custom_quick_style(&mut document, &mut history, kind, slot);
                assert_eq!(Appearance::from_editor(&reopened), appearance.unwrap());
            }
        }
        assert!(!history.can_undo());
        let legacy: ToolPreferences = serde_json::from_str("{}").unwrap();
        assert!(legacy.custom_styles.is_empty());
    }

    #[test]
    fn applying_custom_arrow_style_preserves_curve_and_supports_undo_without_unlocking_artwork() {
        let mut editor = EditorState::default();
        editor.activate_tool(Tool::Arrow);
        editor.color = [18, 88, 190, 255];
        editor.stroke_width = 7.;
        editor.arrow.pattern = Pattern::Dotted;
        editor.arrow.end = Endpoint::Diamond;
        editor.save_custom_quick_style(AnnotationKind::Arrow, 0);
        let mut document = document();
        let mut arrow = editor.new_annotation(AnnotationKind::Arrow, Point { x: 30., y: 60. });
        arrow.end = Point { x: 300., y: 200. };
        arrow.color = [220, 50, 60, 255];
        arrow.stroke_width = 3.;
        arrow.arrow.pattern = Pattern::Solid;
        arrow.arrow.curve = Some([Point { x: 100., y: 20. }, Point { x: 200., y: 250. }]);
        document.annotations.push(arrow);
        let before = document.edit_state();
        let mut history = EditHistory::new(before.clone());
        editor.select(Some(0), &document);
        editor.apply_custom_quick_style(&mut document, &mut history, AnnotationKind::Arrow, 0);
        let after = &document.annotations[0];
        assert_eq!(after.color, [18, 88, 190, 255]);
        assert_eq!(after.arrow.pattern, Pattern::Dotted);
        assert_eq!(after.arrow.curve, before.annotations[0].arrow.curve);
        assert_eq!(
            (after.start, after.end),
            (before.annotations[0].start, before.annotations[0].end)
        );
        assert!(history.undo(&mut document));
        assert_eq!(document.edit_state(), before);
        document.annotations[0].layer.locked = true;
        let locked = document.edit_state();
        editor.select(Some(0), &document);
        editor.apply_custom_quick_style(&mut document, &mut history, AnnotationKind::Arrow, 0);
        assert_eq!(document.edit_state(), locked);
    }

    #[test]
    fn invalid_custom_slots_are_dropped_independently() {
        let mut editor = EditorState::default();
        editor.activate_tool(Tool::Arrow);
        editor.save_custom_quick_style(AnnotationKind::Arrow, 0);
        editor.save_custom_quick_style(AnnotationKind::Arrow, 1);
        let mut preferences = editor.take_tool_preferences_change().unwrap();
        preferences.custom_styles[0].slots[0]
            .as_mut()
            .unwrap()
            .stroke_width = -1.;
        preferences.custom_styles[0].slots[1]
            .as_mut()
            .unwrap()
            .arrow
            .curve = Some([Point { x: 1., y: 2. }, Point { x: 3., y: 4. }]);
        editor.set_tool_preferences(preferences);
        let slots = editor.custom_quick_styles(AnnotationKind::Arrow);
        assert!(slots[0].is_none());
        assert!(slots[1].as_ref().unwrap().arrow.curve.is_none());
        editor.clear_custom_quick_style(AnnotationKind::Arrow, 1);
        assert!(
            editor
                .take_tool_preferences_change()
                .unwrap()
                .custom_styles
                .is_empty()
        );
    }

    #[test]
    fn styles_and_colors_are_independent_and_survive_reselect_and_document_reset() {
        let mut editor = EditorState::default();
        let mut document = document();
        let mut history = EditHistory::new(document.edit_state());
        let tools = [
            Tool::Arrow,
            Tool::Rectangle,
            Tool::Ellipse,
            Tool::Shape(AnnotationKind::Triangle),
            Tool::Text,
            Tool::Callout,
            Tool::Step,
            Tool::Highlighter,
            Tool::Pen,
            Tool::Blur,
            Tool::Pixelate,
            Tool::Magnify,
            Tool::Spotlight,
            Tool::Simplify,
        ];
        let mut expected = Vec::new();
        for (index, tool) in tools.into_iter().enumerate() {
            editor.activate_tool(tool);
            editor.color = [20 + index as u8, 100, 190, 100 + index as u8];
            editor.stroke_width = 3. + index as f32;
            editor.fill = index % 2 == 0;
            editor.arrow.pattern = if index % 2 == 0 {
                Pattern::Dashed
            } else {
                Pattern::Dotted
            };
            editor.arrow.start = Endpoint::Circle;
            editor.arrow.end = Endpoint::None;
            editor.arrow.start_size = 5.;
            editor.font_size = 25. + index as f32;
            editor.text_style.monospace = index % 2 == 0;
            editor.text_style.italic = true;
            editor.text_style.alignment = crate::text_style::Alignment::Right;
            editor.callout_style = crate::callout_style::CalloutStyle::Note;
            editor.text_color = [200, 100 + index as u8, 40, 255];
            editor.effect_size = 8 + index as u32;
            editor.step_diameter = 40. + index as f32;
            editor.studio.zoom = 3.;
            editor.apply_inspector(&mut document, &mut history);
            expected.push(Appearance::from_editor(&editor));
        }
        assert!(document.annotations.is_empty());
        assert!(!history.can_undo());
        for reset in [false, true] {
            if reset {
                editor.reset();
            }
            for (tool, expected) in tools.into_iter().zip(&expected) {
                editor.activate_tool(tool);
                assert_eq!(&Appearance::from_editor(&editor), expected, "{tool:?}");
                editor.activate_tool(tool);
                assert_eq!(
                    &Appearance::from_editor(&editor),
                    expected,
                    "reselect {tool:?}"
                );
            }
        }
        assert!(editor.take_tool_preferences_change().is_some());
        assert!(editor.take_tool_preferences_change().is_none());
    }

    #[test]
    fn inspecting_existing_artwork_does_not_replace_the_next_drawn_arrow() {
        let mut editor = EditorState::default();
        editor.activate_tool(Tool::Arrow);
        editor.color = [10, 100, 200, 180];
        editor.stroke_width = 9.;
        editor.arrow.pattern = Pattern::LongDash;
        editor.arrow.end = Endpoint::None;
        editor.remember_appearance(AnnotationKind::Arrow);
        let chosen = Appearance::from_editor(&editor);
        let mut document = document();
        let mut old = editor.new_annotation(AnnotationKind::Arrow, Point { x: 20., y: 20. });
        old.end = Point { x: 80., y: 40. };
        old.color = [220, 80, 40, 255];
        old.stroke_width = 2.;
        old.arrow = Default::default();
        old.arrow.curve = Some([Point { x: 30., y: 60. }, Point { x: 50., y: 80. }]);
        document.annotations.push(old);
        let original = document.edit_state();
        editor.select(Some(0), &document);
        editor.clear_selection();
        assert_eq!(Appearance::from_editor(&editor), chosen);
        editor.select(Some(0), &document);
        editor.begin_tool_gesture(Point { x: 300., y: 200. }, 1., &mut document, false, true);
        assert_eq!(document.annotations[0], original.annotations[0]);
        assert_eq!(document.geometry, original.geometry);
        let new = &document.annotations[1];
        assert_eq!(new.color, chosen.color);
        assert_eq!(new.stroke_width, chosen.stroke_width);
        assert_eq!(new.arrow.pattern, Pattern::LongDash);
        assert_eq!(new.arrow.end, Endpoint::None);
        assert_eq!(new.arrow.curve, None);
        assert_eq!(new.start, Point { x: 300., y: 200. });
    }

    #[test]
    fn preferences_round_trip_with_settings_without_text_or_document_geometry() {
        let mut editor = EditorState::default();
        editor.activate_tool(Tool::Callout);
        editor.color = [12, 80, 200, 220];
        editor.font_size = 43.;
        editor.text_color = [25, 30, 40, 255];
        editor.text_style.underline = true;
        editor.callout_style = crate::callout_style::CalloutStyle::ThoughtCloud;
        editor.text = "Private document text must stay out of settings".into();
        editor.arrow.curve = Some([Point { x: 123., y: 234. }, Point { x: 345., y: 456. }]);
        editor.remember_appearance(AnnotationKind::Callout);
        editor.shape_kind = AnnotationKind::Star;
        editor.remember_shape_choice();
        let expected = Appearance::from_editor(&editor);
        let settings = crate::storage::Settings {
            editor_tools: editor.take_tool_preferences_change().unwrap(),
            ..Default::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert!(!json.contains("Private document text"));
        assert!(!json.contains("\"curve\""));
        let loaded: crate::storage::Settings = serde_json::from_str(&json).unwrap();
        let mut reopened = EditorState::default();
        reopened.set_tool_preferences(loaded.editor_tools);
        reopened.activate_tool(Tool::Callout);
        assert_eq!(Appearance::from_editor(&reopened), expected);
        assert_eq!(reopened.text, "Callout");
        assert_eq!(reopened.shape_kind, AnnotationKind::Star);
        assert!(reopened.take_tool_preferences_change().is_none());
        let legacy: crate::storage::Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.editor_tools, ToolPreferences::default());
    }

    #[test]
    fn invalid_saved_values_are_rejected_without_dropping_valid_tool_styles() {
        let mut preferences = ToolPreferences {
            shape: AnnotationKind::Text,
            eraser_diameter: f32::INFINITY,
            fill: FillPreferences {
                tolerance: 255,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut valid = Appearance::for_kind(AnnotationKind::Arrow);
        valid.color = [0, 80, 180, 255];
        valid.arrow.curve = Some([Point { x: 1., y: 2. }, Point { x: 3., y: 4. }]);
        preferences.appearances.push(RememberedAppearance {
            kind: AnnotationKind::Arrow,
            appearance: valid.clone(),
        });
        for (kind, value) in [
            (AnnotationKind::Text, -1.),
            (AnnotationKind::Rectangle, f32::NAN),
        ] {
            preferences.appearances.push(RememberedAppearance {
                kind,
                appearance: Appearance {
                    stroke_width: value,
                    ..Default::default()
                },
            });
        }
        let mut editor = EditorState::default();
        editor.set_tool_preferences(preferences);
        assert_eq!(editor.tool_preferences.appearances.len(), 1);
        assert_eq!(editor.shape_kind, AnnotationKind::Rectangle);
        assert_eq!(editor.eraser_diameter, 32.);
        assert_eq!(editor.tool_preferences.fill.tolerance, 100);
        editor.activate_tool(Tool::Arrow);
        assert_eq!(editor.color, valid.color);
        assert_eq!(editor.arrow.curve, None);
        editor.activate_tool(Tool::Text);
        assert_eq!(editor.stroke_width, 4.);
    }
}
