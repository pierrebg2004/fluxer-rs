// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! Font setup.
//!
//! The official client ships "Fluxer Sans" as woff2 webfonts. egui cannot parse
//! woff2, and bundling a proprietary font is pointless, so the closest
//! metric-compatible open replacement is used: **Noto Sans**, the default
//! sans-serif on this system. Regular, bold, italic, bold-italic and monospace
//! are installed as separate families so `markdown.rs` can request a real bold
//! face instead of faking weight.

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use std::sync::Arc;

pub const BOLD: &str = "fluxer-bold";
pub const ITALIC: &str = "fluxer-italic";
pub const BOLD_ITALIC: &str = "fluxer-bold-italic";

struct Face {
    key: &'static str,
    paths: &'static [&'static str],
}

const FACES: &[Face] = &[
    Face {
        key: "noto-sans",
        paths: &[
            "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/TTF/NotoSans-Regular.ttf",
        ],
    },
    Face {
        key: "noto-sans-bold",
        paths: &[
            "/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf",
            "/usr/share/fonts/noto/NotoSans-Bold.ttf",
            "/usr/share/fonts/TTF/NotoSans-Bold.ttf",
        ],
    },
    Face {
        key: "noto-sans-italic",
        paths: &[
            "/usr/share/fonts/truetype/noto/NotoSans-Italic.ttf",
            "/usr/share/fonts/noto/NotoSans-Italic.ttf",
            "/usr/share/fonts/TTF/NotoSans-Italic.ttf",
        ],
    },
    Face {
        key: "noto-sans-bold-italic",
        paths: &[
            "/usr/share/fonts/truetype/noto/NotoSans-BoldItalic.ttf",
            "/usr/share/fonts/noto/NotoSans-BoldItalic.ttf",
            "/usr/share/fonts/TTF/NotoSans-BoldItalic.ttf",
        ],
    },
    Face {
        key: "noto-mono",
        paths: &[
            "/usr/share/fonts/truetype/noto/NotoSansMono-Regular.ttf",
            "/usr/share/fonts/noto/NotoSansMono-Regular.ttf",
            "/usr/share/fonts/TTF/NotoSansMono-Regular.ttf",
        ],
    },
    Face {
        key: "noto-emoji",
        paths: &[
            "/usr/share/fonts/truetype/noto/NotoEmoji-Regular.ttf",
            "/usr/share/fonts/noto/NotoEmoji-Regular.ttf",
        ],
    },
    Face {
        key: "noto-color-emoji",
        paths: &[
            "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
            "/usr/share/fonts/noto/NotoColorEmoji.ttf",
        ],
    },
];

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let mut loaded: Vec<&'static str> = Vec::new();

    for face in FACES {
        for path in face.paths {
            let Ok(bytes) = std::fs::read(path) else {
                continue;
            };
            fonts
                .font_data
                .insert(face.key.to_owned(), Arc::new(FontData::from_owned(bytes)));
            loaded.push(face.key);
            break;
        }
    }

    if loaded.contains(&"noto-sans") {
        prepend(&mut fonts, FontFamily::Proportional, "noto-sans");
        // Emoji fonts must come *after* the text font: Noto Color Emoji also
        // owns a space glyph with a much wider advance, which would stretch
        // every space as soon as it is listed first.
        if loaded.contains(&"noto-emoji") {
            append(&mut fonts, FontFamily::Proportional, "noto-emoji");
        }
        if loaded.contains(&"noto-color-emoji") {
            append(&mut fonts, FontFamily::Proportional, "noto-color-emoji");
        }

        let bold = FontFamily::Name(BOLD.into());
        let italic = FontFamily::Name(ITALIC.into());
        let bold_italic = FontFamily::Name(BOLD_ITALIC.into());

        let fallbacks = fonts
            .families
            .get(&FontFamily::Proportional)
            .cloned()
            .unwrap_or_default();

        if loaded.contains(&"noto-sans-bold") {
            fonts
                .families
                .insert(bold, with_first("noto-sans-bold", &fallbacks));
        } else {
            fonts.families.insert(bold, fallbacks.clone());
        }
        if loaded.contains(&"noto-sans-italic") {
            fonts
                .families
                .insert(italic, with_first("noto-sans-italic", &fallbacks));
        } else {
            fonts.families.insert(italic, fallbacks.clone());
        }
        if loaded.contains(&"noto-sans-bold-italic") {
            fonts
                .families
                .insert(bold_italic, with_first("noto-sans-bold-italic", &fallbacks));
        } else {
            fonts.families.insert(bold_italic, fallbacks);
        }
    }

    if loaded.contains(&"noto-mono") {
        prepend(&mut fonts, FontFamily::Monospace, "noto-mono");
        if loaded.contains(&"noto-emoji") {
            append(&mut fonts, FontFamily::Monospace, "noto-emoji");
        }
    }

    ctx.set_fonts(fonts);
}

fn prepend(fonts: &mut FontDefinitions, family: FontFamily, key: &str) {
    let entry = fonts.families.entry(family).or_default();
    if !entry.iter().any(|existing| existing == key) {
        entry.insert(0, key.to_owned());
    }
}

/// Adds a fallback at the very end of the family list, so it only serves
/// glyphs that no earlier font provides.
fn append(fonts: &mut FontDefinitions, family: FontFamily, key: &str) {
    let entry = fonts.families.entry(family).or_default();
    if !entry.iter().any(|existing| existing == key) {
        entry.push(key.to_owned());
    }
}

fn with_first(key: &str, fallbacks: &[String]) -> Vec<String> {
    let mut list = Vec::with_capacity(fallbacks.len() + 1);
    list.push(key.to_owned());
    list.extend(fallbacks.iter().filter(|f| *f != key).cloned());
    list
}

/// Font id for bold UI text (headings, usernames).
pub fn bold(size: f32) -> egui::FontId {
    egui::FontId::new(size, FontFamily::Name(BOLD.into()))
}

pub fn italic(size: f32) -> egui::FontId {
    egui::FontId::new(size, FontFamily::Name(ITALIC.into()))
}
