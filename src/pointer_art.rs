//! Small vector glyphs retain the familiar meaning of each native cursor.
use eframe::egui::{CursorIcon as C, Vec2};

const CAT: &str = r##"<path d="M1 7V1L5 4Q7 3 9 4L13 1V7Q15 13 7 14Q0 13 1 7Z" fill="#fff8e8" stroke="#fff" stroke-width="3" stroke-linejoin="round"/><path d="M1 7V1L5 4Q7 3 9 4L13 1V7Q15 13 7 14Q0 13 1 7Z" fill="#fff8e8" stroke="#39342f" stroke-width="1.1" stroke-linejoin="round"/><path d="M1.5 7V2L5 4.5Q7 9 2 10Z" fill="#e6a05a"/><path d="M9 4.5L12.5 2V7Q10 9 9 4.5Z" fill="#45464a"/><path d="M4 8V9M10 8V9" stroke="#337f83" stroke-width="1.5" stroke-linecap="round"/><path d="M6 10H8L7 11Z" fill="#a56d64"/>"##;

pub(super) fn badge() -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="-1 -1 16 16">{CAT}</svg>"#
    )
}

pub(super) fn hotspot(cursor: C) -> Vec2 {
    match cursor {
        C::ContextMenu | C::Help | C::Progress | C::Alias | C::Copy => Vec2::splat(2.),
        C::PointingHand => Vec2::new(10., 2.),
        _ => Vec2::splat(10.),
    }
}

pub(super) fn svg(cursor: C) -> String {
    let arrow = r#"<path d="M2 2V19L7 14L10 20L13 18L10 12H17Z"/>"#;
    let both = r#"<path d="M2 10H18M6 6L2 10L6 14M14 6L18 10L14 14"/>"#;
    let one = r#"<path d="M2 10H18M14 6L18 10L14 14"/>"#;
    let rotated =
        |path: &str, degrees: i32| format!(r#"<g transform="rotate({degrees} 10 10)">{path}</g>"#);
    let glyph: String = match cursor {
        C::Default | C::None | C::Grab | C::Grabbing => unreachable!("dedicated cursor artwork"),
        C::Crosshair => r#"<path d="M10 1V6M10 14V19M1 10H6M14 10H19"/><circle cx="10" cy="10" r=".65"/>"#.into(),
        C::Cell => r#"<path d="M7 2H13V7H18V13H13V18H7V13H2V7H7Z"/>"#.into(),
        C::Text => r#"<path d="M10 2V18M6 2H14M6 18H14"/>"#.into(),
        C::VerticalText => r#"<path d="M2 10H18M2 6V14M18 6V14"/>"#.into(),
        C::PointingHand => r#"<path d="M7 12V5Q7 1 10 2Q12 2 12 5V10L16 9Q19 9 19 12V17Q19 21 14 21H10Q8 21 6 18L3 14Q2 11 5 11Z"/>"#.into(),
        C::ContextMenu => format!(r#"{arrow}<path d="M16 3H24M18 7H24M20 11H24"/>"#),
        C::Help => format!(r#"{arrow}<path d="M19 5Q19 1 23 2Q27 4 23 7V9M23 12V12.2"/>"#),
        C::Progress => format!(r#"{arrow}<circle cx="21" cy="8" r="5"/><path d="M21 4V8L24 10"/>"#),
        C::Wait => r#"<path d="M4 2H16M4 18H16M5 2Q5 7 10 10Q15 13 15 18M15 2Q15 7 10 10Q5 13 5 18M7 15H13"/>"#.into(),
        C::Alias => format!(r#"{arrow}<path d="M18 14V7H25M22 4L25 7L22 10"/>"#),
        C::Copy => format!(r#"{arrow}<path d="M17 3H23V11H17ZM20 6H26V14H23"/>"#),
        C::Move | C::AllScroll => r#"<path d="M1 10H19M10 1V19M4 7L1 10L4 13M16 7L19 10L16 13M7 4L10 1L13 4M7 16L10 19L13 16"/>"#.into(),
        C::NoDrop | C::NotAllowed => r#"<circle cx="10" cy="10" r="8"/><path d="M4 4L16 16"/>"#.into(),
        C::ResizeHorizontal => both.into(),
        C::ResizeVertical => rotated(both, 90),
        C::ResizeNeSw => rotated(both, -45),
        C::ResizeNwSe => rotated(both, 45),
        C::ResizeEast => one.into(),
        C::ResizeSouthEast => rotated(one, 45),
        C::ResizeSouth => rotated(one, 90),
        C::ResizeSouthWest => rotated(one, 135),
        C::ResizeWest => rotated(one, 180),
        C::ResizeNorthWest => rotated(one, 225),
        C::ResizeNorth => rotated(one, 270),
        C::ResizeNorthEast => rotated(one, 315),
        C::ResizeColumn => format!(r#"{both}<path d="M8 2V18M12 2V18"/>"#),
        C::ResizeRow => rotated(&format!(r#"{both}<path d="M8 2V18M12 2V18"/>"#), 90),
        C::ZoomIn | C::ZoomOut => format!(r#"<circle cx="10" cy="10" r="7"/><path d="M15 15L21 21M6 10H14{}"/>"#, if cursor == C::ZoomIn { "M10 6V14" } else { "" }),
    };
    // Three strokes give contrast on both dark artwork and checkerboard;
    // the warm center and tiny cat distinguish the whole Calico family.
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="28" height="28" viewBox="0 0 28 28"><g fill="none" stroke-linecap="round" stroke-linejoin="round"><g stroke="#fff" stroke-width="4.5">{glyph}</g><g stroke="#30343b" stroke-width="2.3">{glyph}</g><g stroke="#edb577" stroke-width=".8">{glyph}</g></g><g transform="translate(18 18) scale(.62)">{CAT}</g></svg>"##
    )
}
