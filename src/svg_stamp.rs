//! Static, self-contained SVG artwork. No browser, scripts, network, or file resolvers.
use image::RgbaImage;
use std::sync::{Arc, OnceLock};

pub const MAX_BYTES: usize = 256 * 1024;
pub const STARTER: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" viewBox="0 0 512 512">
  <circle cx="256" cy="256" r="220" fill="#e0f5f0" stroke="#008992" stroke-width="20"/>
  <path d="M140 260 L220 340 L380 165" fill="none" stroke="#008992" stroke-width="32" stroke-linecap="round" stroke-linejoin="round"/>
</svg>"##;

pub fn prompt(description: &str) -> String {
    format!(
        "Design an original reusable image annotation stamp: {description}\n\nReturn only a complete static SVG, with xmlns=\"http://www.w3.org/2000/svg\", width=\"512\", height=\"512\" and viewBox=\"0 0 512 512\". Keep the background transparent and leave a small margin. Make it clear at small sizes. Use simple paths, circles, rectangles, polygons and gradients with explicit fill/stroke colors. Text may use sans-serif, or convert lettering to paths. No scripts, animation, foreignObject, images, external links, embedded files, filters, patterns, CSS stylesheets or use elements. Use presentation attributes. Keep the SVG below 256 KB."
    )
}

pub fn clean(input: &str) -> String {
    let text = input.trim();
    if text.starts_with("```") && text.ends_with("```") {
        text.split_once('\n')
            .map(|(_, rest)| rest[..rest.len() - 3].trim().to_string())
            .unwrap_or_default()
    } else {
        text.to_string()
    }
}

pub fn tree(svg: &str) -> Result<resvg::usvg::Tree, String> {
    if svg.len() > MAX_BYTES {
        return Err("SVG must be smaller than 256 KB.".into());
    }
    let doc = roxmltree::Document::parse(svg).map_err(|e| format!("Invalid SVG: {e}"))?;
    if doc.root_element().tag_name().name() != "svg" {
        return Err("Paste a complete <svg> document.".into());
    }
    if doc.descendants().count() > 4096 || doc.descendants().any(|n| n.ancestors().count() > 64) {
        return Err("SVG is too complex. Ask for simpler paths and shapes.".into());
    }
    const TAGS: &[&str] = &[
        "svg",
        "g",
        "defs",
        "title",
        "desc",
        "path",
        "rect",
        "circle",
        "ellipse",
        "line",
        "polyline",
        "polygon",
        "text",
        "tspan",
        "linearGradient",
        "radialGradient",
        "stop",
        "clipPath",
        "mask",
    ];
    for node in doc.descendants().filter(|n| n.is_element()) {
        if !TAGS.contains(&node.tag_name().name())
            || node
                .tag_name()
                .namespace()
                .is_some_and(|n| n != "http://www.w3.org/2000/svg")
        {
            return Err(format!(
                "Unsupported SVG element: {}. Use static shapes and paths.",
                node.tag_name().name()
            ));
        }
        for a in node.attributes() {
            let name = a.name().to_ascii_lowercase();
            let value = a.value().trim();
            if name.starts_with("on")
                || matches!(name.as_str(), "href" | "src" | "style" | "filter")
                || value.contains('@')
                || value.to_ascii_lowercase().contains("url(")
                    && !(value.starts_with("url(#")
                        && value.ends_with(')')
                        && !value[5..value.len() - 1].contains(['(', ')', ' ', '\"', '\'']))
            {
                return Err("SVG must use static attributes and local fragment references, without scripts, stylesheets or linked content.".into());
            }
        }
    }
    static FONTS: OnceLock<Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();
    let fonts = FONTS.get_or_init(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        for font in eframe::egui::FontDefinitions::default().font_data.values() {
            db.load_font_data(font.font.to_vec());
        }
        db.set_sans_serif_family("Ubuntu");
        db.set_serif_family("Ubuntu");
        db.set_monospace_family("Hack");
        Arc::new(db)
    });
    let options = resvg::usvg::Options {
        fontdb: fonts.clone(),
        font_family: "Ubuntu".into(),
        image_href_resolver: resvg::usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..Default::default()
    };
    let tree =
        resvg::usvg::Tree::from_str(svg, &options).map_err(|e| format!("Cannot read SVG: {e}"))?;
    if tree.size().width().max(tree.size().height()) > 8192. {
        return Err("Use an SVG canvas no larger than 8192 px per side.".into());
    }
    Ok(tree)
}

pub fn raster(svg: &str, max_side: u32) -> Result<RgbaImage, String> {
    let tree = tree(svg)?;
    let size = tree.size();
    let scale = max_side as f32 / size.width().max(size.height());
    let (w, h) = (
        (size.width() * scale).round().max(1.) as u32,
        (size.height() * scale).round().max(1.) as u32,
    );
    let mut pix = resvg::tiny_skia::Pixmap::new(w, h).ok_or("Cannot allocate SVG preview.")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pix.as_mut(),
    );
    let mut image = RgbaImage::new(w, h);
    for (dest, p) in image.pixels_mut().zip(pix.pixels()) {
        let c = p.demultiply();
        *dest = image::Rgba([c.red(), c.green(), c.blue(), c.alpha()]);
    }
    Ok(image)
}
