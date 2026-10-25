// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! Fluxer's real design tokens, transcribed from the official coal theme.
//!
//! The installed desktop client stores `baseTheme: "coal"` (see
//! `desktop-app-store.sqlite3`), so the clone uses the same pitch-black
//! surfaces rather than the purple-tinted default dark palette:
//!   --background-primary        #0a090a
//!   --background-secondary      #0a090a  (alias of primary)
//!   --background-secondary-lighter #0a090a (alias of primary)
//!   --background-secondary-alt  #0f0e12
//!   --background-tertiary       #141217
//!   --background-textarea       #171719
//!   --panel-control-bg          #101011
//!   --text-primary              #f4f4f6
//!   --text-secondary            #e5e4e9
//!   --text-tertiary             #b1acbb
//!   --brand-primary             #4641D9 (instance override)
//!   --layout-guild-list-width   4.5rem  (72 px)
//!   --layout-sidebar-width      20rem   (320 px)
//!   --layout-header-height      3.5rem  (56 px)
//!   --layout-member-list-width  16.5rem (264 px)
//!   --message-avatar-size       2.5rem  (40 px)
//!   --message-gutter            .75rem  (12 px)
//!   --message-group-spacing     1rem    (16 px)
//!   --guild-list-item-box-size  3rem    (48 px)
//!   --guild-list-item-gap       .375rem (6 px)
//!   --guild-icon-size           2.75rem (44 px)
//!   --footer-box-height         3.625rem (58 px)
//!   --footer-box-inset          .375rem (6 px)

use eframe::egui::{self, Color32, CornerRadius, Stroke};

pub const BG_PRIMARY: Color32 = Color32::from_rgb(0x0a, 0x09, 0x0a);
pub const BG_RAIL: Color32 = Color32::from_rgb(0x09, 0x09, 0x0a);
pub const BG_SIDEBAR: Color32 = Color32::from_rgb(0x0a, 0x09, 0x0a);
pub const BG_SIDEBAR_LIGHTER: Color32 = Color32::from_rgb(0x0a, 0x09, 0x0a);
pub const BG_TERTIARY: Color32 = Color32::from_rgb(0x14, 0x12, 0x17);
/// Panneau de contrôle du bas (zone utilisateur / vocal).
pub const BG_PANEL_CONTROL: Color32 = Color32::from_rgb(0x10, 0x10, 0x11);
/// Survol d'un contrôle (blanc 8 % sur le noir coal).
pub const BG_HOVER: Color32 = Color32::from_rgb(0x1d, 0x1c, 0x1d);
/// Fond des icônes de guilde sans image (`--guild-list-foreground` coal,
/// relevé sur le client officiel : #1a191c).
pub const BG_GUILD_FOREGROUND: Color32 = Color32::from_rgb(0x1a, 0x19, 0x1c);

/// Composer surface (official `--background-textarea`) and inline code.
pub const BG_COMPOSER: Color32 = Color32::from_rgb(0x17, 0x17, 0x19);
pub const CODE_BG: Color32 = Color32::from_rgb(0x11, 0x10, 0x13);
/// Translucent white overlays. `Color32` stores *premultiplied* values, so a
/// white overlay at alpha `a` is `(a, a, a, a)` — passing 255 would render as
/// an almost opaque white.
pub const CHANNEL_HOVER: Color32 = Color32::from_rgba_premultiplied(13, 13, 13, 13);
pub const CHANNEL_ACTIVE: Color32 = Color32::from_rgba_premultiplied(20, 20, 20, 20);
pub const MESSAGE_HOVER: Color32 = Color32::from_rgba_premultiplied(10, 10, 10, 10);

pub const ACCENT: Color32 = Color32::from_rgb(0x46, 0x41, 0xd9);
/// Fond atténué de l'accent, utilisé pour les réactions actives.
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(0x22, 0x20, 0x55);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0x5b, 0x57, 0xe8);

pub const TEXT: Color32 = Color32::from_rgb(0xf4, 0xf4, 0xf6);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0xe5, 0xe4, 0xe9);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0xb1, 0xac, 0xbb);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x9d, 0x97, 0xaa);

pub const LINK: Color32 = Color32::from_rgb(0x6c, 0xcf, 0xf9);
pub const ONLINE: Color32 = Color32::from_rgb(0x1d, 0xaf, 0x6b);
pub const IDLE: Color32 = Color32::from_rgb(0xf6, 0xbb, 0x09);
pub const DND: Color32 = Color32::from_rgb(0xef, 0x43, 0x43);
pub const OFFLINE: Color32 = Color32::from_rgb(0x9c, 0xa3, 0xb0);
pub const DANGER: Color32 = Color32::from_rgb(0xe5, 0x37, 0x34);

/// Anneaux et séparateurs : blanc translucide, jamais une ligne grise opaque.
pub const BORDER: Color32 = Color32::from_rgba_premultiplied(18, 18, 18, 18);
pub const DIVIDER: Color32 = Color32::from_rgba_premultiplied(13, 13, 13, 13);
/// Bordure des cartes (embed, invitation, fichier) : équivalent de
/// `--background-modifier-accent` composé sur le fond coal.
pub const CARD_BORDER: Color32 = Color32::from_rgb(0x1c, 0x1e, 0x22);

pub const LAYOUT_RAIL_WIDTH: f32 = 72.0;
pub const LAYOUT_SIDEBAR_WIDTH: f32 = 320.0;
pub const LAYOUT_HEADER_HEIGHT: f32 = 56.0;
pub const LAYOUT_MEMBER_LIST_WIDTH: f32 = 264.0;
pub const LAYOUT_USER_AREA_HEIGHT: f32 = 58.0;
/// Hauteur de la boîte de saisie (`--footer-box-height`).
pub const LAYOUT_COMPOSER_BOX_HEIGHT: f32 = 58.0;
/// Ligne de statut réservée au-dessus du composeur.
pub const LAYOUT_STATUS_ROW_HEIGHT: f32 = 24.0;
pub const GUILD_ICON_SIZE: f32 = 44.0;
pub const MESSAGE_AVATAR_SIZE: f32 = 40.0;

pub fn apply(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG_PRIMARY;
    visuals.window_fill = BG_SIDEBAR;
    visuals.extreme_bg_color = BG_RAIL;
    visuals.faint_bg_color = BG_SIDEBAR_LIGHTER;
    visuals.override_text_color = Some(TEXT);
    visuals.hyperlink_color = LINK;
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.55);
    visuals.selection.stroke = Stroke::new(1.0, ACCENT_HOVER);
    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.menu_corner_radius = CornerRadius::same(6);
    visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(120),
    };
    visuals.popup_shadow = visuals.window_shadow;

    visuals.widgets.noninteractive.bg_fill = BG_SIDEBAR;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, DIVIDER);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_SECONDARY);
    visuals.widgets.inactive.bg_fill = BG_TERTIARY;
    visuals.widgets.inactive.weak_bg_fill = BG_TERTIARY;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_SECONDARY);
    visuals.widgets.hovered.bg_fill = BG_HOVER;
    visuals.widgets.hovered.weak_bg_fill = BG_HOVER;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.active.bg_fill = ACCENT;
    visuals.widgets.active.weak_bg_fill = ACCENT;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.open.bg_fill = BG_PANEL_CONTROL;

    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, visuals);

    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 4.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.scroll = egui::style::ScrollStyle {
            bar_width: 8.0,
            ..Default::default()
        };
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(15.5));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.5));
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(20.0));
        style
            .text_styles
            .insert(egui::TextStyle::Monospace, egui::FontId::monospace(13.5));
    });
}

/// Name colour for a user without a role colour: Fluxer keeps it white.
pub fn name_color(_id: &str) -> Color32 {
    TEXT
}

/// Stable pastel colour for initials when no avatar/colour is available.
pub fn fallback_color(seed: &str) -> Color32 {
    const PALETTE: [Color32; 10] = [
        Color32::from_rgb(0x58, 0x65, 0xf2),
        Color32::from_rgb(0x3b, 0xa5, 0x5d),
        Color32::from_rgb(0xf0, 0xb2, 0x32),
        Color32::from_rgb(0xed, 0x42, 0x45),
        Color32::from_rgb(0xeb, 0x45, 0x9b),
        Color32::from_rgb(0x9b, 0x59, 0xb6),
        Color32::from_rgb(0x1a, 0xbc, 0x9c),
        Color32::from_rgb(0xe6, 0x7e, 0x22),
        Color32::from_rgb(0x54, 0x96, 0xf8),
        Color32::from_rgb(0x8e, 0x92, 0x9a),
    ];
    let hash = seed.bytes().fold(0u64, |acc, byte| {
        acc.wrapping_mul(1099511628211).wrapping_add(byte as u64)
    });
    PALETTE[(hash % PALETTE.len() as u64) as usize]
}

/// Converts the API's packed RGB integer (`avatar_color`, role colour) into a
/// usable egui colour.
pub fn packed_color(value: Option<i64>, fallback_seed: &str) -> Color32 {
    match value {
        Some(rgb) if (1..=0xFF_FF_FF).contains(&rgb) => Color32::from_rgb(
            ((rgb >> 16) & 0xFF) as u8,
            ((rgb >> 8) & 0xFF) as u8,
            (rgb & 0xFF) as u8,
        ),
        _ => fallback_color(fallback_seed),
    }
}

pub fn role_color(value: i64) -> Option<Color32> {
    if value <= 0 {
        return None;
    }
    Some(packed_color(Some(value), ""))
}

/// Presence colour, defaulting to offline grey.
pub fn status_color(status: Option<&str>) -> Color32 {
    match status.unwrap_or("offline") {
        "online" => ONLINE,
        "idle" => IDLE,
        "dnd" => DND,
        _ => OFFLINE,
    }
}

/// Colour for a guild icon when the guild has no image.
pub fn guild_color(seed: &str) -> Color32 {
    fallback_color(seed).gamma_multiply(0.85)
}
