//! Shared virtual layout for dated gallery rows and marquee hit testing.
use super::*;

pub(super) enum RowContent {
    Date(NaiveDate, std::ops::Range<usize>),
    Projects(std::ops::Range<usize>),
}
pub(super) struct Row {
    pub rect: Rect,
    pub content: RowContent,
}
pub(super) struct Gallery {
    pub rows: Vec<Row>,
    pub cards: Vec<Rect>,
    pub day_checkboxes: Vec<Rect>,
    pub height: f32,
}
impl Gallery {
    pub fn new(
        entries: &[LibraryEntry],
        order: &[usize],
        columns: usize,
        card: Vec2,
        gap: Vec2,
    ) -> Self {
        let mut gallery = Self {
            rows: Vec::new(),
            cards: vec![Rect::NOTHING; order.len()],
            day_checkboxes: Vec::new(),
            height: 0.,
        };
        let width = columns as f32 * card.x + (columns - 1) as f32 * gap.x;
        let mut first = 0;
        while first < order.len() {
            let date = entries[order[first]].date;
            let end = first
                + order[first..]
                    .iter()
                    .take_while(|&&i| entries[i].date == date)
                    .count();
            gallery.rows.push(Row {
                rect: Rect::from_min_size(egui::pos2(0., gallery.height), egui::vec2(width, 30.)),
                content: RowContent::Date(date, first..end),
            });
            gallery.day_checkboxes.push(Rect::from_min_size(
                egui::pos2(0., gallery.height),
                egui::vec2(28., 30.),
            ));
            gallery.height += 30. + gap.y;
            for start in (first..end).step_by(columns) {
                let stop = (start + columns).min(end);
                for index in start..stop {
                    gallery.cards[index] = Rect::from_min_size(
                        egui::pos2((index - start) as f32 * (card.x + gap.x), gallery.height),
                        card,
                    );
                }
                gallery.rows.push(Row {
                    rect: Rect::from_min_size(
                        egui::pos2(0., gallery.height),
                        egui::vec2(width, card.y),
                    ),
                    content: RowContent::Projects(start..stop),
                });
                gallery.height += card.y + gap.y;
            }
            gallery.height += 6.;
            first = end;
        }
        gallery.height = (gallery.height - 6. - gap.y).max(0.);
        gallery
    }
}
