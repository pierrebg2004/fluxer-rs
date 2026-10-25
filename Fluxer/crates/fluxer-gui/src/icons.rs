// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! Vector icon set drawn with the egui painter.
//!
//! The official client ships its icon font as woff2, which egui cannot load, so
//! every glyph used by fluxer-rs is reproduced here with primitives. All icons
//! are designed on a 24×24 grid and scaled to the requested rectangle.

use eframe::egui::{self, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, pos2};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Hash,
    Volume,
    Megaphone,
    ChevronDown,
    ChevronRight,
    Search,
    Members,
    Pin,
    Settings,
    Mic,
    Headphones,
    Plus,
    Gift,
    Gif,
    Emoji,
    Bell,
    Help,
    Compass,
    Inbox,
    Close,
    More,
    Screen,
    Eye,
    Star,
    Phone,
    Camera,
    Link,
    Note,
    Forum,
    Image,
    Thread,
    /// Fichier joint (carte de téléchargement, sans bouton).
    File,
    /// Lecture d'un média vidéo.
    Play,
}

/// Paints `icon` centred in `rect` using `color`.
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32, width: f32) {
    let r = square(rect);
    let stroke = Stroke::new(width, color);
    let line = |points: &[(f32, f32)]| {
        let pts: Vec<Pos2> = points.iter().map(|&(x, y)| at(r, x, y)).collect();
        painter.add(Shape::line(pts, stroke));
    };
    let closed = |points: &[(f32, f32)]| {
        let pts: Vec<Pos2> = points.iter().map(|&(x, y)| at(r, x, y)).collect();
        painter.add(Shape::closed_line(pts, stroke));
    };
    let fill = |points: &[(f32, f32)]| {
        let pts: Vec<Pos2> = points.iter().map(|&(x, y)| at(r, x, y)).collect();
        painter.add(Shape::convex_polygon(pts, color, Stroke::NONE));
    };
    let circle = |cx: f32, cy: f32, radius: f32| {
        painter.circle_stroke(at(r, cx, cy), radius * r.width() / 24.0, stroke);
    };
    let arc = |cx: f32, cy: f32, radius: f32, start: f32, end: f32| {
        let steps = 20;
        let mut pts = Vec::with_capacity(steps + 1);
        for i in 0..=steps {
            let t = start + (end - start) * i as f32 / steps as f32;
            pts.push(at(r, cx + radius * t.cos(), cy + radius * t.sin()));
        }
        painter.add(Shape::line(pts, stroke));
    };

    match icon {
        Icon::Hash => {
            line(&[(9.0, 4.0), (8.0, 20.0)]);
            line(&[(16.0, 4.0), (15.0, 20.0)]);
            line(&[(4.5, 9.0), (20.0, 9.0)]);
            line(&[(4.0, 15.0), (19.5, 15.0)]);
        }
        Icon::Volume => {
            fill(&[
                (4.0, 9.0),
                (8.0, 9.0),
                (12.0, 5.0),
                (12.0, 19.0),
                (8.0, 15.0),
                (4.0, 15.0),
            ]);
            arc(12.0, 12.0, 5.0, -0.9, 0.9);
            arc(12.0, 12.0, 8.0, -0.9, 0.9);
        }
        Icon::Megaphone => {
            fill(&[(4.0, 9.5), (14.0, 5.0), (14.0, 19.0), (4.0, 14.5)]);
            line(&[(7.0, 14.0), (8.0, 19.0)]);
            line(&[(14.0, 8.0), (19.0, 6.0)]);
            line(&[(14.0, 12.0), (20.0, 12.0)]);
            line(&[(14.0, 16.0), (19.0, 18.0)]);
        }
        Icon::ChevronDown => line(&[(7.0, 9.5), (12.0, 14.5), (17.0, 9.5)]),
        Icon::ChevronRight => line(&[(9.5, 7.0), (14.5, 12.0), (9.5, 17.0)]),
        Icon::Search => {
            circle(10.5, 10.5, 6.0);
            line(&[(15.0, 15.0), (20.5, 20.5)]);
        }
        Icon::Members => {
            circle(9.0, 8.0, 3.2);
            arc(9.0, 21.0, 7.0, std::f32::consts::PI, std::f32::consts::TAU);
            circle(17.0, 9.5, 2.6);
            arc(17.0, 21.0, 5.5, std::f32::consts::PI, std::f32::consts::TAU);
        }
        Icon::Pin => {
            fill(&[
                (9.0, 4.0),
                (15.0, 4.0),
                (14.0, 11.0),
                (16.5, 15.0),
                (7.5, 15.0),
                (10.0, 11.0),
            ]);
            line(&[(12.0, 15.0), (12.0, 21.0)]);
        }
        Icon::Settings => {
            circle(12.0, 12.0, 3.2);
            circle(12.0, 12.0, 7.2);
            for i in 0..8 {
                let angle = i as f32 * std::f32::consts::TAU / 8.0;
                let a = at(r, 12.0 + 7.2 * angle.cos(), 12.0 + 7.2 * angle.sin());
                let b = at(r, 12.0 + 10.0 * angle.cos(), 12.0 + 10.0 * angle.sin());
                painter.line_segment([a, b], stroke);
            }
        }
        Icon::Mic => {
            rounded_capsule(painter, r, 9.0, 3.0, 15.0, 13.0, color);
            arc(12.0, 12.0, 6.2, 0.15, std::f32::consts::PI - 0.15);
            line(&[(12.0, 18.2), (12.0, 21.0)]);
            line(&[(9.0, 21.0), (15.0, 21.0)]);
        }
        Icon::Headphones => {
            arc(12.0, 13.0, 8.0, std::f32::consts::PI, std::f32::consts::TAU);
            rounded_box(painter, r, 3.2, 12.5, 7.2, 19.5, 1.8, color);
            rounded_box(painter, r, 16.8, 12.5, 20.8, 19.5, 1.8, color);
        }
        Icon::Plus => {
            line(&[(12.0, 4.0), (12.0, 20.0)]);
            line(&[(4.0, 12.0), (20.0, 12.0)]);
        }
        Icon::Gift => {
            rounded_box(painter, r, 3.5, 9.5, 20.5, 20.5, 1.5, color);
            line(&[(2.5, 9.5), (21.5, 9.5)]);
            line(&[(3.0, 6.0), (21.0, 6.0)]);
            line(&[(3.0, 6.0), (3.0, 9.5)]);
            line(&[(21.0, 6.0), (21.0, 9.5)]);
            line(&[(12.0, 6.0), (12.0, 20.5)]);
            circle(9.0, 5.0, 2.4);
            circle(15.0, 5.0, 2.4);
        }
        Icon::Gif => {
            painter.text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                "GIF",
                FontId::proportional(r.height() * 0.52),
                color,
            );
        }
        Icon::Emoji => {
            circle(12.0, 12.0, 8.8);
            painter.circle_filled(at(r, 9.0, 9.8), width * 0.9, color);
            painter.circle_filled(at(r, 15.0, 9.8), width * 0.9, color);
            arc(12.0, 12.5, 5.2, 0.35, std::f32::consts::PI - 0.35);
        }
        Icon::Bell => {
            line(&[(5.0, 17.0), (19.0, 17.0)]);
            line(&[(7.0, 17.0), (7.0, 10.0)]);
            line(&[(17.0, 17.0), (17.0, 10.0)]);
            arc(12.0, 10.0, 5.0, std::f32::consts::PI, std::f32::consts::TAU);
            circle(12.0, 19.0, 1.6);
        }
        Icon::Help => {
            circle(12.0, 12.0, 8.8);
            painter.text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                "?",
                FontId::proportional(r.height() * 0.62),
                color,
            );
        }
        Icon::Compass => {
            circle(12.0, 12.0, 8.8);
            fill(&[(12.0, 6.0), (15.5, 15.5), (12.0, 13.5), (8.5, 15.5)]);
        }
        Icon::Inbox => {
            closed(&[(4.0, 5.0), (20.0, 5.0), (20.0, 18.0), (4.0, 18.0)]);
            line(&[
                (4.0, 13.0),
                (9.0, 13.0),
                (10.5, 15.5),
                (13.5, 15.5),
                (15.0, 13.0),
                (20.0, 13.0),
            ]);
        }
        Icon::Close => {
            line(&[(6.0, 6.0), (18.0, 18.0)]);
            line(&[(18.0, 6.0), (6.0, 18.0)]);
        }
        Icon::More => {
            for x in [6.0, 12.0, 18.0] {
                painter.circle_filled(at(r, x, 12.0), width, color);
            }
        }
        Icon::Screen => {
            closed(&[(3.0, 5.0), (21.0, 5.0), (21.0, 17.0), (3.0, 17.0)]);
            line(&[(9.0, 21.0), (15.0, 21.0)]);
            line(&[(12.0, 17.0), (12.0, 21.0)]);
        }
        Icon::Eye => {
            let points: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let t = std::f32::consts::TAU * i as f32 / 24.0;
                    at(r, 12.0 + 9.0 * t.cos(), 12.0 + 5.0 * t.sin())
                })
                .collect();
            painter.add(Shape::closed_line(points, stroke));
            painter.circle_filled(r.center(), 2.6 * r.width() / 24.0, color);
        }
        Icon::Star => {
            // Five-pointed star used for the favorites destination.
            let mut points = Vec::with_capacity(10);
            for i in 0..10 {
                let radius = if i % 2 == 0 { 9.4 } else { 4.0 };
                let angle = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
                points.push(at(
                    r,
                    12.0 + radius * angle.cos(),
                    12.0 + radius * angle.sin(),
                ));
            }
            painter.add(Shape::closed_line(points, stroke));
        }
        Icon::Phone => {
            // Handset drawn as a thick rounded path.
            let pts: Vec<Pos2> = [
                (6.0, 4.0),
                (9.0, 3.5),
                (11.0, 8.0),
                (9.0, 10.0),
                (14.0, 15.0),
                (16.0, 13.0),
                (20.5, 15.0),
                (20.0, 18.0),
                (17.5, 20.5),
                (14.0, 20.5),
                (5.0, 11.0),
                (4.0, 6.5),
            ]
            .iter()
            .map(|&(x, y)| at(r, x, y))
            .collect();
            painter.add(Shape::line(pts, stroke));
        }
        Icon::Camera => {
            closed(&[(3.0, 7.0), (15.0, 7.0), (15.0, 17.0), (3.0, 17.0)]);
            fill(&[(15.0, 10.0), (21.0, 7.0), (21.0, 17.0), (15.0, 14.0)]);
        }
        Icon::Link => {
            // Deux maillons inclinés, comme l'icône de salon de lien.
            closed(&[
                (9.0, 15.0),
                (6.5, 17.5),
                (4.0, 15.0),
                (10.0, 9.0),
                (12.5, 11.5),
            ]);
            closed(&[
                (15.0, 9.0),
                (17.5, 6.5),
                (20.0, 9.0),
                (14.0, 15.0),
                (11.5, 12.5),
            ]);
            line(&[(8.5, 15.5), (15.5, 8.5)]);
        }
        Icon::Note => {
            closed(&[(5.0, 4.0), (19.0, 4.0), (19.0, 20.0), (5.0, 20.0)]);
            line(&[(8.0, 9.0), (16.0, 9.0)]);
            line(&[(8.0, 13.0), (16.0, 13.0)]);
            line(&[(8.0, 17.0), (13.0, 17.0)]);
        }
        Icon::Forum => {
            closed(&[
                (3.0, 5.0),
                (16.0, 5.0),
                (16.0, 15.0),
                (8.0, 15.0),
                (4.0, 19.0),
                (4.0, 15.0),
                (3.0, 15.0),
            ]);
            line(&[
                (18.0, 8.0),
                (21.0, 8.0),
                (21.0, 21.0),
                (18.0, 18.0),
                (10.0, 18.0),
                (10.0, 15.0),
            ]);
        }
        Icon::Image => {
            closed(&[(3.0, 5.0), (21.0, 5.0), (21.0, 19.0), (3.0, 19.0)]);
            circle(8.5, 10.0, 1.8);
            line(&[
                (4.5, 17.0),
                (10.0, 11.5),
                (14.0, 15.0),
                (17.0, 12.5),
                (20.0, 16.0),
            ]);
        }
        Icon::Thread => {
            closed(&[
                (4.0, 5.0),
                (20.0, 5.0),
                (20.0, 16.0),
                (10.0, 16.0),
                (6.0, 20.0),
                (6.0, 16.0),
                (4.0, 16.0),
            ]);
            line(&[(8.0, 9.5), (16.0, 9.5)]);
            line(&[(8.0, 12.5), (13.0, 12.5)]);
        }
        Icon::File => {
            closed(&[
                (6.0, 3.0),
                (14.0, 3.0),
                (18.0, 7.0),
                (18.0, 21.0),
                (6.0, 21.0),
            ]);
            line(&[(14.0, 3.0), (14.0, 7.0), (18.0, 7.0)]);
            line(&[(9.0, 12.0), (15.0, 12.0)]);
            line(&[(9.0, 16.0), (15.0, 16.0)]);
        }
        Icon::Play => {
            fill(&[(8.0, 5.0), (19.0, 12.0), (8.0, 19.0)]);
        }
    }
}

fn at(rect: Rect, x: f32, y: f32) -> Pos2 {
    pos2(
        rect.left() + x / 24.0 * rect.width(),
        rect.top() + y / 24.0 * rect.height(),
    )
}

fn square(rect: Rect) -> Rect {
    let side = rect.width().min(rect.height());
    Rect::from_center_size(rect.center(), egui::Vec2::splat(side))
}

fn rounded_capsule(
    painter: &Painter,
    rect: Rect,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    color: Color32,
) {
    painter.rect_filled(
        Rect::from_min_max(at(rect, x0, y0), at(rect, x1, y1)),
        egui::CornerRadius::same((rect.width() * 0.15) as u8),
        color,
    );
}

// Les coordonnées sont exprimées dans une grille 24×24 ; les garder en
// arguments explicites rend les tracés lisibles.
#[allow(clippy::too_many_arguments)]
fn rounded_box(
    painter: &Painter,
    rect: Rect,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    radius: f32,
    color: Color32,
) {
    painter.rect_filled(
        Rect::from_min_max(at(rect, x0, y0), at(rect, x1, y1)),
        egui::CornerRadius::same((radius / 24.0 * rect.width()) as u8),
        color,
    );
}
