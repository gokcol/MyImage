//! A short, input-transparent mascot performance. Never enters the document renderer.
use eframe::egui::{self, Color32, Pos2, Stroke, Vec2};
use std::{
    f32::consts::{PI, TAU},
    time::Duration,
};

#[derive(Default)]
pub struct CalicoPlay {
    started: Option<f64>,
    act: usize,
}

impl CalicoPlay {
    pub fn start(&mut self, now: f64) {
        // Choose a different act each time, without adding a random/runtime dependency.
        self.act = (self.act + 1 + (now.to_bits() as usize % 4)) % 5;
        self.started = Some(now);
    }

    pub fn stop(&mut self) {
        self.started = None;
    }

    pub fn cancel_on_escape(&mut self, ctx: &egui::Context) {
        if self.started.is_some()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.stop();
        }
    }

    fn progress(&mut self, now: f64) -> Option<f32> {
        let elapsed = now - self.started?;
        if !(0.0..5.0).contains(&elapsed) {
            self.stop();
            return None;
        }
        Some(elapsed as f32 / 5.)
    }

    pub fn paint(&mut self, ctx: &egui::Context) {
        let Some(t) = self.progress(ctx.input(|i| i.time)) else {
            return;
        };
        let bounds = ctx.content_rect().shrink(12.);
        let p = ctx
            .layer_painter(egui::LayerId::new(
                egui::Order::Tooltip,
                egui::Id::new("calico-play"),
            ))
            .with_clip_rect(bounds);
        let alpha = (t / 0.055).min((1. - t) / 0.08).clamp(0., 1.);
        let color = |c: Color32| c.gamma_multiply(alpha);
        let safe = bounds.shrink(60.);
        let phase = t * TAU;
        let (x, y) = match self.act {
            0 => (t, 0.92 - 0.05 * (t * PI * 12.).sin().abs()),
            1 => (t, 0.85 - 0.65 * (t * PI * 3.).sin().abs()),
            2 => (0.5 + 0.44 * phase.cos(), 0.5 + 0.39 * phase.sin()),
            3 => {
                let peek = (t * 4.).fract();
                let right = (t * 4.) as usize % 2 == 1;
                (
                    if right {
                        1.03 - 0.09 * (peek * PI).sin()
                    } else {
                        -0.03 + 0.09 * (peek * PI).sin()
                    },
                    0.25 + 0.5 * (t * PI).sin().abs(),
                )
            }
            _ => (0.5 + 0.42 * phase.sin(), 0.5 + 0.3 * (phase * 2.).sin()),
        };
        let at = Pos2::new(
            safe.left() + safe.width() * x,
            safe.top() + safe.height() * y,
        );
        let facing = match self.act {
            2 => -phase.sin(),
            4 => phase.cos(),
            3 => {
                if x > 0.5 {
                    -1.
                } else {
                    1.
                }
            }
            _ => 1.,
        }
        .signum();
        // A rolling yarn ball and tiny sparkles turn a traversal into a game.
        if self.act != 3 {
            let ball = at + Vec2::new(facing * 72., 33. + (phase * 5.).sin() * 9.);
            p.circle_filled(ball, 12., color(crate::appearance::palette(p.ctx()).ginger));
            for i in 0..3 {
                let shift = i as f32 * 4. - 4.;
                p.line_segment(
                    [
                        ball + Vec2::new(-7., shift - 3.),
                        ball + Vec2::new(7., shift + 3.),
                    ],
                    Stroke::new(1.3, color(crate::appearance::palette(p.ctx()).accent)),
                );
            }
            for i in 0..3 {
                let angle = phase * 3. + i as f32 * TAU / 3.;
                let sparkle = ball + Vec2::angled(angle) * 23.;
                let r = 2. + (phase * 8. + i as f32).sin().abs() * 2.;
                p.line_segment(
                    [sparkle - Vec2::X * r, sparkle + Vec2::X * r],
                    Stroke::new(1.5, color(crate::appearance::palette(p.ctx()).accent)),
                );
                p.line_segment(
                    [sparkle - Vec2::Y * r, sparkle + Vec2::Y * r],
                    Stroke::new(1.5, color(crate::appearance::palette(p.ctx()).accent)),
                );
            }
        }
        paint_cat(&p, at, facing, t * 5., alpha);
        // Only the five-second performance requests animation frames. No idle timer.
        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

fn paint_cat(p: &egui::Painter, at: Pos2, facing: f32, seconds: f32, alpha: f32) {
    let ink = Color32::from_rgb(45, 37, 32).gamma_multiply(alpha);
    let cream = Color32::from_rgb(255, 248, 232).gamma_multiply(alpha);
    let ginger = Color32::from_rgb(236, 143, 57).gamma_multiply(alpha);
    let pink = Color32::from_rgb(235, 139, 147).gamma_multiply(alpha);
    let bob = (seconds * 15.).sin() * 3.;
    let pt = |x, y| at + Vec2::new(x * facing, y + bob);
    let ellipse = |center, radii, fill, stroke| {
        p.add(egui::Shape::ellipse_filled(center, radii, fill));
        p.add(egui::Shape::ellipse_stroke(center, radii, stroke));
    };
    ellipse(
        at + Vec2::new(0., 42.),
        Vec2::new(43., 7.),
        Color32::from_black_alpha((22. * alpha) as u8),
        Stroke::NONE,
    );
    // Swishing tail and alternating paws are articulated independently of the body.
    let tail: Vec<_> = (0..16)
        .map(|i| {
            let q = i as f32 / 15.;
            pt(
                -24. - q * 28.,
                12. - q * 42. + (seconds * 7. + q * 2.).sin() * 9. * q,
            )
        })
        .collect();
    p.line(tail.clone(), Stroke::new(12., ink));
    p.line(tail, Stroke::new(8., ginger));
    for i in 0..4 {
        let step = (seconds * 15. + (i % 2) as f32 * PI).sin();
        let x = -22. + i as f32 * 14.;
        p.line_segment(
            [pt(x, 22.), pt(x + step * 9., 35. - step.max(0.) * 7.)],
            Stroke::new(9., ink),
        );
        ellipse(
            pt(x + step * 9., 35. - step.max(0.) * 7.),
            Vec2::new(8., 5.),
            if i == 0 { ginger } else { cream },
            Stroke::new(1.3, ink),
        );
    }
    ellipse(
        pt(-2., 10.),
        Vec2::new(35., 24.),
        cream,
        Stroke::new(2., ink),
    );
    ellipse(pt(-19., 3.), Vec2::new(15., 16.), ginger, Stroke::NONE);
    ellipse(pt(5., 5.), Vec2::new(12., 16.), ink, Stroke::NONE);
    let triangle = |a: (f32, f32), b: (f32, f32), c: (f32, f32), fill| {
        p.add(egui::Shape::convex_polygon(
            vec![pt(a.0, a.1), pt(b.0, b.1), pt(c.0, c.1)],
            fill,
            Stroke::new(2., ink),
        ));
    };
    triangle((8., -20.), (7., -47.), (27., -29.), ginger);
    triangle((31., -29.), (50., -44.), (49., -16.), ink);
    triangle((12., -25.), (11., -39.), (22., -29.), pink);
    triangle((37., -28.), (46., -37.), (45., -24.), pink);
    ellipse(
        pt(29., -15.),
        Vec2::new(27., 24.),
        cream,
        Stroke::new(2., ink),
    );
    ellipse(pt(13., -24.), Vec2::new(11., 13.), ginger, Stroke::NONE);
    ellipse(pt(43., -24.), Vec2::new(11., 13.), ink, Stroke::NONE);
    for x in [19., 39.] {
        ellipse(
            pt(x, -16.),
            Vec2::new(5.8, 7.),
            Color32::WHITE.gamma_multiply(alpha),
            Stroke::new(1., ink),
        );
        if (seconds * 1.5).fract() > 0.92 {
            p.line_segment([pt(x - 3., -16.), pt(x + 3., -16.)], Stroke::new(2., ink));
        } else {
            ellipse(pt(x + 1., -15.), Vec2::new(2.8, 4.5), ink, Stroke::NONE);
            p.circle_filled(pt(x + 2., -17.), 1.3, cream);
        }
    }
    triangle((25., -6.), (33., -6.), (29., -2.), pink);
    p.line(
        vec![
            pt(23., 1.),
            pt(26., 3.),
            pt(29., 0.),
            pt(32., 3.),
            pt(35., 1.),
        ],
        Stroke::new(1.3, ink),
    );
    for sign in [-1., 1.] {
        for dy in [-4., 1., 6.] {
            p.line_segment(
                [pt(29. + sign * 13., -2.), pt(29. + sign * 30., dy)],
                Stroke::new(1., ink),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn each_play_changes_act_and_expires_after_five_seconds() {
        let mut play = CalicoPlay::default();
        for i in 0..20 {
            let old = play.act;
            let now = 100. + i as f64 * 10.;
            play.start(now);
            assert_ne!(play.act, old);
            assert_eq!(play.progress(now), Some(0.));
            assert!(play.progress(now + 4.99).is_some());
            assert!(play.progress(now + 5.).is_none());
            assert!(play.progress(now + 5.01).is_none());
        }
    }
}
