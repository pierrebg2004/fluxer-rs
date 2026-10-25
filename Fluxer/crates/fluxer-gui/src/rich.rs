// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! Rendu des contenus riches d'un message : pièces jointes visuelles,
//! cartes de fichiers, embeds OpenGraph/YouTube et invitations.
//!
//! La géométrie reprend celle du client officiel (feuilles de style
//! `ChannelEmbed.module.css`, `EmbedCard.module.css`, `AttachmentFile.module.css`
//! et `MediaDimensionConfig.ts`) : médias 550×350 px au maximum, cartes
//! d'embed 516 px (432 px avec média), bord gauche coloré de 4 px, cartes
//! d'invitation 360 px avec bannière de 60 px.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use eframe::egui::{
    Align2, Color32, CornerRadius, FontId, Image, Pos2, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2,
};
use eframe::egui::text::{Galley, LayoutJob};
use fluxer_core::{Attachment, Embed, InvitePreview, Message};

use crate::app::FluxerApp;
use crate::engine::Command;
use crate::{fonts, icons, markdown, theme};

/// Largeur maximale d'une mosaïque de pièces jointes.
const ATTACHMENT_MAX_WIDTH: f32 = 550.0;
/// Hauteur maximale d'une pièce jointe seule.
const ATTACHMENT_MAX_HEIGHT: f32 = 350.0;
/// Largeur maximale d'une carte d'embed sans média.
const EMBED_TEXT_WIDTH: f32 = 516.0;
/// Largeur maximale d'une carte d'embed avec image ou vidéo.
const EMBED_MEDIA_WIDTH: f32 = 432.0;
/// Largeur maximale d'une carte de fichier.
const FILE_CARD_WIDTH: f32 = 400.0;
/// Largeur maximale d'une carte d'invitation.
const INVITE_CARD_WIDTH: f32 = 360.0;
/// Espacement vertical entre deux blocs de contenu (4 px officiels).
pub const BLOCK_GAP: f32 = 4.0;
/// Écart des tuiles de mosaïque.
const TILE_GAP: f32 = 4.0;

const EMBED_TITLE_SIZE: f32 = 16.0;
const EMBED_BODY_SIZE: f32 = 14.0;
const EMBED_SMALL_SIZE: f32 = 12.0;
const FOOTNOTE_SIZE: f32 = 10.5;

/// Un bloc de contenu riche placé sous le texte du message.
pub enum Block {
    Media(MediaBlock),
    Files(FileBlock),
    Embed(EmbedBlock),
    Invite(InviteBlock),
}

impl Block {
    pub fn height(&self) -> f32 {
        match self {
            Block::Media(block) => block.height,
            Block::Files(block) => block.height,
            Block::Embed(block) => block.height,
            Block::Invite(block) => block.height,
        }
    }

    pub fn draw(&self, ui: &mut Ui, app: &mut FluxerApp, origin: Pos2, media: &str) {
        match self {
            Block::Media(block) => block.draw(ui, origin),
            Block::Files(block) => block.draw(ui, origin),
            Block::Embed(block) => block.draw(ui, origin, media),
            Block::Invite(block) => block.draw(ui, app, origin, media),
        }
    }
}

// ---------------------------------------------------------------------------
// Pièces jointes visuelles
// ---------------------------------------------------------------------------

struct Tile {
    offset: Vec2,
    size: Vec2,
    natural: Vec2,
    url: String,
    target: String,
    video: bool,
    label: Option<String>,
}

pub struct MediaBlock {
    height: f32,
    footnote: Option<(String, f32)>,
    tiles: Vec<Tile>,
}

impl MediaBlock {
    fn draw(&self, ui: &mut Ui, origin: Pos2) {
        for (index, tile) in self.tiles.iter().enumerate() {
            let rect = Rect::from_min_size(origin + tile.offset, tile.size);
            let uv = cover_uv(tile.natural, rect.size());
            Image::new(tile.url.clone())
                .uv(uv)
                .corner_radius(CornerRadius::same(4))
                .paint_at(ui, rect);
            if tile.video {
                paint_play_overlay(ui, rect, tile.label.as_deref());
            }
            let response = ui
                .interact(rect, ui.scope_id().with(("attachment", index)), Sense::click())
                .on_hover_cursor(eframe::egui::CursorIcon::PointingHand);
            if response.clicked()
                && (tile.target.starts_with("https://") || tile.target.starts_with("http://"))
            {
                crate::ui::open_external(&tile.target);
            }
        }
        if let Some((label, y)) = &self.footnote {
            ui.painter().text(
                origin + vec2(0.0, *y),
                Align2::LEFT_TOP,
                label,
                FontId::proportional(FOOTNOTE_SIZE),
                theme::TEXT_MUTED,
            );
        }
    }
}

/// Recadrage « object-fit: cover » à partir des dimensions naturelles.
fn cover_uv(natural: Vec2, target: Vec2) -> Rect {
    if natural.x <= 0.0 || natural.y <= 0.0 || target.x <= 0.0 || target.y <= 0.0 {
        return Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    }
    let target_ratio = target.x / target.y;
    let natural_ratio = natural.x / natural.y;
    if natural_ratio > target_ratio {
        let visible = target_ratio / natural_ratio;
        let inset = (1.0 - visible) * 0.5;
        Rect::from_min_max(pos2(inset, 0.0), pos2(1.0 - inset, 1.0))
    } else {
        let visible = natural_ratio / target_ratio;
        let inset = (1.0 - visible) * 0.5;
        Rect::from_min_max(pos2(0.0, inset), pos2(1.0, 1.0 - inset))
    }
}

fn paint_play_overlay(ui: &Ui, rect: Rect, label: Option<&str>) {
    ui.painter()
        .circle_filled(rect.center(), 20.0, Color32::from_black_alpha(160));
    icons::paint(
        ui.painter(),
        Rect::from_center_size(rect.center(), Vec2::splat(20.0)),
        icons::Icon::Play,
        Color32::WHITE,
        1.0,
    );
    if let Some(label) = label {
        let badge = Rect::from_min_size(
            pos2(rect.right() - 44.0, rect.bottom() - 22.0),
            vec2(38.0, 16.0),
        );
        ui.painter()
            .rect_filled(badge, CornerRadius::same(3), Color32::from_black_alpha(170));
        ui.painter().text(
            badge.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(11.0),
            Color32::WHITE,
        );
    }
}

fn duration_label(seconds: Option<f64>) -> Option<String> {
    let seconds = seconds?;
    if !seconds.is_finite() || seconds <= 0.0 {
        return None;
    }
    let total = seconds.round() as u64;
    Some(format!("{}:{:02}", total / 60, total % 60))
}

/// Construit la mosaïque des images et vidéos d'un message.
pub fn media_block(message: &Message, available: f32) -> Option<Block> {
    let media: Vec<(usize, &Attachment)> = message
        .attachments
        .iter()
        .enumerate()
        .filter(|(_, attachment)| attachment.visual_size().is_some())
        .collect();
    if media.is_empty() {
        return None;
    }

    let footnote = expiry_footnote(&message.attachments);
    let footnote_height = footnote.as_ref().map(|_| 5.6 + 12.0).unwrap_or(0.0);
    let full_width = available.min(ATTACHMENT_MAX_WIDTH).max(80.0);
    let count = media.len().min(10);

    let mut tiles = Vec::with_capacity(count);
    if count == 1 {
        let (index, attachment) = media[0];
        let (width, height) = attachment.visual_size().unwrap_or((1.0, 1.0));
        let scale = (1.0f32)
            .min(ATTACHMENT_MAX_WIDTH / width)
            .min(ATTACHMENT_MAX_HEIGHT / height);
        let size = vec2((width * scale).round(), (height * scale).round());
        tiles.push(tile_for(index, attachment, Vec2::ZERO, size));
    } else {
        for (index, attachment) in media.iter().take(count) {
            let natural = attachment.visual_size().unwrap_or((1.0, 1.0));
            let (offset, size) = mosaic_slot(count, tiles.len(), full_width);
            tiles.push(tile_for(*index, attachment, offset, size));
            let _ = natural;
        }
    }

    let content_height = tiles
        .iter()
        .map(|tile| tile.offset.y + tile.size.y)
        .fold(0.0f32, f32::max)
        .max(1.0);
    Some(Block::Media(MediaBlock {
        height: content_height + footnote_height,
        footnote: footnote.map(|label| (label, content_height + 5.6)),
        tiles,
    }))
}

fn tile_for(index: usize, attachment: &Attachment, offset: Vec2, size: Vec2) -> Tile {
    let (width, height) = attachment.visual_size().unwrap_or((1.0, 1.0));
    let _ = index;
    Tile {
        offset,
        size,
        natural: vec2(width, height),
        url: attachment.display_url().unwrap_or_default().to_string(),
        target: attachment
            .url
            .clone()
            .or_else(|| attachment.proxy_url.clone())
            .unwrap_or_default(),
        video: attachment.is_video(),
        label: duration_label(attachment.duration),
    }
}

/// Position et taille d'une tuile de mosaïque, selon la disposition officielle.
fn mosaic_slot(count: usize, index: usize, width: f32) -> (Vec2, Vec2) {
    let g = TILE_GAP;
    match count {
        2 => {
            let w = (width - g) / 2.0;
            (vec2(index as f32 * (w + g), 0.0), vec2(w, w))
        }
        3 => {
            let w = (width - 2.0 * g) / 3.0;
            let big = 2.0 * w + g;
            if index == 0 {
                (Vec2::ZERO, vec2(big, big))
            } else {
                (
                    vec2(big + g, (index - 1) as f32 * (w + g)),
                    vec2(w, w),
                )
            }
        }
        4 => {
            let w = (width - g) / 2.0;
            let h = w * 2.0 / 3.0;
            (
                vec2((index % 2) as f32 * (w + g), (index / 2) as f32 * (h + g)),
                vec2(w, h),
            )
        }
        5 => {
            let track = (width - 5.0 * g) / 6.0;
            let big = 3.0 * track + 2.0 * g;
            let small = 2.0 * track + g;
            let big_h = big * 2.0 / 3.0;
            if index < 2 {
                (vec2(index as f32 * (big + g), 0.0), vec2(big, big_h))
            } else {
                (
                    vec2((index - 2) as f32 * (small + g), big_h + g),
                    vec2(small, small),
                )
            }
        }
        6 => {
            let w = (width - 2.0 * g) / 3.0;
            (
                vec2((index % 3) as f32 * (w + g), (index / 3) as f32 * (w + g)),
                vec2(w, w),
            )
        }
        7 => {
            let hero = width * 9.0 / 16.0;
            let w = (width - 2.0 * g) / 3.0;
            if index == 0 {
                (Vec2::ZERO, vec2(width, hero))
            } else {
                let i = index - 1;
                (
                    vec2((i % 3) as f32 * (w + g), hero + g + (i / 3) as f32 * (w + g)),
                    vec2(w, w),
                )
            }
        }
        8 => {
            let top_w = (width - g) / 2.0;
            let top_h = top_w * 2.0 / 3.0;
            let w = (width - 2.0 * g) / 3.0;
            if index < 2 {
                (vec2(index as f32 * (top_w + g), 0.0), vec2(top_w, top_h))
            } else {
                let i = index - 2;
                (
                    vec2((i % 3) as f32 * (w + g), top_h + g + (i / 3) as f32 * (w + g)),
                    vec2(w, w),
                )
            }
        }
        9 => {
            let w = (width - 2.0 * g) / 3.0;
            (
                vec2((index % 3) as f32 * (w + g), (index / 3) as f32 * (w + g)),
                vec2(w, w),
            )
        }
        _ => {
            let hero = width * 9.0 / 16.0;
            let w = (width - 2.0 * g) / 3.0;
            if index == 0 {
                (Vec2::ZERO, vec2(width, hero))
            } else {
                let i = index - 1;
                (
                    vec2((i % 3) as f32 * (w + g), hero + g + (i / 3) as f32 * (w + g)),
                    vec2(w, w),
                )
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Cartes de fichiers
// ---------------------------------------------------------------------------

struct FileCard {
    y: f32,
    name: Arc<Galley>,
    meta: Option<Arc<Galley>>,
    target: String,
}

pub struct FileBlock {
    height: f32,
    cards: Vec<FileCard>,
    footnote: Option<(String, f32)>,
}

impl FileBlock {
    fn draw(&self, ui: &mut Ui, origin: Pos2) {
        for (index, card) in self.cards.iter().enumerate() {
            let rect = Rect::from_min_size(origin + vec2(0.0, card.y), vec2(FILE_CARD_WIDTH, 74.0));
            ui.painter().rect(
                rect,
                CornerRadius::same(8),
                theme::BG_PRIMARY,
                Stroke::new(1.0, theme::CARD_BORDER),
                eframe::egui::StrokeKind::Inside,
            );
            let icon_rect = Rect::from_min_size(rect.min + vec2(12.0, 13.0), Vec2::splat(48.0));
            ui.painter()
                .rect_filled(icon_rect, CornerRadius::same(8), theme::BG_TERTIARY);
            icons::paint(
                ui.painter(),
                Rect::from_center_size(icon_rect.center(), Vec2::splat(24.0)),
                icons::Icon::File,
                theme::TEXT_MUTED,
                1.6,
            );
            let text_x = icon_rect.right() + 12.0;
            ui.painter().galley(
                pos2(text_x, rect.top() + 18.0),
                card.name.clone(),
                theme::TEXT,
            );
            if let Some(meta) = &card.meta {
                ui.painter().galley(
                    pos2(text_x, rect.top() + 39.0),
                    meta.clone(),
                    theme::TEXT_MUTED,
                );
            }
            let response = ui
                .interact(rect, ui.scope_id().with(("file", index)), Sense::click())
                .on_hover_cursor(eframe::egui::CursorIcon::PointingHand);
            if response.clicked()
                && (card.target.starts_with("https://") || card.target.starts_with("http://"))
            {
                crate::ui::open_external(&card.target);
            }
        }
        if let Some((label, y)) = &self.footnote {
            ui.painter().text(
                origin + vec2(0.0, *y),
                Align2::LEFT_TOP,
                label,
                FontId::proportional(FOOTNOTE_SIZE),
                theme::TEXT_MUTED,
            );
        }
    }
}

/// Construit les cartes des pièces jointes non visuelles (PDF, archives…).
pub fn file_block(ui: &Ui, message: &Message) -> Option<Block> {
    let files: Vec<&Attachment> = message
        .attachments
        .iter()
        .filter(|attachment| attachment.visual_size().is_none())
        .collect();
    if files.is_empty() {
        return None;
    }
    let mut cards = Vec::new();
    let mut y = 0.0f32;
    for attachment in files {
        let name = plain_job(
            &attachment.filename,
            14.0,
            theme::TEXT,
            FILE_CARD_WIDTH - 100.0,
            Some(18.0),
        );
        let name = ui.painter().layout_job(name);
        let meta_text = attachment
            .size
            .map(human_size)
            .unwrap_or_else(|| extension_label(&attachment.filename));
        let meta = plain_job(
            &meta_text,
            12.0,
            theme::TEXT_MUTED,
            FILE_CARD_WIDTH - 100.0,
            Some(16.0),
        );
        let meta = ui.painter().layout_job(meta);
        cards.push(FileCard {
            y,
            name,
            meta: Some(meta),
            target: attachment
                .url
                .clone()
                .or_else(|| attachment.proxy_url.clone())
                .unwrap_or_default(),
        });
        y += 74.0 + 8.0;
    }
    let footnote = expiry_footnote(&message.attachments);
    let footnote_h = if footnote.is_some() { 5.6 + 12.0 } else { 0.0 };
    let height = (y - 8.0).max(0.0) + if footnote_h > 0.0 { 4.0 } else { 0.0 } + footnote_h;
    Some(Block::Files(FileBlock {
        height,
        cards,
        footnote: footnote.map(|label| (label, (y - 8.0).max(0.0) + 4.0 + 5.6)),
    }))
}

fn extension_label(filename: &str) -> String {
    filename
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_uppercase())
        .unwrap_or_else(|| "FICHIER".to_string())
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["o", "Ko", "Mo", "Go"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} o")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

// ---------------------------------------------------------------------------
// Embeds
// ---------------------------------------------------------------------------

struct AuthorLine {
    name: Arc<Galley>,
    icon: Option<String>,
    y: f32,
}

struct FieldCell {
    name: Arc<Galley>,
    value: Arc<Galley>,
    x: f32,
    y: f32,
    width: f32,
}

struct MediaSlot {
    y: f32,
    width: f32,
    height: f32,
    url: String,
    target: String,
    video: bool,
    natural: Vec2,
}

struct ThumbSlot {
    url: String,
    target: String,
}

struct FooterSlot {
    text: Arc<Galley>,
    icon: Option<String>,
    y: f32,
}

pub struct EmbedBlock {
    height: f32,
    width: f32,
    accent: Color32,
    provider: Option<(Arc<Galley>, f32)>,
    author: Option<AuthorLine>,
    title: Option<(Arc<Galley>, f32)>,
    description: Option<(Arc<Galley>, f32)>,
    fields: Vec<FieldCell>,
    media: Option<MediaSlot>,
    thumbnail: Option<(ThumbSlot, Rect)>,
    footer: Option<FooterSlot>,
    padding_top: f32,
    padding_bottom: f32,
    text_x: f32,
    text_width: f32,
}

impl EmbedBlock {
    fn draw(&self, ui: &mut Ui, origin: Pos2, _media: &str) {
        let card = Rect::from_min_size(origin, vec2(self.width, self.height));
        ui.painter().rect(
            card,
            CornerRadius::same(8),
            theme::BG_PRIMARY,
            Stroke::new(1.0, theme::CARD_BORDER),
            eframe::egui::StrokeKind::Inside,
        );
        // Bord gauche coloré (4 px) : coins arrondis uniquement à gauche.
        let bar = Rect::from_min_size(card.min + vec2(1.0, 1.0), vec2(4.0, self.height - 2.0));
        ui.painter().rect_filled(
            bar,
            CornerRadius {
                nw: 8,
                sw: 8,
                ne: 0,
                se: 0,
            },
            self.accent,
        );

        let x = origin.x + self.text_x;
        if let Some((provider, y)) = &self.provider {
            ui.painter().galley(
                pos2(x, origin.y + *y),
                provider.clone(),
                theme::TEXT_MUTED,
            );
        }
        if let Some(author) = &self.author {
            let y = origin.y + author.y;
            let mut name_x = x;
            if let Some(icon) = &author.icon {
                let rect = Rect::from_min_size(pos2(x, y), Vec2::splat(24.0));
                Image::new(icon.clone())
                    .corner_radius(CornerRadius::same(12))
                    .paint_at(ui, rect);
                name_x += 32.0;
            }
            ui.painter().galley(
                pos2(name_x, y + 3.0),
                author.name.clone(),
                theme::TEXT,
            );
        }
        if let Some((title, y)) = &self.title {
            ui.painter()
                .galley(pos2(x, origin.y + *y), title.clone(), theme::TEXT);
        }
        if let Some((description, y)) = &self.description {
            ui.painter().galley(
                pos2(x, origin.y + *y),
                description.clone(),
                theme::TEXT_SECONDARY,
            );
        }
        for field in &self.fields {
            let field_x = x + field.x;
            let field_y = origin.y + field.y;
            ui.painter().galley(
                pos2(field_x, field_y),
                field.name.clone(),
                theme::TEXT,
            );
            ui.painter().galley(
                pos2(field_x, field_y + field.name.size().y + 2.0),
                field.value.clone(),
                theme::TEXT_SECONDARY,
            );
            let _ = field.width;
        }
        if let Some(slot) = &self.media {
            let rect = Rect::from_min_size(
                pos2(x, origin.y + slot.y),
                vec2(slot.width, slot.height),
            );
            let uv = cover_uv(slot.natural, rect.size());
            Image::new(slot.url.clone())
                .uv(uv)
                .corner_radius(CornerRadius::same(4))
                .paint_at(ui, rect);
            if slot.video {
                paint_play_overlay(ui, rect, None);
            }
            let response = ui
                .interact(rect, ui.scope_id().with("embed-media"), Sense::click())
                .on_hover_cursor(eframe::egui::CursorIcon::PointingHand);
            if response.clicked()
                && (slot.target.starts_with("https://") || slot.target.starts_with("http://"))
            {
                crate::ui::open_external(&slot.target);
            }
        }
        if let Some((thumb, rect)) = &self.thumbnail {
            let rect = rect.translate(origin.to_vec2());
            Image::new(thumb.url.clone())
                .corner_radius(CornerRadius::same(4))
                .paint_at(ui, rect);
            let response = ui
                .interact(rect, ui.scope_id().with("embed-thumb"), Sense::click())
                .on_hover_cursor(eframe::egui::CursorIcon::PointingHand);
            if response.clicked()
                && (thumb.target.starts_with("https://") || thumb.target.starts_with("http://"))
            {
                crate::ui::open_external(&thumb.target);
            }
        }
        if let Some(footer) = &self.footer {
            let mut text_x = x;
            if let Some(icon) = &footer.icon {
                let icon_rect =
                    Rect::from_min_size(pos2(x, origin.y + footer.y), Vec2::splat(20.0));
                Image::new(icon.clone())
                    .corner_radius(CornerRadius::same(10))
                    .paint_at(ui, icon_rect);
                text_x += 26.0;
            }
            ui.painter().galley(
                pos2(text_x, origin.y + footer.y + 2.0),
                footer.text.clone(),
                theme::TEXT_MUTED,
            );
        }
        let _ = (self.padding_top, self.padding_bottom, self.text_width);
    }
}

/// Construit la carte d'un embed (lien enrichi, vidéo, article).
/// Construit la carte d'un embed (lien enrichi, vidéo, article).
pub fn embed_block(ui: &Ui, embed: &Embed, available: f32) -> Option<Block> {
    if !embed.has_content() {
        return None;
    }
    let has_wide_media = embed.image.is_some() || embed.video.is_some();
    let has_thumb = embed.thumbnail.is_some() && !has_wide_media;
    let width = if has_wide_media {
        EMBED_MEDIA_WIDTH.min(available)
    } else {
        EMBED_TEXT_WIDTH.min(available)
    }
    .max(160.0);

    let padding_top = 12.0;
    let padding_bottom = 14.0;
    let text_x = 17.0; // bord 1 px + barre 4 px + marge 12 px
    let mut text_width = width - text_x - 13.0;
    if has_thumb {
        text_width -= 96.0;
    }
    text_width = text_width.max(80.0);

    let accent = embed
        .border_color()
        .map(|color| {
            let value = color as u32;
            Color32::from_rgb(
                ((value >> 16) & 0xff) as u8,
                ((value >> 8) & 0xff) as u8,
                (value & 0xff) as u8,
            )
        })
        .unwrap_or(theme::ACCENT);

    // --- Construction des éléments, dans l'ordre du client officiel. ---
    let provider = embed
        .provider
        .as_ref()
        .filter(|provider| !provider.name.is_empty())
        .map(|provider| {
            let job = plain_job(
                &provider.name,
                EMBED_SMALL_SIZE,
                theme::TEXT_MUTED,
                text_width,
                Some(16.0),
            );
            ui.painter().layout_job(job)
        });

    let author_name = embed
        .author
        .as_ref()
        .filter(|author| !author.name.is_empty());
    let author = author_name.map(|author| {
        let job = plain_job(
            &author.name,
            EMBED_BODY_SIZE,
            theme::TEXT,
            text_width,
            Some(18.0),
        );
        AuthorLine {
            name: ui.painter().layout_job(job),
            icon: author.icon().map(str::to_string),
            y: 0.0,
        }
    });

    let title = embed
        .title
        .as_ref()
        .filter(|title| !title.is_empty())
        .map(|title| {
            let job = plain_job(
                title,
                EMBED_TITLE_SIZE,
                theme::TEXT,
                text_width,
                Some(22.0),
            );
            ui.painter().layout_job(job)
        });

    // YouTube affiche sa vidéo sans le pavé de description de l'API.
    let is_youtube = embed
        .provider
        .as_ref()
        .and_then(|provider| provider.url.as_deref())
        .is_some_and(|url| url.contains("youtube.com"));
    let description = embed
        .description
        .as_ref()
        .filter(|description| !description.is_empty() && !is_youtube)
        .map(|description| {
            let mut job = markdown::render(description, EMBED_BODY_SIZE, theme::TEXT_SECONDARY);
            job.wrap.max_width = text_width;
            ui.painter().layout_job(job)
        });

    let mut fields: Vec<FieldCell> = Vec::new();
    let mut fields_height = 0.0f32;
    if !embed.fields.is_empty() {
        let column = (text_width - 16.0) / 3.0;
        let mut x = 0.0f32;
        let mut y = 0.0f32;
        let mut row_height = 0.0f32;
        for field in &embed.fields {
            let cell_width = if field.inline {
                column.max(60.0)
            } else {
                text_width
            };
            if field.inline {
                if x > 0.0 && x + cell_width > text_width + 0.5 {
                    y += row_height + 8.0;
                    x = 0.0;
                    row_height = 0.0;
                }
            } else if x > 0.0 {
                y += row_height + 8.0;
                x = 0.0;
                row_height = 0.0;
            }
            let name_job = plain_job(
                &field.name,
                EMBED_BODY_SIZE,
                theme::TEXT,
                cell_width,
                Some(18.0),
            );
            let value_job = plain_job(
                &field.value,
                EMBED_BODY_SIZE,
                theme::TEXT_SECONDARY,
                cell_width,
                Some(18.0),
            );
            let name = ui.painter().layout_job(name_job);
            let value = ui.painter().layout_job(value_job);
            let height = name.size().y + 2.0 + value.size().y;
            fields.push(FieldCell {
                name,
                value,
                x,
                y,
                width: cell_width,
            });
            row_height = row_height.max(height);
            x += cell_width + 8.0;
            if x >= text_width - 1.0 {
                y += row_height + 8.0;
                x = 0.0;
                row_height = 0.0;
            }
        }
        fields_height = y + row_height;
    }

    let mut media_slot = None;
    if let Some(media) = embed.image.as_ref().or(embed.video.as_ref()) {
        let (natural_w, natural_h) = media.dimensions().unwrap_or((400.0, 225.0));
        let max_height = if natural_h > natural_w * 1.5 {
            450.0
        } else {
            300.0
        };
        let scale = (1.0f32)
            .min(400.0 / natural_w)
            .min(max_height / natural_h)
            .min(text_width / natural_w);
        let slot_w = (natural_w * scale).max(1.0);
        let slot_h = (natural_h * scale).max(1.0);
        let video = embed.video.is_some();
        let (url, target) = if video {
            let thumb = embed.thumbnail.as_ref().and_then(|thumb| thumb.src());
            (
                thumb.map(str::to_string).unwrap_or_default(),
                embed
                    .url
                    .clone()
                    .or_else(|| embed.video.as_ref().and_then(|video| video.url.clone()))
                    .unwrap_or_default(),
            )
        } else {
            (
                media.src().map(str::to_string).unwrap_or_default(),
                embed
                    .url
                    .clone()
                    .or_else(|| media.url.clone())
                    .unwrap_or_default(),
            )
        };
        media_slot = Some(MediaSlot {
            y: 0.0,
            width: slot_w,
            height: slot_h,
            url,
            target,
            video,
            natural: vec2(natural_w, natural_h),
        });
    }

    let mut thumbnail = None;
    if has_thumb && let Some(thumb) = &embed.thumbnail {
        let url = thumb.src().unwrap_or_default().to_string();
        let target = embed.url.clone().unwrap_or_default();
        thumbnail = Some((
            ThumbSlot { url, target },
            Rect::from_min_size(pos2(width - 13.0 - 80.0, padding_top + 8.0), Vec2::splat(80.0)),
        ));
    }

    let footer_text = embed
        .footer
        .as_ref()
        .map(|footer| footer.text.clone())
        .filter(|text| !text.is_empty());
    let timestamp = embed.timestamp.as_deref().and_then(short_date);
    let footer_label = match (footer_text, timestamp) {
        (Some(text), Some(date)) => Some(format!("{text} • {date}")),
        (Some(text), None) => Some(text),
        (None, Some(date)) => Some(date),
        (None, None) => None,
    };
    let footer = footer_label.map(|label| {
        let job = plain_job(
            &label,
            EMBED_SMALL_SIZE,
            theme::TEXT_MUTED,
            text_width,
            Some(16.0),
        );
        FooterSlot {
            text: ui.painter().layout_job(job),
            icon: embed
                .footer
                .as_ref()
                .and_then(|footer| footer.proxy_icon_url.clone().or(footer.icon_url.clone())),
            y: 0.0,
        }
    });

    // --- Empilement vertical (8 px entre éléments, marges officielles). ---
    let mut kinds: Vec<u8> = Vec::new();
    if provider.is_some() {
        kinds.push(0);
    }
    if author.is_some() {
        kinds.push(1);
    }
    if title.is_some() {
        kinds.push(2);
    }
    if description.is_some() {
        kinds.push(3);
    }
    if !fields.is_empty() {
        kinds.push(4);
    }
    if media_slot.is_some() {
        kinds.push(5);
    }
    if footer.is_some() {
        kinds.push(6);
    }

    let mut y = padding_top + 4.0;
    let mut positions = [0.0f32; 7];
    for (index, kind) in kinds.iter().enumerate() {
        if index > 0 {
            y += 8.0;
        }
        // `margin-top` supplémentaire des champs et des médias.
        if *kind == 4 || *kind == 5 {
            y += 8.0;
        }
        positions[*kind as usize] = y;
        let height = match *kind {
            0 => provider.as_ref().map(|galley| galley.size().y).unwrap_or(0.0),
            1 => 24.0,
            2 => title.as_ref().map(|galley| galley.size().y).unwrap_or(0.0),
            3 => description
                .as_ref()
                .map(|galley| galley.size().y)
                .unwrap_or(0.0),
            4 => fields_height,
            5 => media_slot
                .as_ref()
                .map(|slot: &MediaSlot| slot.height + 4.0)
                .unwrap_or(0.0),
            6 => 20.0,
            _ => 0.0,
        };
        y += height;
    }
    let height = y + padding_bottom;

    let provider = provider.map(|galley| (galley, positions[0]));
    let author = author.map(|mut author| {
        author.y = positions[1];
        author
    });
    let title = title.map(|galley| (galley, positions[2]));
    let description = description.map(|galley| (galley, positions[3]));
    for field in &mut fields {
        field.y += positions[4];
    }
    let media = media_slot.map(|mut slot| {
        slot.y = positions[5];
        slot
    });
    let footer = footer.map(|mut footer| {
        footer.y = positions[6];
        footer
    });

    Some(Block::Embed(EmbedBlock {
        height,
        width,
        accent,
        provider,
        author,
        title,
        description,
        fields,
        media,
        thumbnail,
        footer,
        padding_top,
        padding_bottom,
        text_x,
        text_width,
    }))
}


// ---------------------------------------------------------------------------
// Invitations
// ---------------------------------------------------------------------------

pub struct InviteBlock {
    pub code: String,
    width: f32,
    height: f32,
    state: InviteState,
}

enum InviteState {
    Loading,
    Invalid,
    Guild {
        name: String,
        icon: Option<String>,
        splash: Option<String>,
        guild_id: String,
        online: i64,
        members: i64,
        joined: bool,
        title: Arc<Galley>,
        stats_online: Arc<Galley>,
        stats_members: Arc<Galley>,
    },
}

impl InviteBlock {
    fn draw(&self, ui: &mut Ui, app: &mut FluxerApp, origin: Pos2, _media: &str) {
        let card = Rect::from_min_size(origin, vec2(self.width, self.height));
        ui.painter().rect(
            card,
            CornerRadius::same(12),
            theme::BG_PRIMARY,
            Stroke::new(1.0, theme::CARD_BORDER),
            eframe::egui::StrokeKind::Inside,
        );

        match &self.state {
            InviteState::Loading => {
                ui.painter().text(
                    card.min + vec2(14.0, 14.0),
                    Align2::LEFT_TOP,
                    "Chargement de l'invitation…",
                    FontId::proportional(13.0),
                    theme::TEXT_MUTED,
                );
            }
            InviteState::Invalid => {
                ui.painter().text(
                    card.min + vec2(14.0, 13.0),
                    Align2::LEFT_TOP,
                    "Invitation inconnue",
                    fonts::bold(16.0),
                    theme::TEXT,
                );
                ui.painter().text(
                    card.min + vec2(14.0, 36.0),
                    Align2::LEFT_TOP,
                    "Essayez de demander une nouvelle invitation.",
                    FontId::proportional(13.0),
                    theme::TEXT_MUTED,
                );
            }
            InviteState::Guild {
                name,
                icon,
                splash,
                guild_id,
                online,
                members,
                joined,
                title,
                stats_online,
                stats_members,
            } => {
                let splash_height = if splash.is_some() { 60.0 } else { 0.0 };
                if let Some(splash) = splash {
                    let splash_rect = Rect::from_min_size(card.min, vec2(self.width, 60.0));
                    Image::new(splash.clone())
                        .uv(cover_uv(vec2(3.0, 1.0), splash_rect.size()))
                        .corner_radius(CornerRadius {
                            nw: 12,
                            ne: 12,
                            sw: 0,
                            se: 0,
                        })
                        .paint_at(ui, splash_rect);
                }
                let grid_top = card.top() + splash_height;
                let icon_rect =
                    Rect::from_min_size(pos2(card.left() + 17.6, grid_top + 13.6), Vec2::splat(48.0));
                match icon {
                    Some(url) => {
                        Image::new(url.clone())
                            .corner_radius(CornerRadius::same(24))
                            .paint_at(ui, icon_rect);
                    }
                    None => {
                        ui.painter()
                            .circle_filled(icon_rect.center(), 24.0, theme::BG_TERTIARY);
                        ui.painter().text(
                            icon_rect.center(),
                            Align2::CENTER_CENTER,
                            initials(name),
                            fonts::bold(18.0),
                            theme::TEXT,
                        );
                    }
                }
                let text_x = card.left() + 84.0;
                ui.painter()
                    .galley(pos2(text_x, grid_top + 17.6), title.clone(), theme::TEXT);
                let stats_y = grid_top + 41.6;
                ui.painter().circle_filled(
                    pos2(text_x + 4.0, stats_y + 6.0),
                    4.0,
                    theme::ONLINE,
                );
                ui.painter().galley(
                    pos2(text_x + 12.8, stats_y),
                    stats_online.clone(),
                    theme::TEXT_MUTED,
                );
                let members_x = text_x + 12.8 + stats_online.size().x + 9.6;
                ui.painter().circle_filled(
                    pos2(members_x + 4.0, stats_y + 6.0),
                    4.0,
                    theme::TEXT_MUTED,
                );
                ui.painter().galley(
                    pos2(members_x + 12.8, stats_y),
                    stats_members.clone(),
                    theme::TEXT_MUTED,
                );
                let _ = (online, members);

                let divider_y = grid_top + 75.2;
                ui.painter().line_segment(
                    [
                        pos2(card.left() + 1.0, divider_y),
                        pos2(card.right() - 1.0, divider_y),
                    ],
                    Stroke::new(1.0, theme::CARD_BORDER),
                );
                let button = Rect::from_min_size(
                    pos2(card.left() + 13.6, divider_y + 13.6),
                    vec2(self.width - 27.2, 36.0),
                );
                let hovered = ui.rect_contains_pointer(button);
                let fill = if hovered {
                    theme::ACCENT_HOVER
                } else {
                    theme::ACCENT
                };
                ui.painter()
                    .rect_filled(button, CornerRadius::same(6), fill);
                let label = if *joined {
                    "Aller à la communauté"
                } else {
                    "Rejoindre la communauté"
                };
                ui.painter().text(
                    button.center(),
                    Align2::CENTER_CENTER,
                    label,
                    fonts::bold(14.0),
                    Color32::WHITE,
                );
                let response = ui
                    .interact(button, ui.scope_id().with(("invite", &self.code)), Sense::click())
                    .on_hover_cursor(eframe::egui::CursorIcon::PointingHand);
                if response.clicked() {
                    if *joined {
                        app.view = crate::app::View::Guilds;
                        app.command(Command::SelectGuild(guild_id.clone()));
                    } else {
                        app.command(Command::JoinInvite {
                            code: self.code.clone(),
                        });
                    }
                }
            }
        }
    }
}

fn initials(name: &str) -> String {
    let mut words = name.split_whitespace().filter_map(|word| word.chars().next());
    match (words.next(), words.next()) {
        (Some(a), Some(b)) => format!("{a}{b}").to_uppercase(),
        (Some(a), None) => a.to_uppercase().to_string(),
        _ => "?".into(),
    }
}

fn format_count(value: i64) -> String {
    let digits = value.max(0).to_string();
    let mut out = String::new();
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push('\u{202f}');
        }
        out.push(character);
    }
    out
}

pub fn invite_block(
    ui: &Ui,
    code: &str,
    cached: Option<&Option<InvitePreview>>,
    joined_guilds: &HashSet<String>,
    media: &str,
    available: f32,
) -> Block {
    let width = INVITE_CARD_WIDTH.min(available).max(160.0);
    let state = match cached {
        None => InviteState::Loading,
        Some(None) => InviteState::Invalid,
        Some(Some(invite)) => match &invite.guild {
            Some(guild) => {
                let title_job = plain_job(
                    &guild.name,
                    EMBED_TITLE_SIZE,
                    theme::TEXT,
                    width - 97.6,
                    Some(20.0),
                );
                let online_job = plain_job(
                    &format!("{} en ligne", format_count(invite.presence_count)),
                    13.0,
                    theme::TEXT_MUTED,
                    140.0,
                    Some(16.0),
                );
                let members_job = plain_job(
                    &format!("{} membres", format_count(invite.member_count)),
                    13.0,
                    theme::TEXT_MUTED,
                    150.0,
                    Some(16.0),
                );
                InviteState::Guild {
                    name: guild.name.clone(),
                    icon: guild.icon_url(media),
                    splash: guild.splash_url(media),
                    guild_id: guild.id.clone(),
                    online: invite.presence_count,
                    members: invite.member_count,
                    joined: joined_guilds.contains(&guild.id),
                    title: ui.painter().layout_job(title_job),
                    stats_online: ui.painter().layout_job(online_job),
                    stats_members: ui.painter().layout_job(members_job),
                }
            }
            None => InviteState::Invalid,
        },
    };
    let height = match &state {
        InviteState::Guild { splash, .. } => {
            (if splash.is_some() { 60.0 } else { 0.0 }) + 14.0 + 48.0 + 14.0 + 1.0 + 13.6 + 36.0 + 13.6
        }
        _ => 76.0,
    };
    Block::Invite(InviteBlock {
        code: code.to_string(),
        width,
        height,
        state,
    })
}

// ---------------------------------------------------------------------------
// Outils texte / dates
// ---------------------------------------------------------------------------

pub fn plain_job(
    text: &str,
    size: f32,
    color: Color32,
    wrap: f32,
    line_height: Option<f32>,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap.max(1.0);
    job.append(
        text,
        0.0,
        eframe::egui::TextFormat {
            font_id: FontId::proportional(size),
            color,
            line_height,
            ..Default::default()
        },
    );
    job
}

/// Date courte française : « 23 oct. 2029 ».
pub fn short_date(iso: &str) -> Option<String> {
    const MONTHS: [&str; 12] = [
        "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.",
        "nov.", "déc.",
    ];
    let parsed = chrono::DateTime::parse_from_rfc3339(iso).ok()?;
    let local = parsed.with_timezone(&chrono::Local);
    use chrono::Datelike;
    Some(format!(
        "{} {} {}",
        local.day(),
        MONTHS[(local.month0() as usize).min(11)],
        local.year()
    ))
}

/// Légende d'expiration agrégée (la plus proche) des pièces jointes.
fn expiry_footnote(attachments: &[Attachment]) -> Option<String> {
    let mut best: Option<(chrono::DateTime<chrono::FixedOffset>, &str)> = None;
    for attachment in attachments {
        let Some(raw) = attachment.expires_at.as_deref() else {
            continue;
        };
        let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(raw) else {
            continue;
        };
        if best.is_none_or(|(current, _)| parsed < current) {
            best = Some((parsed, raw));
        }
    }
    let (when, raw) = best?;
    let date = short_date(raw)?;
    if when < chrono::Utc::now() {
        Some(format!("Expiré le {date}"))
    } else {
        Some(format!("Expire le {date}"))
    }
}

/// Construit tous les blocs riches d'un message, dans l'ordre officiel.
pub fn plan(
    ui: &Ui,
    message: &Message,
    available: f32,
    invites: &HashMap<String, Option<InvitePreview>>,
    pending: &HashSet<String>,
    joined_guilds: &HashSet<String>,
    media: &str,
) -> Vec<Block> {
    let mut blocks = Vec::new();
    if let Some(block) = media_block(message, available) {
        blocks.push(block);
    }
    if let Some(block) = file_block(ui, message) {
        blocks.push(block);
    }
    for embed in &message.embeds {
        if let Some(block) = embed_block(ui, embed, available) {
            blocks.push(block);
        }
    }
    for code in crate::app::invite_codes(&message.content) {
        let cached = invites.get(&code);
        if cached.is_none() && !pending.contains(&code) {
            continue;
        }
        blocks.push(invite_block(
            ui,
            &code,
            cached,
            joined_guilds,
            media,
            available,
        ));
    }
    blocks
}
