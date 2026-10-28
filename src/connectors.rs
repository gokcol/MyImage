//! Stable object identities keep connections independent of layer order.
use crate::document::{Annotation, AnnotationKind, Document, Point};
use eframe::egui::{Pos2, Rect};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Connection {
    pub start: Option<u64>,
    pub end: Option<u64>,
}
fn point(p: Point) -> Pos2 {
    Pos2::new(p.x, p.y)
}
fn value(p: Pos2) -> Point {
    Point { x: p.x, y: p.y }
}
pub fn fresh(used: &BTreeSet<u64>) -> u64 {
    (1..).find(|i| !used.contains(i)).unwrap()
}
pub fn ensure_id(document: &mut Document, index: usize) -> u64 {
    if let Some(id) = document.annotations[index].layer.object_id {
        return id;
    }
    let used = document
        .annotations
        .iter()
        .filter_map(|a| a.layer.object_id)
        .collect();
    let id = fresh(&used);
    document.annotations[index].layer.object_id = Some(id);
    id
}
fn edge(bounds: Rect, toward: Pos2, ellipse: bool) -> Pos2 {
    let center = bounds.center();
    let delta = toward - center;
    let half = bounds.size() * 0.5;
    if delta.length_sq() < 0.001 {
        return bounds.right_center();
    }
    let ratio = if ellipse {
        ((delta.x / half.x.max(0.5)).powi(2) + (delta.y / half.y.max(0.5)).powi(2)).sqrt()
    } else {
        (delta.x.abs() / half.x.max(0.5)).max(delta.y.abs() / half.y.max(0.5))
    };
    center + delta / ratio.max(0.001)
}
pub fn sync(document: &mut Document) -> bool {
    if !document
        .annotations
        .iter()
        .any(|a| a.arrow.connection.is_some())
    {
        return false;
    }
    let targets: BTreeMap<_, _> = document
        .annotations
        .iter()
        .filter(|a| a.kind != AnnotationKind::Arrow && !a.kind.is_area_edit())
        .filter_map(|a| {
            a.layer.object_id.map(|id| {
                (
                    id,
                    (
                        crate::render::annotation_bounds(a),
                        matches!(
                            a.kind,
                            AnnotationKind::Ellipse | AnnotationKind::Step | AnnotationKind::Sphere
                        ),
                    ),
                )
            })
        })
        .collect();
    let mut changed = false;
    for a in &mut document.annotations {
        let Some(mut connection) = a.arrow.connection.clone() else {
            continue;
        };
        let before = a.clone();
        let start = connection.start.and_then(|id| targets.get(&id));
        let end = connection.end.and_then(|id| targets.get(&id));
        if start.is_none() {
            connection.start = None;
        }
        if end.is_none() {
            connection.end = None;
        }
        let old_start = point(a.start);
        let old_end = point(a.end);
        if let Some(&(bounds, ellipse)) = start {
            a.start = value(edge(bounds, end.map_or(old_end, |t| t.0.center()), ellipse));
        }
        if let Some(&(bounds, ellipse)) = end {
            a.end = value(edge(
                bounds,
                start.map_or(old_start, |t| t.0.center()),
                ellipse,
            ));
        }
        if let Some(c) = &mut a.arrow.curve {
            c[0] = value(point(c[0]) + (point(a.start) - old_start));
            c[1] = value(point(c[1]) + (point(a.end) - old_end));
        }
        a.arrow.connection =
            (connection.start.is_some() || connection.end.is_some()).then_some(connection);
        changed |= *a != before;
    }
    changed
}
pub fn resolved(document: &Document) -> std::borrow::Cow<'_, Document> {
    if !document
        .annotations
        .iter()
        .any(|a| a.arrow.connection.is_some())
    {
        return std::borrow::Cow::Borrowed(document);
    }
    let mut result = document.clone();
    sync(&mut result);
    std::borrow::Cow::Owned(result)
}
/// Copying a set connects its new members to each other. References outside the
/// copied set become free endpoints, never links into an unrelated project.
pub fn remap(copies: &mut [Annotation], destination: &[Annotation]) {
    let mut used: BTreeSet<_> = destination
        .iter()
        .filter_map(|a| a.layer.object_id)
        .collect();
    let mut ids = BTreeMap::new();
    for a in copies.iter_mut() {
        if let Some(old) = a.layer.object_id {
            let id = fresh(&used);
            used.insert(id);
            ids.insert(old, id);
            a.layer.object_id = Some(id);
        }
    }
    for a in copies {
        if let Some(c) = &mut a.arrow.connection {
            c.start = c.start.and_then(|id| ids.get(&id).copied());
            c.end = c.end.and_then(|id| ids.get(&id).copied());
            if c.start.is_none() && c.end.is_none() {
                a.arrow.connection = None;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn object(kind: &str, start: [f32; 2], end: [f32; 2]) -> Annotation {
        serde_json::from_value(serde_json::json!({"kind":kind,"start":{"x":start[0],"y":start[1]},"end":{"x":end[0],"y":end[1]},"color":[20,30,50,255],"stroke_width":3.,"fill":true,"text":"","font_size":24.})).unwrap()
    }
    fn document() -> Document {
        let mut d = Document {
            image: std::sync::Arc::new(image::RgbaImage::from_pixel(
                800,
                600,
                image::Rgba([255; 4]),
            )),
            annotations: vec![
                object("Rectangle", [50., 100.], [150., 200.]),
                object("Ellipse", [400., 100.], [500., 200.]),
                object("Arrow", [150., 150.], [400., 150.]),
            ],
            geometry: Default::default(),
        };
        let start = ensure_id(&mut d, 0);
        let end = ensure_id(&mut d, 1);
        d.annotations[2].arrow.connection = Some(Connection {
            start: Some(start),
            end: Some(end),
        });
        sync(&mut d);
        d
    }
    #[test]
    fn links_follow_move_resize_and_layer_order_with_one_undo() {
        let mut d = document();
        let original = d.edit_state();
        let mut history = crate::document::EditHistory::new(original.clone());
        d.annotations[0].start.x += 50.;
        d.annotations[0].end.x += 50.;
        d.annotations[1].end.y += 100.;
        d.annotations.swap(0, 1);
        assert!(sync(&mut d));
        assert_eq!(d.annotations[2].start.x, 201.5);
        assert_ne!(d.annotations[2].end.y, 150.);
        let moved = d.edit_state();
        history.commit(original.clone(), &d);
        assert!(history.undo(&mut d));
        assert_eq!(d.edit_state(), original);
        assert!(history.redo(&mut d));
        assert_eq!(d.edit_state(), moved);
    }
    #[test]
    fn duplicates_connect_to_their_own_copies_and_missing_targets_detach() {
        let mut d = document();
        let mut copies = d.annotations.clone();
        remap(&mut copies, &d.annotations);
        assert_ne!(copies[0].layer.object_id, d.annotations[0].layer.object_id);
        assert_eq!(
            copies[2].arrow.connection.as_ref().unwrap().start,
            copies[0].layer.object_id
        );
        let mut arrow = vec![d.annotations[2].clone()];
        remap(&mut arrow, &d.annotations);
        assert!(arrow[0].arrow.connection.is_none());
        let old = d.annotations[2].start;
        d.annotations.remove(0);
        sync(&mut d);
        assert_eq!(d.annotations[1].start, old);
        assert!(
            d.annotations[1]
                .arrow
                .connection
                .as_ref()
                .unwrap()
                .start
                .is_none()
        );
    }
    #[test]
    fn curves_and_elbows_export_and_reopen_with_attached_endpoints() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("connections.myimage");
        for elbow in [false, true] {
            let mut d = document();
            d.annotations[2].arrow.elbow = elbow;
            d.annotations[2].arrow.curve =
                Some([Point { x: 200., y: 40. }, Point { x: 350., y: 40. }]);
            let before = d.annotations[2].arrow.curve.unwrap();
            d.annotations[0].start.y += 30.;
            d.annotations[0].end.y += 30.;
            assert!(sync(&mut d));
            assert_ne!(d.annotations[2].arrow.curve.unwrap(), before);
            let pixels = crate::render::flatten(&d).unwrap();
            crate::document::save_project(&d, &path).unwrap();
            let reopened = crate::document::load_project(&path).unwrap();
            assert_eq!(d.edit_state(), reopened.edit_state());
            assert_eq!(pixels, crate::render::flatten(&reopened).unwrap());
        }
    }
}
