//! Glyph quads keep their letter shapes while their baselines follow arc length.
use super::*;
pub(super) fn raster(
    fonts: &mut egui::epaint::text::Fonts,
    annotation: &Annotation,
) -> Result<TextRaster, String> {
    let path = annotation
        .text_style
        .path
        .as_ref()
        .ok_or("Missing text path.")?;
    path.validate()?;
    let geometry = path.geometry();
    let mut straight = annotation.clone();
    straight.text = straight.text.replace(['\n', '\r'], " ");
    straight.text_style.path = None;
    straight.text_style.alignment = crate::text_style::Alignment::Left;
    let (galley, _) = text_layout(fonts, &straight)?;
    if fonts.font_atlas_fill_ratio() >= 1. {
        return Err(
            "This path contains too many large letters. Use a smaller font or shorter text.".into(),
        );
    }
    let count: usize = galley.rows.iter().map(|r| r.glyphs.len()).sum();
    let length = galley.size().x + count.saturating_sub(1) as f32 * f32::from(path.spacing);
    let scale = if path.fit {
        (geometry.length / length.max(1.)).min(1.)
    } else {
        1.
    };
    let start = match annotation.text_style.alignment {
        crate::text_style::Alignment::Left => 0.,
        crate::text_style::Alignment::Center => (geometry.length - length * scale) * 0.5,
        crate::text_style::Alignment::Right => geometry.length - length * scale,
    };
    let mut mesh = egui::epaint::Mesh::default();
    let mut ordinal = 0;
    for row in &galley.rows {
        for glyph in &row.glyphs {
            let center = glyph.pos.x + glyph.advance_width * 0.5;
            let distance = start + (center + ordinal as f32 * f32::from(path.spacing)) * scale;
            ordinal += 1;
            let (position, tangent) = geometry.at(distance);
            // In image coordinates, this normal points below the path.
            let normal = egui::vec2(-tangent.y, tangent.x);
            let ascent = glyph.font_ascent * scale;
            let descent = (glyph.font_height - glyph.font_ascent) * scale;
            let baseline = match path.placement {
                crate::text_path::Placement::Over => -descent - f32::from(path.offset),
                crate::text_path::Placement::On => {
                    (ascent - descent) * 0.5 + f32::from(path.offset)
                }
                crate::text_path::Placement::Below => ascent + f32::from(path.offset),
            };
            let transform = |p: Pos2| {
                position
                    + tangent * ((p.x - center) * scale)
                    + normal * ((p.y - glyph.pos.y) * scale + baseline)
            };
            let i = glyph.first_vertex as usize;
            if glyph.uv_rect.size.x > 0.
                && glyph.uv_rect.size.y > 0.
                && i + 4 <= row.visuals.mesh.vertices.len()
            {
                let base = mesh.vertices.len() as u32;
                for vertex in &row.visuals.mesh.vertices[i..i + 4] {
                    let mut v = *vertex;
                    v.pos = transform(v.pos);
                    mesh.vertices.push(v);
                }
                mesh.indices.extend_from_slice(&[
                    base,
                    base + 1,
                    base + 2,
                    base + 2,
                    base + 1,
                    base + 3,
                ]);
            }
            if annotation.text_style.underline && glyph.advance_width > 0. {
                let y = glyph.pos.y + glyph.font_height / 12.;
                let width = (annotation.font_size / 18.).max(1.);
                let base = mesh.vertices.len() as u32;
                for p in [
                    egui::pos2(glyph.pos.x, y),
                    egui::pos2(
                        glyph.pos.x + glyph.advance_width + f32::from(path.spacing),
                        y,
                    ),
                    egui::pos2(glyph.pos.x, y + width),
                    egui::pos2(
                        glyph.pos.x + glyph.advance_width + f32::from(path.spacing),
                        y + width,
                    ),
                ] {
                    mesh.vertices.push(egui::epaint::Vertex {
                        pos: transform(p),
                        uv: Pos2::ZERO,
                        color: Color32::WHITE,
                    });
                }
                mesh.indices.extend_from_slice(&[
                    base,
                    base + 1,
                    base + 2,
                    base + 2,
                    base + 1,
                    base + 3,
                ]);
            }
        }
    }
    let bounds = mesh.calc_bounds();
    if !bounds.is_positive() {
        return Ok(TextRaster {
            bounds: Rect::from_min_size(Pos2::ZERO, Vec2::splat(1.)),
            offset: Vec2::ZERO,
            size: [0, 0],
            alpha: vec![],
        });
    }
    let offset = bounds.min.floor().to_vec2();
    let extent = bounds.max.ceil().to_vec2() - offset;
    let size = [extent.x as usize, extent.y as usize];
    if size[0] > 32_768
        || size[1] > 32_768
        || size[0].saturating_mul(size[1]) > MAX_TEXT_RASTER_PIXELS
    {
        return Err(
            "This text path is too large to render. Reduce its dimensions or font size.".into(),
        );
    }
    let mut alpha = vec![0_u8; size[0] * size[1]];
    let atlas = fonts.texture_atlas().image();
    for triangle in mesh.indices.chunks_exact(3) {
        let [a, b, c] = [
            mesh.vertices[triangle[0] as usize],
            mesh.vertices[triangle[1] as usize],
            mesh.vertices[triangle[2] as usize],
        ];
        let [pa, pb, pc] = [a.pos - offset, b.pos - offset, c.pos - offset];
        let bounds = Rect::from_points(&[pa, pb, pc]);
        let ab = pb - pa;
        let ac = pc - pa;
        let cross = |u: Vec2, v: Vec2| u.x * v.y - u.y * v.x;
        let det = cross(ab, ac);
        if det.abs() < 0.00001 {
            continue;
        }
        for y in (bounds.min.y.floor().max(0.) as usize)
            ..(bounds.max.y.ceil().max(0.) as usize).min(size[1])
        {
            for x in (bounds.min.x.floor().max(0.) as usize)
                ..(bounds.max.x.ceil().max(0.) as usize).min(size[0])
            {
                let p = egui::pos2(x as f32 + 0.5, y as f32 + 0.5) - pa;
                let u = cross(p, ac) / det;
                let v = cross(ab, p) / det;
                if u < -0.0001 || v < -0.0001 || u + v > 1.0001 {
                    continue;
                }
                let sample = a.uv + (b.uv - a.uv) * u + (c.uv - a.uv) * v - Vec2::splat(0.5);
                let coverage = if a.uv == Pos2::ZERO && b.uv == Pos2::ZERO && c.uv == Pos2::ZERO {
                    255
                } else {
                    (sample_alpha(atlas, sample) * 255.).round() as u8
                };
                alpha[y * size[0] + x] = alpha[y * size[0] + x].max(coverage);
            }
        }
    }
    Ok(TextRaster {
        bounds,
        offset,
        size,
        alpha,
    })
}
pub(super) fn paint_path(overlay: &mut Pixmap, annotation: &Annotation, transform: Transform) {
    let Some(path) = annotation.text_style.path.as_ref().filter(|p| p.visible) else {
        return;
    };
    let geometry = path.geometry();
    let mut builder = PathBuilder::new();
    for (i, p) in geometry.points.iter().enumerate() {
        if i == 0 {
            builder.move_to(p.x + annotation.start.x, p.y + annotation.start.y);
        } else {
            builder.line_to(p.x + annotation.start.x, p.y + annotation.start.y);
        }
    }
    if let Some(line) = builder.finish() {
        let [r, g, b, a] = path.color;
        let mut paint = Paint::default();
        paint.set_color_rgba8(r, g, b, a);
        overlay.stroke_path(
            &line,
            &paint,
            &tiny_skia::Stroke {
                width: f32::from(path.stroke),
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                ..Default::default()
            },
            transform,
            None,
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::text_path::{Placement, Shape as PathShape, TextPath};
    fn annotation() -> Annotation {
        serde_json::from_value(serde_json::json!({"kind":"Text","start":{"x":100.,"y":100.},"end":{"x":100.,"y":100.},"color":[20,30,50,255],"stroke_width":2.,"fill":false,"text":"Hello path • MyImage","font_size":32.})).unwrap()
    }
    #[test]
    fn every_path_renders_and_survives_project_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        for shape in PathShape::ALL {
            let mut a = annotation();
            a.text_style.path = Some(TextPath {
                shape,
                width: 260,
                height: 260,
                visible: true,
                ..Default::default()
            });
            a.text_style.italic = true;
            a.text_style.underline = true;
            let doc = Document {
                image: Arc::new(RgbaImage::from_pixel(600, 600, image::Rgba([255; 4]))),
                annotations: vec![a],
                geometry: Default::default(),
            };
            let image = flatten(&doc).unwrap();
            assert!(
                image.pixels().filter(|p| p[0] < 100).count() > 400,
                "{}",
                shape.name()
            );
            let path = dir.path().join("path.myimage");
            crate::document::save_project(&doc, &path).unwrap();
            let loaded = crate::document::load_project(&path).unwrap();
            assert_eq!(doc.annotations, loaded.annotations);
            assert_eq!(image, flatten(&loaded).unwrap());
            if let Ok(directory) = std::env::var("MYIMAGE_PATH_REVIEW") {
                crate::document::save_project(
                    &doc,
                    &std::path::Path::new(&directory).join(format!("{}.myimage", shape.name())),
                )
                .unwrap();
                image
                    .save(std::path::Path::new(&directory).join(format!("{}.png", shape.name())))
                    .unwrap();
            }
        }
    }
    #[test]
    fn path_side_distance_and_visibility_do_not_change_text_content() {
        let mut a = annotation();
        a.text_style.path = Some(TextPath::default());
        let over = text_raster(&a).unwrap();
        a.text_style.path.as_mut().unwrap().placement = Placement::Below;
        let below = text_raster(&a).unwrap();
        assert!(below.bounds.top() > over.bounds.bottom());
        a.text_style.path.as_mut().unwrap().offset += 20;
        assert!((text_raster(&a).unwrap().bounds.top() - below.bounds.top() - 20.).abs() < 0.1);
        let hidden = text_raster(&a).unwrap();
        a.text_style.path.as_mut().unwrap().visible = true;
        assert_eq!(hidden.alpha, text_raster(&a).unwrap().alpha);
        assert_eq!(a.text, "Hello path • MyImage");
    }
    #[test]
    fn moving_path_text_changes_anchor_only_and_hit_bounds_follow_it() {
        let mut a = annotation();
        a.text_style.path = Some(TextPath {
            shape: PathShape::Circle,
            width: 200,
            ..Default::default()
        });
        let before = annotation_bounds(&a);
        let raster = text_raster(&a).unwrap();
        a.start.x += 45.;
        a.start.y -= 12.;
        assert_eq!(
            annotation_bounds(&a),
            before.translate(egui::vec2(45., -12.))
        );
        assert_eq!(text_raster(&a).unwrap().alpha, raster.alpha);
    }
}
