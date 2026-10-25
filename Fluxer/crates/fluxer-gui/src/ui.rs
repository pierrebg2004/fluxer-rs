// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! Entire egui layout, drawn pixel by pixel in Rust.
//!
//! Structure mirrors the official Fluxer desktop client:
//!
//! ```text
//! ┌────┬──────────────┬──────────────────────────────┬──────────┐
//! │rail│ channel list │ header (salon + recherche)    │ membres  │
//! │72px│    320px     ├──────────────────────────────┤  264px   │
//! │    │              │ timeline                     │          │
//! │    │              ├──────────────────────────────┤          │
//! │    │ user panel   │ composer                     │          │
//! └────┴──────────────┴──────────────────────────────┴──────────┘
//! ```
//!
//! Sizes and colours come from the shipped stylesheet (see `theme.rs`).

use std::collections::{HashMap, HashSet};

use eframe::egui::{
    self, Align, Align2, Button, Color32, CornerRadius, CursorIcon, FontId, Frame, Image, Key,
    Layout, Margin, Rect, Response, RichText, ScrollArea, Sense, Stroke, TextEdit, Ui, UiBuilder,
    Vec2, pos2, vec2,
};
use fluxer_core::{Channel, ChannelKind, Guild, Member, Message, User};

use crate::app::{FluxerApp, View};
use crate::engine::{Command, GatewayStatus};
use crate::fonts;
use crate::icons::{self, Icon};
use crate::markdown;
use crate::rich;
use crate::theme;

const TEXT_SIZE: f32 = 15.5;
const NAME_SIZE: f32 = 15.5;
const TIME_SIZE: f32 = 11.5;
const RAIL_WIDTH: f32 = theme::LAYOUT_RAIL_WIDTH;
const SIDEBAR_WIDTH: f32 = theme::LAYOUT_SIDEBAR_WIDTH;

pub fn draw(app: &mut FluxerApp, ui: &mut Ui) {
    if app.state.me.is_none() {
        login_screen(app, ui);
    } else {
        main_screen(app, ui);
    }

    if app.show_settings {
        settings_window(app, ui.ctx());
    }

    delete_confirm_window(app, ui.ctx());
    join_guild_window(app, ui.ctx());
    profile_popup(app, ui.ctx());
    emoji_picker_window(app, ui.ctx());
    inbox_window(app, ui.ctx());
}

/// Carte de profil affichée au clic sur un membre, comme dans le client
/// officiel : avatar, nom, identifiant, pronoms, bio et rôles.
fn profile_popup(app: &mut FluxerApp, ctx: &egui::Context) {
    let Some(user) = app.profile_popup.clone() else {
        return;
    };
    if ctx.input(|i| i.key_pressed(Key::Escape)) {
        app.profile_popup = None;
        return;
    }

    let media = app.state.media_base.clone();
    let member = app
        .state
        .members
        .iter()
        .find(|member| member.user.id == user.id)
        .cloned();
    let mut role_names: Vec<(String, i64)> = Vec::new();
    if let (Some(member), Some(detail)) = (&member, &app.state.detail) {
        for role_id in &member.roles {
            if let Some(role) = detail.roles.iter().find(|role| &role.id == role_id)
                && !role.name.is_empty()
                && role.name != "@everyone"
            {
                role_names.push((role.name.clone(), role.color));
            }
        }
    }

    egui::Window::new("Carte de profil")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .fixed_size(vec2(330.0, 330.0))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .frame(
            Frame::default()
                .fill(theme::BG_SIDEBAR)
                .corner_radius(CornerRadius::same(12))
                .stroke(Stroke::new(1.0, theme::BORDER))
                .inner_margin(Margin::same(18)),
        )
        .show(ctx, |ui| {
            ui.set_min_width(330.0);
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                if icon_button(ui, Icon::Close, 22.0, theme::TEXT_MUTED).clicked() {
                    app.profile_popup = None;
                }
            });
            let (avatar_area, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), 78.0), Sense::hover());
            paint_avatar(
                ui,
                Rect::from_center_size(avatar_area.center(), Vec2::splat(76.0)),
                &user,
                &media,
                None,
                theme::BG_SIDEBAR,
            );
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(user.display_name())
                        .font(fonts::bold(18.0))
                        .color(theme::TEXT),
                );
                let discriminator = user
                    .discriminator
                    .as_deref()
                    .filter(|value| !value.is_empty() && *value != "0")
                    .map(|value| format!("#{value}"))
                    .unwrap_or_default();
                ui.label(
                    RichText::new(format!("@{}{discriminator}", user.username))
                        .size(12.0)
                        .color(theme::TEXT_MUTED),
                );
            });
            if let Some(pronouns) = user.pronouns.as_deref().filter(|p| !p.is_empty()) {
                ui.add_space(4.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(pronouns).size(11.5).color(theme::TEXT_MUTED));
                });
            }
            ui.add_space(10.0);
            settings_separator(ui);
            ui.add_space(8.0);
            if let Some(bio) = user.bio.as_deref().filter(|bio| !bio.trim().is_empty()) {
                ui.label(RichText::new(bio).size(12.0).color(theme::TEXT));
                ui.add_space(8.0);
            } else {
                ui.label(
                    RichText::new("Aucune bio.")
                        .size(12.0)
                        .color(theme::TEXT_FAINT),
                );
                ui.add_space(8.0);
            }
            if let Some(joined) = member
                .as_ref()
                .and_then(|member| member.joined_at.as_deref())
                .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            {
                ui.label(
                    RichText::new(format!("Membre depuis le {}", joined.format("%d/%m/%Y")))
                        .size(11.0)
                        .color(theme::TEXT_FAINT),
                );
                ui.add_space(6.0);
            }
            if !role_names.is_empty() {
                ui.label(
                    RichText::new("Rôles")
                        .font(fonts::bold(11.0))
                        .color(theme::TEXT_MUTED),
                );
                ui.horizontal_wrapped(|ui| {
                    for (name, color) in &role_names {
                        let color = theme::role_color(*color).unwrap_or(theme::TEXT_MUTED);
                        Frame::default()
                            .fill(theme::BG_COMPOSER)
                            .corner_radius(CornerRadius::same(4))
                            .stroke(Stroke::new(1.0, color.gamma_multiply(0.6)))
                            .inner_margin(Margin::symmetric(6, 3))
                            .show(ui, |ui| {
                                ui.label(RichText::new(name).size(11.0).color(color));
                            });
                    }
                });
            }
        });
}

/// Modale de confirmation avant suppression définitive d'un message.
///
/// La suppression est une action destructive et irréversible côté serveur :
/// on demande toujours une confirmation explicite, comme le client officiel.
fn delete_confirm_window(app: &mut FluxerApp, ctx: &egui::Context) {
    if app.confirm_delete.is_none() {
        return;
    }

    if ctx.input(|i| i.key_pressed(Key::Escape)) {
        app.confirm_delete = None;
        return;
    }

    // Voile sombre pour isoler la modale du reste de l'interface.
    let screen = ctx.content_rect();
    let painter_layer = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::unique("delete_confirm_veil"),
    ));
    painter_layer.rect_filled(screen, 0.0, Color32::from_rgba_premultiplied(0, 0, 0, 140));

    egui::Window::new("Supprimer le message")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .fixed_size(vec2(420.0, 190.0))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .frame(
            Frame::default()
                .fill(theme::BG_SIDEBAR)
                .corner_radius(CornerRadius::same(10))
                .stroke(Stroke::new(1.0, theme::BORDER))
                .inner_margin(Margin::same(20)),
        )
        .show(ctx, |ui| {
            ui.set_min_width(420.0);
            ui.label(
                RichText::new("Supprimer le message ?")
                    .font(fonts::bold(16.0))
                    .color(theme::TEXT),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new(
                    "Cette action est définitive : le message sera supprimé pour tout le monde et ne pourra pas être restauré.",
                )
                .size(12.5)
                .color(theme::TEXT_MUTED),
            );
            ui.add_space(18.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add(
                        Button::new(
                            RichText::new("Supprimer")
                                .color(Color32::WHITE)
                                .font(fonts::bold(12.5)),
                        )
                        .fill(theme::DANGER)
                        .corner_radius(CornerRadius::same(5)),
                    )
                    .clicked()
                {
                    app.confirm_delete_message();
                }
                ui.add_space(6.0);
                if ui
                    .add(
                        Button::new(RichText::new("Annuler").size(12.5))
                            .corner_radius(CornerRadius::same(5)),
                    )
                    .clicked()
                {
                    app.confirm_delete = None;
                }
            });
        });
}

// ---------------------------------------------------------------------------
// Rejoindre un serveur
// ---------------------------------------------------------------------------

fn join_guild_window(app: &mut FluxerApp, ctx: &egui::Context) {
    if !app.show_join_guild {
        return;
    }
    if ctx.input(|i| i.key_pressed(Key::Escape)) {
        app.show_join_guild = false;
        app.join_busy = false;
        return;
    }

    let screen = ctx.content_rect();
    let veil = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::unique("join_guild_veil"),
    ));
    veil.rect_filled(screen, 0.0, Color32::from_rgba_premultiplied(0, 0, 0, 140));

    let submitted = egui::Window::new("Rejoindre un serveur")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .fixed_size(vec2(440.0, 208.0))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .frame(
            Frame::default()
                .fill(theme::BG_SIDEBAR)
                .corner_radius(CornerRadius::same(10))
                .stroke(Stroke::new(1.0, theme::BORDER))
                .inner_margin(Margin::same(20)),
        )
        .show(ctx, |ui| {
            ui.set_min_width(440.0);
            ui.label(
                RichText::new("Rejoindre un serveur")
                    .font(fonts::bold(16.0))
                    .color(theme::TEXT),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "Collez une invitation Fluxer (https://fluxer.gg/…) ou son code pour rejoindre une communauté.",
                )
                .size(12.5)
                .color(theme::TEXT_MUTED),
            );
            ui.add_space(12.0);
            let response = ui.add_sized(
                [440.0, 34.0],
                TextEdit::singleline(&mut app.join_invite)
                    .hint_text("https://fluxer.gg/abcd1234")
                    .interactive(!app.join_busy),
            );
            response.request_focus();
            let enter = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            ui.add_space(16.0);
            let mut submit = enter;
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !app.join_busy && !app.join_invite.trim().is_empty(),
                        Button::new(
                            RichText::new(if app.join_busy {
                                "Connexion…"
                            } else {
                                "Rejoindre"
                            })
                            .color(Color32::WHITE)
                            .font(fonts::bold(12.5)),
                        )
                        .fill(theme::ACCENT)
                        .corner_radius(CornerRadius::same(5)),
                    )
                    .clicked()
                {
                    submit = true;
                }
                ui.add_space(6.0);
                if ui
                    .add(
                        Button::new(RichText::new("Annuler").size(12.5))
                            .corner_radius(CornerRadius::same(5)),
                    )
                    .clicked()
                {
                    app.show_join_guild = false;
                    app.join_busy = false;
                }
            });
            submit
        })
        .and_then(|response| response.inner)
        .unwrap_or(false);

    if submitted && !app.join_busy {
        app.submit_invite();
    }
}

// ---------------------------------------------------------------------------
// Login
// ---------------------------------------------------------------------------

fn login_screen(app: &mut FluxerApp, ui: &mut Ui) {
    egui::CentralPanel::default()
        .frame(Frame::default().fill(theme::BG_PRIMARY))
        .show(ui, |ui| {
            let full = ui.max_rect();
            let painter = ui.painter();
            // Soft brand glow, like the ambient gradient of the web login.
            painter.circle_filled(
                pos2(full.center().x - 260.0, full.top() + 120.0),
                360.0,
                Color32::from_rgba_premultiplied(0x2a, 0x26, 0x8a, 22),
            );
            painter.circle_filled(
                pos2(full.center().x + 320.0, full.bottom() - 40.0),
                300.0,
                Color32::from_rgba_premultiplied(0x1b, 0x3d, 0x8a, 16),
            );

            ui.vertical_centered(|ui| {
                ui.add_space((full.height() * 0.10).max(18.0));

                Frame::default()
                    .fill(Color32::from_rgb(0x11, 0x11, 0x14))
                    .corner_radius(CornerRadius::same(16))
                    .stroke(Stroke::new(1.0, theme::BORDER))
                    .inner_margin(Margin::same(32))
                    .show(ui, |ui| {
                        ui.set_width(408.0);
                        ui.vertical_centered(|ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(Vec2::splat(64.0), Sense::hover());
                            paint_logo(ui, rect);
                            ui.add_space(14.0);
                            ui.label(
                                RichText::new("Bon retour !")
                                    .font(fonts::bold(25.0))
                                    .color(theme::TEXT),
                            );
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(
                                    "Connectez-vous pour retrouver vos serveurs et vos messages.",
                                )
                                .size(14.0)
                                .color(theme::TEXT_MUTED),
                            );
                        });

                        ui.add_space(22.0);

                        field_label(ui, "Adresse électronique ou jeton");
                        Frame::default()
                            .fill(theme::BG_COMPOSER)
                            .corner_radius(CornerRadius::same(8))
                            .stroke(Stroke::new(1.0, theme::BORDER))
                            .inner_margin(Margin::symmetric(12, 9))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let eye = app.login.show_token;
                                    let response = ui.add(
                                        TextEdit::singleline(&mut app.login.token)
                                            .password(!eye)
                                            .hint_text("Collez votre jeton de compte")
                                            .frame(Frame::NONE)
                                            .desired_width(f32::INFINITY)
                                            .text_color(theme::TEXT),
                                    );
                                    let _ = response;
                                    if icon_button(
                                        ui,
                                        Icon::Eye,
                                        24.0,
                                        if eye {
                                            theme::ACCENT_HOVER
                                        } else {
                                            theme::TEXT_FAINT
                                        },
                                    )
                                    .on_hover_text("Afficher le jeton")
                                    .clicked()
                                    {
                                        app.login.show_token = !app.login.show_token;
                                    }
                                });
                            });

                        ui.add_space(10.0);
                        let advanced_label = if app.login.advanced {
                            "Masquer l'instance personnalisée"
                        } else {
                            "Utiliser une instance personnalisée"
                        };
                        ui.horizontal(|ui| {
                            ui.add(
                                Button::new(
                                    RichText::new(advanced_label)
                                        .size(12.5)
                                        .color(theme::TEXT_MUTED),
                                )
                                .frame(false),
                            )
                            .clicked()
                            .then(|| app.login.advanced = !app.login.advanced);
                        });
                        if app.login.advanced {
                            Frame::default()
                                .fill(theme::BG_COMPOSER)
                                .corner_radius(CornerRadius::same(8))
                                .stroke(Stroke::new(1.0, theme::BORDER))
                                .inner_margin(Margin::symmetric(12, 9))
                                .show(ui, |ui| {
                                    ui.add(
                                        TextEdit::singleline(&mut app.login.instance)
                                            .hint_text("https://web.fluxer.app")
                                            .frame(Frame::NONE)
                                            .desired_width(f32::INFINITY),
                                    );
                                });
                        }

                        ui.add_space(18.0);
                        let can_submit = !app.login.token.trim().is_empty() && !app.login.busy;
                        if ui
                            .add_enabled(
                                can_submit,
                                Button::new(
                                    RichText::new(if app.login.busy {
                                        "Connexion…"
                                    } else {
                                        "Se connecter"
                                    })
                                    .font(fonts::bold(15.0)),
                                )
                                .fill(theme::ACCENT)
                                .corner_radius(CornerRadius::same(8))
                                .min_size(Vec2::new(ui.available_width(), 42.0)),
                            )
                            .clicked()
                        {
                            app.login.busy = true;
                            app.state.error = None;
                            app.command(Command::Login {
                                instance: app.login.instance.clone(),
                                token: app.login.token.trim().to_string(),
                            });
                        }

                        ui.add_space(8.0);
                        if ui
                            .add_enabled(
                                !app.login.busy,
                                Button::new(
                                    RichText::new("Importer la session Fluxer installée")
                                        .size(13.5),
                                )
                                .fill(theme::BG_COMPOSER)
                                .corner_radius(CornerRadius::same(8))
                                .min_size(Vec2::new(ui.available_width(), 36.0)),
                            )
                            .clicked()
                        {
                            app.login.busy = true;
                            app.state.error = None;
                            app.command(Command::ImportInstalled);
                        }

                        if app.login.busy {
                            ui.add_space(12.0);
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(
                                    RichText::new(&app.state.status)
                                        .size(12.5)
                                        .color(theme::TEXT_MUTED),
                                );
                            });
                        }

                        if let Some(error) = app.state.error.clone() {
                            ui.add_space(10.0);
                            Frame::default()
                                .fill(Color32::from_rgba_premultiplied(0x54, 0x18, 0x18, 120))
                                .corner_radius(CornerRadius::same(6))
                                .inner_margin(Margin::symmetric(10, 8))
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(error)
                                            .size(12.5)
                                            .color(Color32::from_rgb(0xff, 0xb3, 0xb3)),
                                    );
                                });
                        }
                    });

                ui.add_space(14.0);
                ui.label(
                    RichText::new(
                        "Jeton stocké localement · ~/.config/fluxer-rs/config.json (0600)",
                    )
                    .size(11.5)
                    .color(theme::TEXT_FAINT),
                );
            });
        });
}

fn field_label(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text.to_uppercase())
            .font(fonts::bold(10.5))
            .color(theme::TEXT_MUTED),
    );
    ui.add_space(5.0);
}

fn paint_logo(ui: &Ui, rect: Rect) {
    let painter = ui.painter();
    let size = rect.width();
    painter.circle_filled(rect.center(), size / 2.0, theme::ACCENT);
    // Les deux vaguelettes du logo officiel (voir fluxer-logo-color.svg).
    let thickness = size * 0.098;
    for center_y in [0.417_f32, 0.688_f32] {
        let mut points = Vec::with_capacity(41);
        for step in 0..=40 {
            let t = step as f32 / 40.0;
            let x = 0.198 + 0.604 * t;
            let y = center_y + 0.046 * (t * std::f32::consts::TAU).sin();
            points.push(pos2(rect.left() + x * size, rect.top() + y * size));
        }
        painter.add(egui::Shape::line(
            points.clone(),
            Stroke::new(thickness, Color32::WHITE),
        ));
        // Extrémités arrondies, comme les rubans du SVG.
        painter.circle_filled(points[0], thickness / 2.0, Color32::WHITE);
        painter.circle_filled(
            *points.last().unwrap_or(&points[0]),
            thickness / 2.0,
            Color32::WHITE,
        );
    }
}

// ---------------------------------------------------------------------------
// Main window
// ---------------------------------------------------------------------------

fn main_screen(app: &mut FluxerApp, ui: &mut Ui) {
    left_nav(app, ui);
    match app.view {
        View::Guilds => {
            let on_voice_channel = app.state.voice_channel.is_some()
                && app.state.voice_channel == app.state.current_channel;
            if app.state.show_members && app.state.current_guild.is_some() && !on_voice_channel {
                member_list(app, ui);
            }
            chat_header(app, ui);
            composer(app, ui);
            if on_voice_channel {
                voice_call(app, ui);
            } else {
                chat(app, ui);
            }
        }
        View::Dms => {
            if app.state.current_channel.is_some() {
                chat_header(app, ui);
                composer(app, ui);
                chat(app, ui);
            } else {
                dm_home(app, ui);
            }
        }
        View::Favorites => {
            favorites_header(app, ui);
            favorites_view(app, ui);
        }
        View::Discovery => {
            discovery_page(app, ui);
        }
    }
}

// --- colonne de gauche (rail + barre latérale + zone utilisateur) ----------

/// Le client officiel place la zone utilisateur dans un unique panneau qui
/// couvre **à la fois** le rail des serveurs et la barre latérale. On reproduit
/// cette structure pour que l'avatar commence dans le rail, comme sur Fluxer.
fn left_nav(app: &mut FluxerApp, ui: &mut Ui) {
    egui::Panel::left("left_nav")
        .default_size(RAIL_WIDTH + SIDEBAR_WIDTH)
        .min_size(RAIL_WIDTH + SIDEBAR_WIDTH)
        .max_size(RAIL_WIDTH + SIDEBAR_WIDTH)
        .resizable(false)
        .frame(Frame::default().fill(theme::BG_SIDEBAR))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;

            // Pied de colonne : 58 px de contenu + 7 px d'inset bas.
            egui::Panel::bottom("left_footer")
                .default_size(theme::LAYOUT_USER_AREA_HEIGHT + 7.0)
                .min_size(theme::LAYOUT_USER_AREA_HEIGHT + 7.0)
                .max_size(theme::LAYOUT_USER_AREA_HEIGHT + 7.0)
                .resizable(false)
                .frame(
                    Frame::default()
                        .fill(theme::BG_SIDEBAR)
                        .inner_margin(Margin {
                            left: 6,
                            right: 6,
                            top: 0,
                            bottom: 7,
                        }),
                )
                .show(ui, |ui| {
                    Frame::default()
                        .fill(theme::BG_PANEL_CONTROL)
                        .corner_radius(CornerRadius::same(8))
                        .stroke(Stroke::new(1.0, theme::BORDER))
                        .inner_margin(Margin {
                            left: 14,
                            right: 8,
                            top: 13,
                            bottom: 13,
                        })
                        .show(ui, |ui| {
                            ui.set_min_height(32.0);
                            user_panel(app, ui);
                        });
                });

            egui::Panel::left("guild_rail")
                .default_size(RAIL_WIDTH)
                .min_size(RAIL_WIDTH)
                .max_size(RAIL_WIDTH)
                .resizable(false)
                .frame(
                    Frame::default()
                        .fill(theme::BG_RAIL)
                        .inner_margin(Margin::symmetric(0, 8)),
                )
                .show(ui, |ui| rail_contents(app, ui));

            match app.view {
                View::Guilds => channel_sidebar(app, ui),
                View::Dms => dm_sidebar(app, ui),
                View::Favorites => favorites_sidebar(app, ui),
                View::Discovery => discovery_sidebar(app, ui),
            }
        });
}

// --- guild rail ------------------------------------------------------------

fn rail_contents(app: &mut FluxerApp, ui: &mut Ui) {
    ui.spacing_mut().item_spacing.y = 0.0;

    let guilds = app.state.guilds.clone();
    let media = app.state.media_base.clone();
    let selected_guild = app.state.current_guild.clone();
    let selected_channel = app.state.current_channel.clone();
    let unread = app.unread.clone();
    let view = app.view;
    let mut dms: Vec<Channel> = app
        .state
        .private_channels
        .iter()
        .filter(|channel| {
            unread.get(&channel.id).copied().unwrap_or(0) > 0
                || (view == View::Dms && selected_channel.as_deref() == Some(channel.id.as_str()))
        })
        .cloned()
        .collect();
    dms.sort_by(|a, b| b.last_message_id.cmp(&a.last_message_id));

    let mut go_dms = false;
    let mut go_favorites = false;
    let mut go_discovery = false;
    let mut open_join = false;
    let mut open_help = false;
    let mut clicked_guild = None;
    let mut clicked_dm = None;

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(4.0);

            // Accueil : le logo ouvre les messages privés, comme le
            // bouton « Fluxer » du client officiel.
            if rail_logo(ui, view == View::Dms)
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_text("Messages privés")
                .clicked()
            {
                go_dms = true;
            }
            ui.add_space(6.0);

            if rail_action(ui, Icon::Star, "Messages favoris", view == View::Favorites).clicked() {
                go_favorites = true;
            }
            if !dms.is_empty() {
                ui.add_space(6.0);
            }

            for channel in &dms {
                let selected =
                    view == View::Dms && selected_channel.as_deref() == Some(channel.id.as_str());
                if rail_dm(ui, channel, &media, selected).clicked() {
                    clicked_dm = Some(channel.id.clone());
                }
            }

            rail_divider(ui);
            for guild in &guilds {
                let selected =
                    view == View::Guilds && selected_guild.as_deref() == Some(guild.id.as_str());
                if let Some(id) = rail_guild(ui, guild, &media, selected) {
                    clicked_guild = Some(id);
                }
                ui.add_space(6.0);
            }

            ui.add_space(6.0);
            if rail_action(
                ui,
                Icon::Compass,
                "Explorer les communautés",
                view == View::Discovery,
            )
            .clicked()
            {
                go_discovery = true;
            }
            ui.add_space(6.0);
            if rail_action(ui, Icon::Plus, "Rejoindre un serveur", false).clicked() {
                open_join = true;
            }
            ui.add_space(6.0);
            if rail_action(ui, Icon::Help, "Centre d'aide", false).clicked() {
                open_help = true;
            }
            ui.add_space(8.0);
        });

    if go_dms {
        app.view = View::Dms;
        app.search.clear();
        let already_dm = current_channel(app)
            .map(|channel| channel.kind().is_direct())
            .unwrap_or(false);
        if !already_dm
            && let Some(first) = app
                .state
                .private_channels
                .iter()
                .max_by(|a, b| a.last_message_id.cmp(&b.last_message_id))
        {
            app.command(Command::SelectDm(first.id.clone()));
        }
    }
    if go_favorites {
        app.view = View::Favorites;
        app.search.clear();
    }
    if go_discovery {
        app.view = View::Discovery;
        app.search.clear();
        app.command(Command::LoadDiscovery { offset: 0 });
    }
    if open_join {
        app.join_busy = false;
        app.show_join_guild = true;
    }
    if open_help && !open_external("https://fluxer.app/help") {
        app.state.error = Some("Impossible d'ouvrir le centre d'aide.".into());
    }
    if let Some(id) = clicked_guild {
        app.view = View::Guilds;
        app.search.clear();
        app.command(Command::SelectGuild(id));
    }
    if let Some(id) = clicked_dm {
        app.view = View::Dms;
        app.search.clear();
        app.command(Command::SelectDm(id));
    }
}

/// Dessine la barre d'indication à gauche d'une ligne de navigation.
fn rail_indicator(ui: &Ui, row: Rect, selected: bool, hovered: bool) {
    // `resolveGuildListIndicatorHeight` : 40 px sélectionné, 20 px survol.
    let height = if selected {
        40.0
    } else if hovered {
        20.0
    } else {
        return;
    };
    // Barre de 0.35rem, bords droits arrondis, collée au bord gauche.
    ui.painter().rect_filled(
        Rect::from_min_size(
            pos2(row.left() - 1.0, row.center().y - height / 2.0),
            vec2(5.6, height),
        ),
        CornerRadius {
            nw: 0,
            sw: 0,
            ne: 3,
            se: 3,
        },
        Color32::WHITE,
    );
}

/// Logo Fluxer en haut de la barre.
fn rail_logo(ui: &mut Ui, selected: bool) -> Response {
    let (row, response) = ui.allocate_exact_size(vec2(RAIL_WIDTH, 48.0), Sense::click());
    rail_indicator(ui, row, selected, response.hovered());
    let icon = Rect::from_center_size(row.center(), Vec2::splat(theme::GUILD_ICON_SIZE));
    paint_logo(ui, icon);
    response
}

/// Bouton d'action circulaire de la barre (favoris, découverte, aide…).
fn rail_action(ui: &mut Ui, icon: Icon, label: &str, selected: bool) -> Response {
    let (row, response) = ui.allocate_exact_size(vec2(RAIL_WIDTH, 48.0), Sense::click());
    let hovered = response.hovered();
    rail_indicator(ui, row, selected, hovered);
    let rect = Rect::from_center_size(row.center(), Vec2::splat(theme::GUILD_ICON_SIZE));
    let radius = if selected || hovered { 13 } else { 22 };
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, CornerRadius::same(radius), theme::ACCENT);
    } else if hovered {
        painter.rect_filled(rect, CornerRadius::same(radius), theme::BG_HOVER);
    }
    let border = if selected || hovered {
        theme::TEXT
    } else {
        Color32::from_rgba_premultiplied(38, 38, 38, 38)
    };
    // Cercle pointillé, comme les boutons secondaires du rail officiel.
    let points: Vec<_> = (0..=72)
        .map(|step| {
            let angle = step as f32 * std::f32::consts::TAU / 72.0;
            pos2(
                rect.center().x + 21.5 * angle.cos(),
                rect.center().y + 21.5 * angle.sin(),
            )
        })
        .collect();
    ui.painter().extend(egui::Shape::dashed_line(
        &points,
        Stroke::new(1.4, border),
        3.0,
        2.4,
    ));
    let tint = if selected {
        Color32::WHITE
    } else if hovered {
        theme::TEXT
    } else {
        theme::TEXT_MUTED
    };
    icons::paint(ui.painter(), rect.shrink(12.0), icon, tint, 1.8);
    response
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(label)
}

/// Avatar d'une conversation privée non lue sur la barre de gauche.
fn rail_dm(ui: &mut Ui, channel: &Channel, media: &str, selected: bool) -> Response {
    let (row, response) = ui.allocate_exact_size(vec2(RAIL_WIDTH, 48.0), Sense::click());
    let hovered = response.hovered();
    rail_indicator(ui, row, selected, hovered);
    let rect = Rect::from_center_size(row.center(), Vec2::splat(theme::GUILD_ICON_SIZE));
    let radius = if selected || hovered { 13 } else { 22 };
    if let Some(user) = channel.dm_user() {
        match user.avatar_url(media) {
            Some(url) => {
                Image::new(url)
                    .fit_to_exact_size(rect.size())
                    .maintain_aspect_ratio(false)
                    .corner_radius(CornerRadius::same(radius))
                    .paint_at(ui, rect);
            }
            None => {
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(radius),
                    theme::packed_color(user.avatar_color, &user.id),
                );
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    user.initials(),
                    fonts::bold(15.0),
                    Color32::WHITE,
                );
            }
        }
    } else {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(radius),
            theme::guild_color(&channel.id),
        );
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            channel
                .direct_label()
                .chars()
                .next()
                .unwrap_or('?')
                .to_string(),
            fonts::bold(16.0),
            Color32::WHITE,
        );
    }
    response
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(channel.direct_label())
}

/// Séparateur horizontal du rail.
fn rail_divider(ui: &mut Ui) {
    ui.add_space(6.0);
    let (rect, _) = ui.allocate_exact_size(vec2(RAIL_WIDTH, 2.0), Sense::hover());
    ui.painter().line_segment(
        [
            pos2(rect.center().x - 16.0, rect.center().y),
            pos2(rect.center().x + 16.0, rect.center().y),
        ],
        Stroke::new(1.0, theme::DIVIDER),
    );
    ui.add_space(6.0);
}

/// One guild icon. Returns its id when clicked. Draws the selection pill and
/// the unread dot on the rail.
fn rail_guild(ui: &mut Ui, guild: &Guild, media: &str, selected: bool) -> Option<String> {
    let (row, response) = ui.allocate_exact_size(vec2(RAIL_WIDTH, 48.0), Sense::click());
    let icon_rect = Rect::from_center_size(row.center(), Vec2::splat(theme::GUILD_ICON_SIZE));
    let painter = ui.painter();
    let hovered = response.hovered();

    // Pill on the left edge: tall when selected, short on hover.
    let pill_h = if selected {
        28.0
    } else if hovered {
        12.0
    } else {
        0.0
    };
    if pill_h > 0.0 {
        painter.rect_filled(
            Rect::from_min_size(
                pos2(row.left() + 1.0, row.center().y - pill_h / 2.0),
                vec2(4.0, pill_h),
            ),
            CornerRadius::same(2),
            Color32::WHITE,
        );
    }

    let radius = if selected || hovered { 13 } else { 22 };
    match guild.icon_url(media) {
        Some(url) => {
            Image::new(url)
                .fit_to_exact_size(icon_rect.size())
                .maintain_aspect_ratio(false)
                .corner_radius(CornerRadius::same(radius))
                .paint_at(ui, icon_rect);
        }
        None => {
            // `guildIconNoImage` : fond `--guild-list-foreground`, puis
            // `--brand-primary` au survol et à la sélection.
            let fill = if selected || hovered {
                theme::ACCENT
            } else {
                theme::BG_GUILD_FOREGROUND
            };
            painter.rect_filled(icon_rect, CornerRadius::same(radius), fill);
            painter.text(
                icon_rect.center(),
                Align2::CENTER_CENTER,
                guild.initials(),
                fonts::bold(16.0),
                Color32::WHITE,
            );
        }
    }

    // Small red dot for servers with unread channels.
    let response = response
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(&guild.name);

    if response.clicked() {
        Some(guild.id.clone())
    } else {
        None
    }
}

// --- channel sidebar -------------------------------------------------------

fn channel_sidebar(app: &mut FluxerApp, ui: &mut Ui) {
    egui::Panel::left("channel_sidebar")
        .default_size(SIDEBAR_WIDTH)
        .min_size(SIDEBAR_WIDTH)
        .max_size(SIDEBAR_WIDTH)
        .resizable(false)
        .frame(Frame::default().fill(theme::BG_SIDEBAR))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;

            let guild = current_guild(app).cloned();
            let media = app.state.media_base.clone();

            sidebar_header(ui, guild.as_ref());

            if let Some(guild) = &guild
                && let Some(url) = guild.banner_url(&media)
            {
                banner(ui, &url, guild, &media);
            }

            voice_panel(app, ui);

            let channels = app.state.channels.clone();
            let selected = app.state.current_channel.clone();
            let unread = app.unread.clone();
            let mut collapsed = app.collapsed.clone();
            let mut clicked = None;

            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(8.0);
                    if channels.is_empty() {
                        ui.add_space(10.0);
                        ui.vertical_centered(|ui| {
                            if app.state.loading_channels {
                                ui.spinner();
                            } else {
                                ui.label(
                                    RichText::new("Aucun salon")
                                        .size(13.0)
                                        .color(theme::TEXT_FAINT),
                                );
                            }
                        });
                        return;
                    }

                    let mut categories: Vec<&Channel> = channels
                        .iter()
                        .filter(|c| c.kind() == ChannelKind::Category)
                        .collect();
                    categories.sort_by_key(|c| c.position.unwrap_or(0));

                    let loose: Vec<&Channel> = channels
                        .iter()
                        .filter(|c| c.kind() != ChannelKind::Category && c.parent_id.is_none())
                        .collect();
                    for channel in loose {
                        if channel_row(ui, channel, &selected, &unread) {
                            clicked = Some(channel.clone());
                        }
                        sidebar_voice_participants(app, ui, channel);
                    }

                    for category in categories {
                        let mut children: Vec<&Channel> = channels
                            .iter()
                            .filter(|c| {
                                c.kind() != ChannelKind::Category
                                    && c.parent_id.as_deref() == Some(category.id.as_str())
                            })
                            .collect();
                        if children.is_empty() {
                            continue;
                        }
                        children.sort_by_key(|c| c.position.unwrap_or(0));

                        let is_collapsed = collapsed.contains(&category.id);
                        if category_row(ui, category, is_collapsed) {
                            if is_collapsed {
                                collapsed.remove(&category.id);
                            } else {
                                collapsed.insert(category.id.clone());
                            }
                        }
                        if !is_collapsed {
                            for channel in children {
                                if channel_row(ui, channel, &selected, &unread) {
                                    clicked = Some(channel.clone());
                                }
                                sidebar_voice_participants(app, ui, channel);
                            }
                        }
                    }

                    ui.add_space(12.0);
                });

            app.collapsed = collapsed;
            if let Some(channel) = clicked {
                if channel.kind() == ChannelKind::Link {
                    // Un salon de lien ne contient pas d'historique : il ouvre
                    // simplement sa cible dans le navigateur.
                    if let Some(url) = channel.url.as_deref()
                        && !open_external(url)
                    {
                        app.state.error = Some(format!("Impossible d'ouvrir {url}"));
                    }
                } else if channel.kind() == ChannelKind::Voice {
                    if let Some(guild_id) = channel
                        .guild_id
                        .clone()
                        .or_else(|| app.state.current_guild.clone())
                    {
                        app.command(Command::JoinVoice {
                            guild_id,
                            channel_id: channel.id,
                        });
                    }
                } else {
                    app.view = View::Guilds;
                    app.command(Command::SelectChannel(channel.id));
                }
            }
        });
}

fn sidebar_header(ui: &mut Ui, guild: Option<&Guild>) {
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), theme::LAYOUT_HEADER_HEIGHT),
        Sense::click(),
    );
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect, 0, theme::CHANNEL_HOVER);
    }
    painter.line_segment(
        [
            pos2(rect.left(), rect.bottom() - 0.5),
            pos2(rect.right(), rect.bottom() - 0.5),
        ],
        Stroke::new(1.0, theme::DIVIDER),
    );

    let name = guild.map(|g| g.name.as_str()).unwrap_or("Aucun serveur");
    let clip = Rect::from_min_max(
        pos2(rect.left() + 16.0, rect.top()),
        pos2(rect.right() - 40.0, rect.bottom()),
    );
    let painter = painter.with_clip_rect(clip);
    let text_rect = painter.text(
        pos2(rect.left() + 16.0, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        fonts::bold(15.0),
        theme::TEXT,
    );
    let chevron = Rect::from_center_size(
        pos2(
            (text_rect.right() + 14.0).min(rect.right() - 24.0),
            rect.center().y,
        ),
        Vec2::splat(14.0),
    );
    icons::paint(
        ui.painter(),
        chevron,
        Icon::ChevronDown,
        theme::TEXT_MUTED,
        1.6,
    );
    response.on_hover_cursor(CursorIcon::PointingHand);
}

fn banner(ui: &mut Ui, url: &str, guild: &Guild, media: &str) {
    let width = ui.available_width();
    let ratio = match (guild.banner_width, guild.banner_height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => h as f32 / w as f32,
        _ => 0.30,
    };
    let height = (width * ratio).clamp(64.0, 132.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    Image::new(url.to_owned())
        .fit_to_exact_size(rect.size())
        .maintain_aspect_ratio(false)
        .paint_at(ui, rect);

    // Fade the bottom of the banner into the sidebar background.
    let steps = 18;
    for i in 0..steps {
        let t = i as f32 / steps as f32;
        let alpha = (t * t * 255.0) as u8;
        let y0 = rect.bottom() - height * (1.0 - t) * 0.9;
        let y1 = y0 + height / steps as f32 + 1.0;
        ui.painter().rect_filled(
            Rect::from_min_max(pos2(rect.left(), y0), pos2(rect.right(), y1)),
            0,
            Color32::from_rgba_unmultiplied(
                theme::BG_SIDEBAR.r(),
                theme::BG_SIDEBAR.g(),
                theme::BG_SIDEBAR.b(),
                alpha,
            ),
        );
    }
    let _ = media;

    // Small guild icon overlapping the banner, like the real client.
    if let Some(icon) = guild.icon_url(media) {
        let icon_rect = Rect::from_min_size(
            pos2(rect.left() + 16.0, rect.bottom() - 26.0),
            Vec2::splat(52.0),
        );
        Image::new(icon)
            .fit_to_exact_size(icon_rect.size())
            .corner_radius(CornerRadius::same(14))
            .paint_at(ui, icon_rect);
        ui.painter().rect_stroke(
            icon_rect.expand(1.0),
            CornerRadius::same(15),
            Stroke::new(3.0, theme::BG_SIDEBAR),
            egui::StrokeKind::Outside,
        );
        ui.add_space(28.0);
    }
    ui.add_space(4.0);
}

/// Icône associée à chaque type de salon, alignée sur le client officiel.
fn channel_icon(kind: ChannelKind) -> Icon {
    match kind {
        ChannelKind::Voice => Icon::Volume,
        ChannelKind::Announcement => Icon::Megaphone,
        ChannelKind::Link => Icon::Link,
        ChannelKind::Forum => Icon::Forum,
        ChannelKind::Media => Icon::Image,
        ChannelKind::AnnouncementThread
        | ChannelKind::PublicThread
        | ChannelKind::PrivateThread => Icon::Thread,
        ChannelKind::PersonalNotes => Icon::Note,
        _ => Icon::Hash,
    }
}

/// Ouvre une cible externe avec le gestionnaire d'URL du bureau Wayland.
pub(crate) fn open_external(url: &str) -> bool {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return false;
    }
    std::process::Command::new("xdg-open")
        .arg(url)
        .spawn()
        .is_ok()
}

fn channel_row(
    ui: &mut Ui,
    channel: &Channel,
    selected: &Option<String>,
    unread: &HashMap<String, usize>,
) -> bool {
    let kind = channel.kind();
    let readable = kind.is_readable();
    let is_selected = selected.as_deref() == Some(channel.id.as_str());
    let count = unread.get(&channel.id).copied().unwrap_or(0);

    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
    let row = Rect::from_min_max(
        pos2(rect.left() + 8.0, rect.top()),
        pos2(rect.right() - 8.0, rect.bottom()),
    );

    let painter = ui.painter();
    if is_selected {
        painter.rect_filled(row, CornerRadius::same(6), theme::CHANNEL_ACTIVE);
    } else if response.hovered() {
        painter.rect_filled(row, CornerRadius::same(6), theme::CHANNEL_HOVER);
    }

    let icon_rect =
        Rect::from_center_size(pos2(row.left() + 18.0, row.center().y), Vec2::splat(19.0));
    icons::paint(
        ui.painter(),
        icon_rect,
        channel_icon(kind),
        theme::TEXT_FAINT,
        1.6,
    );

    let color = if is_selected {
        theme::TEXT
    } else if readable {
        theme::TEXT_MUTED
    } else {
        theme::TEXT_FAINT
    };
    let font = if count > 0 {
        fonts::bold(14.5)
    } else {
        FontId::proportional(14.5)
    };
    let clip = Rect::from_min_max(
        pos2(row.left() + 34.0, row.top()),
        pos2(
            row.right() - if count > 0 { 34.0 } else { 8.0 },
            row.bottom(),
        ),
    );
    ui.painter().with_clip_rect(clip).text(
        pos2(row.left() + 34.0, row.center().y),
        Align2::LEFT_CENTER,
        channel.label(),
        font,
        if count > 0 { theme::TEXT } else { color },
    );

    if count > 0 {
        badge(ui, row.right() - 14.0, row.center().y, count);
    }

    response.clicked() && (readable || matches!(kind, ChannelKind::Voice | ChannelKind::Link))
}

/// Liste compacte des participants sous un salon vocal, comme dans le client
/// officiel. On n'affiche que le salon actuellement rejoint : la passerelle ne
/// nous transmet les `VOICE_STATE_UPDATE` que pour celui-ci.
fn sidebar_voice_participants(app: &FluxerApp, ui: &mut Ui, channel: &Channel) {
    if channel.kind() != ChannelKind::Voice
        || app.state.voice_channel.as_deref() != Some(channel.id.as_str())
    {
        return;
    }

    let media = app.state.media_base.clone();
    let mut users: Vec<User> = Vec::new();
    if let Some(me) = app.state.me.clone() {
        users.push(me);
    }
    for id in &app.state.voice_members {
        if users.iter().any(|user| &user.id == id) {
            continue;
        }
        if let Some(member) = app.state.members.iter().find(|m| &m.user.id == id) {
            users.push(member.user.clone());
        }
    }

    for user in &users {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
        let avatar = Rect::from_min_size(
            pos2(rect.left() + 40.0, rect.center().y - 9.0),
            Vec2::splat(18.0),
        );
        paint_avatar(
            ui,
            avatar,
            user,
            &media,
            app.state.presence.get(&user.id).map(String::as_str),
            theme::BG_SIDEBAR,
        );
        ui.painter().with_clip_rect(rect).text(
            pos2(avatar.right() + 8.0, rect.center().y),
            Align2::LEFT_CENTER,
            user.display_name(),
            FontId::proportional(12.5),
            theme::TEXT_MUTED,
        );
    }
    ui.add_space(2.0);
}

fn category_row(ui: &mut Ui, category: &Channel, collapsed: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(
            Rect::from_min_max(
                pos2(rect.left() + 8.0, rect.top()),
                pos2(rect.right() - 8.0, rect.bottom()),
            ),
            CornerRadius::same(6),
            theme::CHANNEL_HOVER,
        );
    }
    let arrow =
        Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), Vec2::splat(11.0));
    icons::paint(
        ui.painter(),
        arrow,
        if collapsed {
            Icon::ChevronRight
        } else {
            Icon::ChevronDown
        },
        theme::TEXT_FAINT,
        1.6,
    );
    ui.painter()
        .with_clip_rect(Rect::from_min_max(
            pos2(rect.left() + 30.0, rect.top()),
            pos2(rect.right() - 8.0, rect.bottom()),
        ))
        .text(
            pos2(rect.left() + 30.0, rect.center().y),
            Align2::LEFT_CENTER,
            category.label().to_uppercase(),
            fonts::bold(11.0),
            theme::TEXT_FAINT,
        );
    response.clicked()
}

fn badge(ui: &Ui, cx: f32, cy: f32, count: usize) {
    let text = if count > 99 {
        "99+".to_string()
    } else {
        count.to_string()
    };
    let width = if count > 9 { 20.0 } else { 16.0 };
    let rect = Rect::from_center_size(pos2(cx, cy), vec2(width, 16.0));
    ui.painter()
        .rect_filled(rect, CornerRadius::same(8), theme::DANGER);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        fonts::bold(10.0),
        Color32::WHITE,
    );
}

fn user_panel(app: &mut FluxerApp, ui: &mut Ui) {
    let Some(me) = app.state.me.clone() else {
        return;
    };
    let media = app.state.media_base.clone();
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
    paint_avatar(
        ui,
        Rect::from_min_size(rect.min, Vec2::splat(32.0)),
        &me,
        &media,
        app.state.presence.get(&me.id).map(String::as_str),
        theme::BG_PANEL_CONTROL,
    );
    let name_rect = Rect::from_min_max(
        pos2(rect.left() + 40.0, rect.top()),
        pos2(rect.right() - 116.0, rect.bottom()),
    );
    let painter = ui.painter().with_clip_rect(name_rect);
    painter.text(
        pos2(name_rect.left(), rect.center().y - 7.0),
        Align2::LEFT_CENTER,
        me.display_name(),
        fonts::bold(13.5),
        theme::TEXT,
    );
    painter.text(
        pos2(name_rect.left(), rect.center().y + 8.0),
        Align2::LEFT_CENTER,
        format!("@{}", me.username),
        FontId::proportional(10.5),
        theme::TEXT_FAINT,
    );
    let name_response = ui
        .interact(
            name_rect,
            ui.scope_id().with("user_area_name"),
            Sense::click(),
        )
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text("Paramètres du compte");
    if name_response.clicked() {
        app.show_settings = true;
    }

    // Ordre officiel : micro, casque, réglages (de gauche à droite).
    let controls = Rect::from_min_max(pos2(rect.right() - 102.0, rect.top()), rect.max);
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(controls)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let _ = icon_button(ui, Icon::Mic, 32.0, theme::TEXT_MUTED).on_hover_text("Micro");
            let _ =
                icon_button(ui, Icon::Headphones, 32.0, theme::TEXT_MUTED).on_hover_text("Casque");
            if icon_button(ui, Icon::Settings, 32.0, theme::TEXT_MUTED)
                .on_hover_text("Paramètres")
                .clicked()
            {
                app.show_settings = true;
            }
        },
    );
}

// --- panneau vocal ---------------------------------------------------------

/// Compact voice status panel shown above the user area, like the official
/// client: channel name, live connection state and the mute/deafen controls.
fn voice_panel(app: &mut FluxerApp, ui: &mut Ui) {
    let Some(channel_id) = app.state.voice_channel.clone() else {
        return;
    };
    let guild_id = app.state.voice_guild.clone();
    let connected = app.state.voice_connected;
    let label = app
        .state
        .channels
        .iter()
        .chain(app.state.private_channels.iter())
        .find(|channel| channel.id == channel_id)
        .map(|channel| channel.label())
        .unwrap_or_else(|| "Salon vocal".to_string());
    let members = app.state.voice_members.len().max(1);
    let media = app.state.media_base.clone();

    egui::Panel::bottom("voice_panel")
        .default_size(104.0)
        .min_size(104.0)
        .max_size(104.0)
        .resizable(false)
        .frame(
            Frame::default()
                .fill(theme::BG_PANEL_CONTROL)
                .inner_margin(Margin::symmetric(10, 8)),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (icon, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                let tint = if connected {
                    theme::ONLINE
                } else {
                    theme::TEXT_FAINT
                };
                icons::paint(ui.painter(), icon, Icon::Volume, tint, 1.8);
                ui.add_space(2.0);
                ui.label(
                    RichText::new(if connected {
                        "Voix connectée"
                    } else {
                        "Connexion au vocal…"
                    })
                    .font(fonts::bold(11.0))
                    .color(tint),
                );
            });
            ui.add_space(2.0);
            ui.label(
                RichText::new(label)
                    .font(fonts::bold(13.5))
                    .color(theme::TEXT),
            );
            ui.label(
                RichText::new(format!(
                    "{members} participant(s) · {}",
                    if connected {
                        "audio actif"
                    } else {
                        "négociation audio…"
                    }
                ))
                .size(10.5)
                .color(theme::TEXT_FAINT),
            );
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                let muted = app.state.voice_muted;
                let deafened = app.state.voice_deafened;
                if icon_button(
                    ui,
                    Icon::Mic,
                    26.0,
                    if muted {
                        theme::DANGER
                    } else {
                        theme::TEXT_MUTED
                    },
                )
                .on_hover_text(if muted {
                    "Réactiver le micro"
                } else {
                    "Couper le micro"
                })
                .clicked()
                {
                    app.state.voice_muted = !muted;
                    let _ = media;
                    app.command(Command::SetVoiceState {
                        muted: app.state.voice_muted,
                        deafened: app.state.voice_deafened,
                    });
                    app.save_prefs();
                }
                if icon_button(
                    ui,
                    Icon::Headphones,
                    26.0,
                    if deafened {
                        theme::DANGER
                    } else {
                        theme::TEXT_MUTED
                    },
                )
                .on_hover_text(if deafened {
                    "Réactiver le son"
                } else {
                    "Couper le son"
                })
                .clicked()
                {
                    app.state.voice_deafened = !deafened;
                    app.command(Command::SetVoiceState {
                        muted: app.state.voice_muted,
                        deafened: app.state.voice_deafened,
                    });
                    app.save_prefs();
                }
                if icon_button(ui, Icon::Phone, 26.0, theme::DANGER)
                    .on_hover_text("Quitter le salon vocal")
                    .clicked()
                    && let Some(guild_id) = guild_id.clone()
                {
                    app.command(Command::LeaveVoice { guild_id });
                }
            });
        });
}

// --- member list -----------------------------------------------------------

fn member_list(app: &mut FluxerApp, ui: &mut Ui) {
    egui::Panel::right("member_list")
        .default_size(theme::LAYOUT_MEMBER_LIST_WIDTH)
        .min_size(theme::LAYOUT_MEMBER_LIST_WIDTH)
        .max_size(theme::LAYOUT_MEMBER_LIST_WIDTH)
        .resizable(false)
        .frame(Frame::default().fill(theme::BG_SIDEBAR_LIGHTER))
        .show(ui, |ui| {
            let (header, _) = ui.allocate_exact_size(
                vec2(ui.available_width(), theme::LAYOUT_HEADER_HEIGHT),
                Sense::hover(),
            );
            ui.painter().text(
                pos2(header.left() + 16.0, header.center().y),
                Align2::LEFT_CENTER,
                format!("MEMBRES — {}", app.state.members.len()),
                fonts::bold(11.0),
                theme::TEXT_FAINT,
            );

            if app.state.loading_members && app.state.members.is_empty() {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.spinner();
                });
                return;
            }

            let mut online: Vec<Member> = app
                .state
                .members
                .iter()
                .filter(|m| {
                    app.state
                        .presence
                        .get(&m.user.id)
                        .map(|s| s != "offline")
                        .unwrap_or(false)
                })
                .cloned()
                .collect();
            let mut offline: Vec<Member> = app
                .state
                .members
                .iter()
                .filter(|m| {
                    !app.state
                        .presence
                        .get(&m.user.id)
                        .map(|s| s != "offline")
                        .unwrap_or(false)
                })
                .cloned()
                .collect();
            if std::env::var_os("FLUXER_DEBUG_PRESENCE").is_some() {
                eprintln!(
                    "presence: map={} members={} online={}",
                    app.state.presence.len(),
                    app.state.members.len(),
                    online.len()
                );
            }
            let key = |m: &Member| m.display_name().to_lowercase();
            online.sort_by_key(key);
            offline.sort_by_key(key);

            let media = app.state.media_base.clone();

            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if !online.is_empty() {
                        section_label(ui, "EN LIGNE", online.len());
                        for member in &online {
                            member_row(app, ui, member, &media);
                        }
                    }
                    if !offline.is_empty() {
                        section_label(ui, "HORS LIGNE", offline.len());
                        for member in &offline {
                            member_row(app, ui, member, &media);
                        }
                    }
                    ui.add_space(8.0);
                });
        });
}

fn section_label(ui: &mut Ui, label: &str, count: usize) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
    ui.painter().text(
        pos2(rect.left() + 16.0, rect.center().y),
        Align2::LEFT_CENTER,
        format!("{label} — {count}"),
        fonts::bold(10.5),
        theme::TEXT_FAINT,
    );
}

fn member_row(app: &mut FluxerApp, ui: &mut Ui, member: &Member, media: &str) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::click());
    let row = Rect::from_min_max(
        pos2(rect.left() + 8.0, rect.top()),
        pos2(rect.right() - 8.0, rect.bottom()),
    );
    if response.hovered() {
        ui.painter()
            .rect_filled(row, CornerRadius::same(6), theme::CHANNEL_HOVER);
    }

    let status = app
        .state
        .presence
        .get(&member.user.id)
        .map(String::as_str)
        .unwrap_or("offline");
    let offline = status == "offline";
    let avatar_rect =
        Rect::from_min_size(pos2(row.left() + 8.0, rect.top() + 5.0), Vec2::splat(32.0));
    paint_avatar(
        ui,
        avatar_rect,
        &member.user,
        media,
        Some(status),
        theme::BG_SIDEBAR,
    );

    let color = member_color(app, member);
    let color = if offline {
        color.gamma_multiply(0.55)
    } else {
        color
    };
    let clip = Rect::from_min_max(
        pos2(row.left() + 48.0, rect.top()),
        pos2(row.right() - 6.0, rect.bottom()),
    );
    ui.painter().with_clip_rect(clip).text(
        pos2(row.left() + 48.0, rect.center().y),
        Align2::LEFT_CENTER,
        member.display_name(),
        FontId::proportional(13.5),
        color,
    );

    if response.clicked() {
        app.profile_popup = Some(member.user.clone());
    }
    response.on_hover_cursor(CursorIcon::PointingHand);
    if member.user.bot.unwrap_or(false) {
        ui.painter().text(
            pos2(row.right() - 4.0, rect.center().y),
            Align2::RIGHT_CENTER,
            "BOT",
            fonts::bold(9.0),
            theme::ACCENT_HOVER,
        );
    }
}

fn member_color(app: &FluxerApp, member: &Member) -> Color32 {
    let roles = app.state.detail.as_ref().map(|d| &d.roles);
    if let Some(roles) = roles {
        let mut best: Option<(i64, i64)> = None;
        for role_id in &member.roles {
            if let Some(role) = roles.iter().find(|r| &r.id == role_id)
                && role.color != 0
                && best.map(|(pos, _)| role.position > pos).unwrap_or(true)
            {
                best = Some((role.position, role.color));
            }
        }
        if let Some(color) = best.and_then(|(_, color)| theme::role_color(color)) {
            return color;
        }
    }
    theme::TEXT_SECONDARY
}

// --- chat header -----------------------------------------------------------

// --- messages privés -------------------------------------------------------

fn dm_sidebar(app: &mut FluxerApp, ui: &mut Ui) {
    egui::Panel::left("dm_sidebar")
        .default_size(SIDEBAR_WIDTH)
        .min_size(SIDEBAR_WIDTH)
        .max_size(SIDEBAR_WIDTH)
        .resizable(false)
        .frame(Frame::default().fill(theme::BG_SIDEBAR))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;

            let (rect, _) = ui.allocate_exact_size(
                vec2(ui.available_width(), theme::LAYOUT_HEADER_HEIGHT),
                Sense::hover(),
            );
            ui.painter().text(
                pos2(rect.left() + 16.0, rect.center().y),
                Align2::LEFT_CENTER,
                "Messages privés",
                fonts::bold(15.5),
                theme::TEXT,
            );
            let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
            ui.painter().rect_filled(line, 0, theme::DIVIDER);

            voice_panel(app, ui);

            let mut channels = app.state.private_channels.clone();
            channels.sort_by(|a, b| b.last_message_id.cmp(&a.last_message_id));
            let selected = app.state.current_channel.clone();
            let unread = app.unread.clone();
            let presence = app.state.presence.clone();
            let media = app.state.media_base.clone();
            let mut clicked = None;

            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(10.0);
                    section_label(ui, "Conversations", channels.len());
                    ui.add_space(4.0);
                    if channels.is_empty() {
                        ui.add_space(16.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new("Aucune conversation privée")
                                    .size(12.5)
                                    .color(theme::TEXT_FAINT),
                            );
                        });
                    }
                    for channel in &channels {
                        let is_selected = selected.as_deref() == Some(channel.id.as_str());
                        let count = unread.get(&channel.id).copied().unwrap_or(0);
                        if dm_row(ui, channel, &media, &presence, is_selected, count) {
                            clicked = Some(channel.id.clone());
                        }
                    }
                    ui.add_space(12.0);
                });

            if let Some(id) = clicked {
                app.view = View::Dms;
                app.command(Command::SelectDm(id));
            }
        });
}

fn dm_row(
    ui: &mut Ui,
    channel: &Channel,
    media: &str,
    presence: &HashMap<String, String>,
    selected: bool,
    unread: usize,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 54.0), Sense::click());
    let row = Rect::from_min_max(
        pos2(rect.left() + 8.0, rect.top()),
        pos2(rect.right() - 8.0, rect.bottom()),
    );
    if selected {
        ui.painter()
            .rect_filled(row, CornerRadius::same(6), theme::CHANNEL_ACTIVE);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(row, CornerRadius::same(6), theme::CHANNEL_HOVER);
    }

    let avatar_rect =
        Rect::from_center_size(pos2(row.left() + 24.0, row.center().y), Vec2::splat(34.0));
    let status = channel
        .dm_user()
        .and_then(|user| presence.get(&user.id))
        .map(String::as_str);
    if let Some(user) = channel.dm_user() {
        paint_avatar(ui, avatar_rect, user, media, status, theme::BG_SIDEBAR);
    } else {
        ui.painter()
            .rect_filled(avatar_rect, CornerRadius::same(17), theme::BG_TERTIARY);
        ui.painter().text(
            avatar_rect.center(),
            Align2::CENTER_CENTER,
            channel.label().chars().next().unwrap_or('?'),
            fonts::bold(15.0),
            Color32::WHITE,
        );
    }

    let text_left = row.left() + 48.0;
    let clip = Rect::from_min_max(
        pos2(text_left, row.top() + 11.0),
        pos2(
            row.right() - if unread > 0 { 30.0 } else { 8.0 },
            row.bottom() - 9.0,
        ),
    );
    ui.painter().with_clip_rect(clip).text(
        pos2(text_left, row.center().y - 7.0),
        Align2::LEFT_CENTER,
        channel.direct_label(),
        if unread > 0 {
            fonts::bold(14.0)
        } else {
            FontId::proportional(14.0)
        },
        theme::TEXT,
    );
    let status_label = match status {
        Some("online") => "En ligne",
        Some("idle") => "Absent",
        Some("dnd") => "Ne pas déranger",
        _ => "Hors ligne",
    };
    ui.painter().text(
        pos2(text_left, row.center().y + 9.0),
        Align2::LEFT_CENTER,
        status_label,
        FontId::proportional(11.0),
        theme::TEXT_FAINT,
    );
    if unread > 0 {
        badge(ui, row.right() - 12.0, row.center().y, unread);
    }
    response.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

fn dm_home(app: &mut FluxerApp, ui: &mut Ui) {
    egui::CentralPanel::default()
        .frame(Frame::default().fill(theme::BG_PRIMARY))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() * 0.22);
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(72.0), Sense::hover());
                paint_logo(ui, rect);
                ui.add_space(14.0);
                ui.label(
                    RichText::new("Messages privés")
                        .font(fonts::bold(24.0))
                        .color(theme::TEXT),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!(
                        "{} conversation(s) — choisissez-la dans la liste de gauche.",
                        app.state.private_channels.len()
                    ))
                    .size(13.5)
                    .color(theme::TEXT_MUTED),
                );
                if !app.state.private_channels.is_empty() {
                    ui.add_space(18.0);
                    let mut first = None;
                    ui.horizontal(|ui| {
                        for channel in app.state.private_channels.iter().take(4) {
                            if ui
                                .add(
                                    Button::new(
                                        RichText::new(channel.direct_label())
                                            .size(12.5)
                                            .color(theme::TEXT),
                                    )
                                    .fill(theme::BG_TERTIARY)
                                    .corner_radius(CornerRadius::same(6)),
                                )
                                .clicked()
                            {
                                first = Some(channel.id.clone());
                            }
                        }
                    });
                    if let Some(id) = first {
                        app.command(Command::SelectDm(id));
                    }
                }
            });
        });
}

// --- favoris ---------------------------------------------------------------

// --- annuaire public -------------------------------------------------------

fn discovery_sidebar(app: &mut FluxerApp, ui: &mut Ui) {
    egui::Panel::left("discovery_sidebar")
        .default_size(SIDEBAR_WIDTH)
        .min_size(SIDEBAR_WIDTH)
        .max_size(SIDEBAR_WIDTH)
        .resizable(false)
        .frame(Frame::default().fill(theme::BG_SIDEBAR))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let (rect, _) = ui.allocate_exact_size(
                vec2(ui.available_width(), theme::LAYOUT_HEADER_HEIGHT),
                Sense::hover(),
            );
            ui.painter().text(
                pos2(rect.left() + 16.0, rect.center().y),
                Align2::LEFT_CENTER,
                "Explorer",
                fonts::bold(15.5),
                theme::TEXT,
            );
            let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
            ui.painter().rect_filled(line, 0, theme::DIVIDER);

            ui.add_space(14.0);
            Frame::default()
                .inner_margin(Margin::symmetric(12, 0))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("Découvrez des communautés publiques et rejoignez celles qui vous intéressent.")
                            .size(12.0)
                            .color(theme::TEXT_MUTED),
                    );
                    ui.add_space(12.0);
                    ui.add_sized(
                        [ui.available_width(), 32.0],
                        TextEdit::singleline(&mut app.search)
                            .hint_text("Rechercher une communauté")
                            .margin(Vec2::new(8.0, 7.0)),
                    );
                    ui.add_space(10.0);
                    if ui
                        .add_enabled(
                            !app.state.discovery_loading,
                            Button::new(RichText::new("Actualiser").size(12.0))
                                .corner_radius(CornerRadius::same(5)),
                        )
                        .clicked()
                    {
                        app.command(Command::LoadDiscovery { offset: 0 });
                    }
                    ui.add_space(14.0);
                    let shown = app.state.discovery.len();
                    let total = app.state.discovery_total.max(shown as i64);
                    ui.label(
                        RichText::new(format!("{shown} / {total} communauté(s)"))
                            .size(11.0)
                            .color(theme::TEXT_FAINT),
                    );
                });
        });
}

fn discovery_page(app: &mut FluxerApp, ui: &mut Ui) {
    egui::CentralPanel::default()
        .frame(Frame::default().fill(theme::BG_PRIMARY))
        .show(ui, |ui| {
            let (header, _) = ui.allocate_exact_size(
                vec2(ui.available_width(), theme::LAYOUT_HEADER_HEIGHT),
                Sense::hover(),
            );
            ui.painter().text(
                pos2(header.left() + 20.0, header.center().y),
                Align2::LEFT_CENTER,
                "Communautés publiques",
                fonts::bold(16.0),
                theme::TEXT,
            );
            let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
            ui.painter().rect_filled(line, 0, theme::DIVIDER);

            if app.state.discovery_loading {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.spinner();
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Chargement de l'annuaire…")
                            .size(12.5)
                            .color(theme::TEXT_MUTED),
                    );
                });
                return;
            }

            let query = app.search.trim().to_lowercase();
            let guilds: Vec<_> = app
                .state
                .discovery
                .iter()
                .filter(|guild| {
                    query.is_empty()
                        || guild.name.to_lowercase().contains(&query)
                        || guild
                            .description
                            .as_deref()
                            .unwrap_or_default()
                            .to_lowercase()
                            .contains(&query)
                        || guild
                            .custom_tags
                            .iter()
                            .any(|tag| tag.to_lowercase().contains(&query))
                })
                .cloned()
                .collect();
            let media = app.state.media_base.clone();
            let joined: std::collections::HashSet<String> = app
                .state
                .guilds
                .iter()
                .map(|guild| guild.id.clone())
                .collect();

            if guilds.is_empty() {
                ui.add_space(48.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new(if query.is_empty() {
                            "Aucune communauté disponible pour le moment."
                        } else {
                            "Aucune communauté ne correspond à cette recherche."
                        })
                        .size(13.0)
                        .color(theme::TEXT_FAINT),
                    );
                });
                return;
            }

            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(16.0);
                    ui.spacing_mut().item_spacing.y = 10.0;
                    for guild in &guilds {
                        Frame::default()
                            .fill(theme::BG_SIDEBAR)
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(Margin::same(14))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    let (icon_rect, _) =
                                        ui.allocate_exact_size(Vec2::splat(56.0), Sense::hover());
                                    match guild.icon_url(&media) {
                                        Some(url) => {
                                            Image::new(url)
                                                .fit_to_exact_size(icon_rect.size())
                                                .maintain_aspect_ratio(false)
                                                .corner_radius(CornerRadius::same(14))
                                                .paint_at(ui, icon_rect);
                                        }
                                        None => {
                                            ui.painter().rect_filled(
                                                icon_rect,
                                                CornerRadius::same(14),
                                                theme::guild_color(&guild.id),
                                            );
                                            ui.painter().text(
                                                icon_rect.center(),
                                                Align2::CENTER_CENTER,
                                                guild.initials(),
                                                fonts::bold(18.0),
                                                Color32::WHITE,
                                            );
                                        }
                                    }
                                    ui.add_space(12.0);
                                    ui.vertical(|ui| {
                                        ui.set_width(ui.available_width() - 130.0);
                                        ui.label(
                                            RichText::new(&guild.name)
                                                .font(fonts::bold(15.0))
                                                .color(theme::TEXT),
                                        );
                                        if let Some(description) = guild
                                            .description
                                            .as_deref()
                                            .filter(|description| !description.is_empty())
                                        {
                                            ui.label(
                                                RichText::new(description)
                                                    .size(12.0)
                                                    .color(theme::TEXT_MUTED),
                                            );
                                        }
                                        ui.add_space(4.0);
                                        let members = guild.member_count.unwrap_or(0);
                                        let online = guild.online_count.unwrap_or(0);
                                        let mut facts = format!("{members} membres");
                                        if online > 0 {
                                            facts.push_str(&format!(" · {online} en ligne"));
                                        }
                                        if let Some(language) = guild
                                            .primary_language
                                            .as_deref()
                                            .filter(|language| !language.is_empty())
                                        {
                                            facts.push_str(&format!(" · {language}"));
                                        }
                                        ui.label(
                                            RichText::new(facts)
                                                .size(11.0)
                                                .color(theme::TEXT_FAINT),
                                        );
                                        if !guild.custom_tags.is_empty() {
                                            ui.horizontal_wrapped(|ui| {
                                                for tag in guild.custom_tags.iter().take(5) {
                                                    Frame::default()
                                                        .fill(theme::BG_TERTIARY)
                                                        .corner_radius(CornerRadius::same(4))
                                                        .inner_margin(Margin::symmetric(5, 2))
                                                        .show(ui, |ui| {
                                                            ui.label(
                                                                RichText::new(tag)
                                                                    .size(10.0)
                                                                    .color(theme::TEXT_MUTED),
                                                            );
                                                        });
                                                }
                                            });
                                        }
                                    });
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        let is_member = joined.contains(&guild.id);
                                        let label = if is_member { "Ouvrir" } else { "Rejoindre" };
                                        if ui
                                            .add(
                                                Button::new(
                                                    RichText::new(label)
                                                        .color(Color32::WHITE)
                                                        .font(fonts::bold(12.0)),
                                                )
                                                .fill(if is_member {
                                                    theme::BG_TERTIARY
                                                } else {
                                                    theme::ACCENT
                                                })
                                                .corner_radius(CornerRadius::same(5)),
                                            )
                                            .clicked()
                                        {
                                            if is_member {
                                                app.view = View::Guilds;
                                                app.command(Command::SelectGuild(guild.id.clone()));
                                            } else {
                                                app.state.status =
                                                    format!("Adhésion à {}…", guild.name);
                                                app.command(Command::JoinDiscoveryGuild {
                                                    guild_id: guild.id.clone(),
                                                });
                                            }
                                        }
                                    });
                                });
                            });
                    }
                    if app.state.discovery.len() < app.state.discovery_total as usize {
                        ui.add_space(6.0);
                        ui.vertical_centered(|ui| {
                            if ui
                                .add_enabled(
                                    !app.state.discovery_loading,
                                    Button::new(
                                        RichText::new(if app.state.discovery_loading {
                                            "Chargement…"
                                        } else {
                                            "Charger plus de communautés"
                                        })
                                        .size(12.0),
                                    )
                                    .corner_radius(CornerRadius::same(5)),
                                )
                                .clicked()
                            {
                                let offset = app.state.discovery.len() as u16;
                                app.command(Command::LoadDiscovery { offset });
                            }
                        });
                    }
                    ui.add_space(20.0);
                });
        });
}

fn favorites_sidebar(app: &mut FluxerApp, ui: &mut Ui) {
    egui::Panel::left("favorites_sidebar")
        .default_size(SIDEBAR_WIDTH)
        .min_size(SIDEBAR_WIDTH)
        .max_size(SIDEBAR_WIDTH)
        .resizable(false)
        .frame(Frame::default().fill(theme::BG_SIDEBAR))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;

            let (rect, _) = ui.allocate_exact_size(
                vec2(ui.available_width(), theme::LAYOUT_HEADER_HEIGHT),
                Sense::hover(),
            );
            ui.painter().text(
                pos2(rect.left() + 16.0, rect.center().y),
                Align2::LEFT_CENTER,
                "Favoris",
                fonts::bold(15.5),
                theme::TEXT,
            );
            let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
            ui.painter().rect_filled(line, 0, theme::DIVIDER);

            ui.add_space(10.0);
            Frame::default()
                .fill(theme::BG_COMPOSER)
                .corner_radius(CornerRadius::same(6))
                .inner_margin(Margin::symmetric(9, 5))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (icon, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                        icons::paint(ui.painter(), icon, Icon::Search, theme::TEXT_FAINT, 1.6);
                        ui.add(
                            TextEdit::singleline(&mut app.favorites_query)
                                .hint_text("Rechercher un favori")
                                .frame(Frame::NONE)
                                .desired_width(f32::INFINITY),
                        );
                    });
                });
            ui.add_space(8.0);

            let query = app.favorites_query.to_lowercase();
            let entries: Vec<_> = app
                .favorites
                .iter()
                .filter(|saved| {
                    query.is_empty()
                        || saved.content.to_lowercase().contains(&query)
                        || saved.author.to_lowercase().contains(&query)
                        || saved.channel_label.to_lowercase().contains(&query)
                })
                .cloned()
                .collect();
            let mut clicked = None;

            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    section_label(ui, "Messages enregistrés", entries.len());
                    ui.add_space(4.0);
                    if entries.is_empty() {
                        ui.add_space(14.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new("Aucun favori pour le moment")
                                    .size(12.5)
                                    .color(theme::TEXT_FAINT),
                            );
                        });
                    }
                    for saved in &entries {
                        let selected =
                            app.favorites_selected.as_deref() == Some(saved.message_id.as_str());
                        if favorite_row(ui, saved, selected) {
                            clicked = Some(saved.message_id.clone());
                        }
                    }
                    ui.add_space(12.0);
                });

            if let Some(id) = clicked {
                app.favorites_selected = Some(id);
            } else if app.favorites_selected.is_none() {
                app.favorites_selected = entries.first().map(|saved| saved.message_id.clone());
            }
        });
}

fn favorite_row(ui: &mut Ui, saved: &fluxer_core::session::SavedMessage, selected: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 62.0), Sense::click());
    let row = Rect::from_min_max(
        pos2(rect.left() + 8.0, rect.top() + 1.0),
        pos2(rect.right() - 8.0, rect.bottom() - 1.0),
    );
    if selected {
        ui.painter()
            .rect_filled(row, CornerRadius::same(6), theme::CHANNEL_ACTIVE);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(row, CornerRadius::same(6), theme::CHANNEL_HOVER);
    }
    let painter = ui.painter().with_clip_rect(row.shrink2(vec2(10.0, 7.0)));
    painter.text(
        pos2(row.left() + 10.0, row.top() + 13.0),
        Align2::LEFT_CENTER,
        &saved.author,
        fonts::bold(12.5),
        theme::TEXT,
    );
    painter.text(
        pos2(row.right() - 10.0, row.top() + 13.0),
        Align2::RIGHT_CENTER,
        &saved.channel_label,
        FontId::proportional(10.5),
        theme::TEXT_FAINT,
    );
    let preview = markdown::plain(&saved.content);
    let preview: String = preview.chars().take(80).collect();
    painter.text(
        pos2(row.left() + 10.0, row.top() + 34.0),
        Align2::LEFT_CENTER,
        preview,
        FontId::proportional(12.0),
        theme::TEXT_MUTED,
    );
    painter.text(
        pos2(row.left() + 10.0, row.top() + 49.0),
        Align2::LEFT_CENTER,
        favorite_date(&saved.timestamp),
        FontId::proportional(10.0),
        theme::TEXT_FAINT,
    );
    response.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

fn favorite_date(timestamp: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(timestamp)
        .map(|dt| {
            dt.with_timezone(&chrono::Local)
                .format("%d/%m/%Y à %H:%M")
                .to_string()
        })
        .unwrap_or_else(|_| timestamp.to_string())
}

fn favorites_header(app: &mut FluxerApp, ui: &mut Ui) {
    egui::Panel::top("favorites_header")
        .default_size(theme::LAYOUT_HEADER_HEIGHT)
        .min_size(theme::LAYOUT_HEADER_HEIGHT)
        .max_size(theme::LAYOUT_HEADER_HEIGHT)
        .resizable(false)
        .frame(
            Frame::default()
                .fill(theme::BG_PRIMARY)
                .inner_margin(Margin::symmetric(16, 0)),
        )
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                let (icon, _) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::hover());
                icons::paint(ui.painter(), icon, Icon::Star, theme::ACCENT, 1.8);
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Messages favoris")
                        .font(fonts::bold(16.0))
                        .color(theme::TEXT),
                );
                ui.label(
                    RichText::new(format!("· {}", app.favorites.len()))
                        .size(12.0)
                        .color(theme::TEXT_FAINT),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if !app.favorites.is_empty()
                        && ui
                            .add(
                                Button::new(
                                    RichText::new("Tout effacer")
                                        .size(12.0)
                                        .color(theme::TEXT_MUTED),
                                )
                                .frame(false),
                            )
                            .clicked()
                    {
                        app.favorites.clear();
                        app.favorites_selected = None;
                        app.save_prefs();
                    }
                });
            });
            let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
            ui.painter().rect_filled(line, 0, theme::DIVIDER);
        });
}

fn favorites_view(app: &mut FluxerApp, ui: &mut Ui) {
    egui::CentralPanel::default()
        .frame(Frame::default().fill(theme::BG_PRIMARY))
        .show(ui, |ui| {
            let query = app.favorites_query.to_lowercase();
            let entries: Vec<_> = app
                .favorites
                .iter()
                .filter(|saved| {
                    query.is_empty()
                        || saved.content.to_lowercase().contains(&query)
                        || saved.author.to_lowercase().contains(&query)
                        || saved.channel_label.to_lowercase().contains(&query)
                })
                .cloned()
                .collect();

            if entries.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() * 0.28);
                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::splat(64.0), Sense::hover());
                    icons::paint(ui.painter(), rect, Icon::Star, theme::TEXT_FAINT, 1.4);
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new("Aucun message favori")
                            .font(fonts::bold(19.0))
                            .color(theme::TEXT),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(
                            "Clic droit sur un message puis « Ajouter aux favoris » pour le retrouver ici.",
                        )
                        .size(12.5)
                        .color(theme::TEXT_MUTED),
                    );
                });
                return;
            }

            // Detail of the selected favorite, otherwise the full list.
            let selected = app
                .favorites_selected
                .as_deref()
                .and_then(|id| entries.iter().find(|entry| entry.message_id == id))
                .cloned();
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        ui.add_space(16.0);
                        ui.vertical(|ui| {
                            ui.set_width((ui.available_width() - 32.0).max(200.0));
                            if let Some(saved) = &selected {
                                favorite_detail(app, ui, saved);
                            } else {
                                for saved in &entries {
                                    if favorite_card(ui, saved) {
                                        app.favorites_selected =
                                            Some(saved.message_id.clone());
                                    }
                                    ui.add_space(8.0);
                                }
                            }
                        });
                        ui.add_space(16.0);
                    });
                    ui.add_space(20.0);
                });
        });
}

fn favorite_card(ui: &mut Ui, saved: &fluxer_core::session::SavedMessage) -> bool {
    let response = Frame::default()
        .fill(theme::BG_SIDEBAR)
        .corner_radius(CornerRadius::same(8))
        .stroke(Stroke::new(1.0, theme::BORDER))
        .inner_margin(Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(&saved.author)
                        .font(fonts::bold(13.5))
                        .color(theme::TEXT),
                );
                ui.label(
                    RichText::new(&saved.channel_label)
                        .size(11.0)
                        .color(theme::TEXT_FAINT),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(favorite_date(&saved.timestamp))
                            .size(10.5)
                            .color(theme::TEXT_FAINT),
                    );
                });
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(markdown::plain(&saved.content))
                    .size(13.0)
                    .color(theme::TEXT_MUTED),
            );
        })
        .response;
    response.interact(Sense::click()).clicked()
}

fn favorite_detail(app: &mut FluxerApp, ui: &mut Ui, saved: &fluxer_core::session::SavedMessage) {
    ui.horizontal(|ui| {
        if ui.button("← Tous les favoris").clicked() {
            app.favorites_selected = None;
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add(
                    Button::new(
                        RichText::new("Retirer")
                            .size(12.0)
                            .color(Color32::from_rgb(0xff, 0x9a, 0x9a)),
                    )
                    .fill(Color32::from_rgb(0x3a, 0x1c, 0x1f))
                    .corner_radius(CornerRadius::same(5)),
                )
                .clicked()
            {
                app.favorites
                    .retain(|entry| entry.message_id != saved.message_id);
                app.favorites_selected = None;
                app.save_prefs();
            }
            if ui
                .add(
                    Button::new(RichText::new("Ouvrir le salon").size(12.0))
                        .corner_radius(CornerRadius::same(5)),
                )
                .clicked()
            {
                app.open_favorite(saved);
            }
        });
    });
    ui.add_space(12.0);
    Frame::default()
        .fill(theme::BG_SIDEBAR)
        .corner_radius(CornerRadius::same(8))
        .stroke(Stroke::new(1.0, theme::BORDER))
        .inner_margin(Margin::symmetric(16, 14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(&saved.author)
                        .font(fonts::bold(15.0))
                        .color(theme::TEXT),
                );
                ui.label(
                    RichText::new(&saved.channel_label)
                        .size(11.5)
                        .color(theme::TEXT_FAINT),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(favorite_date(&saved.timestamp))
                            .size(11.0)
                            .color(theme::TEXT_FAINT),
                    );
                });
            });
            ui.add_space(10.0);
            if saved.content.trim().is_empty() {
                ui.label(
                    RichText::new("(message sans texte)")
                        .italics()
                        .size(12.5)
                        .color(theme::TEXT_FAINT),
                );
            } else {
                let mut job = markdown::render(&saved.content, 14.5, theme::TEXT);
                job.wrap.max_width = ui.available_width();
                ui.label(job);
            }
        });
}

fn chat_header(app: &mut FluxerApp, ui: &mut Ui) {
    egui::Panel::top("chat_header")
        .default_size(theme::LAYOUT_HEADER_HEIGHT)
        .min_size(theme::LAYOUT_HEADER_HEIGHT)
        .max_size(theme::LAYOUT_HEADER_HEIGHT)
        .resizable(false)
        .frame(
            Frame::default()
                .fill(theme::BG_SIDEBAR_LIGHTER)
                .inner_margin(Margin::symmetric(16, 0)),
        )
        .show(ui, |ui| {
            let header_height = theme::LAYOUT_HEADER_HEIGHT;
            // Fixed reserve for the actions (search field + three icons), so
            // the topic can never slide underneath them.
            let controls_width = 320.0;
            ui.horizontal_centered(|ui| {
                let left_width = (ui.available_width() - controls_width).max(140.0);
                ui.allocate_ui_with_layout(
                    vec2(left_width, header_height),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        let channel = current_channel(app).cloned();
                        if let Some(channel) = &channel {
                            if channel.kind().is_direct() {
                                // Private conversation: avatar + name + status
                                // instead of the usual "#channel".
                                let media = app.state.media_base.clone();
                                let (avatar_rect, _) =
                                    ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
                                if let Some(user) = channel.dm_user() {
                                    paint_avatar(
                                        ui,
                                        avatar_rect,
                                        user,
                                        &media,
                                        app.state.presence.get(&user.id).map(String::as_str),
                                        theme::BG_PRIMARY,
                                    );
                                }
                                ui.add_space(6.0);
                                ui.label(
                                    RichText::new(channel.direct_label())
                                        .font(fonts::bold(16.0))
                                        .color(theme::TEXT),
                                );
                                let status = channel
                                    .dm_user()
                                    .and_then(|user| app.state.presence.get(&user.id))
                                    .map(|status| match status.as_str() {
                                        "online" => "En ligne",
                                        "idle" => "Absent",
                                        "dnd" => "Ne pas déranger",
                                        _ => "Hors ligne",
                                    })
                                    .unwrap_or("Conversation privée");
                                ui.add_space(6.0);
                                let (line, _) =
                                    ui.allocate_exact_size(vec2(1.0, 20.0), Sense::hover());
                                ui.painter().line_segment(
                                    [line.center_top(), line.center_bottom()],
                                    Stroke::new(1.0, theme::DIVIDER),
                                );
                                ui.add_space(4.0);
                                ui.label(RichText::new(status).size(11.5).color(theme::TEXT_MUTED));
                            } else {
                                let (icon_rect, _) =
                                    ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
                                icons::paint(
                                    ui.painter(),
                                    icon_rect,
                                    channel_icon(channel.kind()),
                                    theme::TEXT_FAINT,
                                    1.8,
                                );
                                ui.add_space(2.0);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(channel.label())
                                            .font(fonts::bold(16.0))
                                            .color(theme::TEXT),
                                    )
                                    .truncate(),
                                );
                                if let Some(topic) = channel.topic.clone().filter(|t| !t.is_empty())
                                {
                                    ui.add_space(6.0);
                                    let (line, _) =
                                        ui.allocate_exact_size(vec2(1.0, 20.0), Sense::hover());
                                    ui.painter().line_segment(
                                        [line.center_top(), line.center_bottom()],
                                        Stroke::new(1.0, theme::DIVIDER),
                                    );
                                    ui.add_space(4.0);
                                    let width = (ui.available_width() - 4.0).max(40.0);
                                    ui.add_sized(
                                        vec2(width, 20.0),
                                        egui::Label::new(
                                            RichText::new(markdown::plain(&topic))
                                                .size(12.5)
                                                .color(theme::TEXT_MUTED),
                                        )
                                        .truncate(),
                                    );
                                }
                            }
                        } else {
                            ui.label(
                                RichText::new("Fluxer")
                                    .font(fonts::bold(16.0))
                                    .color(theme::TEXT),
                            );
                        }
                    },
                );

                ui.allocate_ui_with_layout(
                    vec2(ui.available_width(), header_height),
                    Layout::right_to_left(Align::Center),
                    |ui| {
                        let is_dm = current_channel(app)
                            .map(|channel| channel.kind().is_direct())
                            .unwrap_or(false);
                        if is_dm {
                            if icon_button(ui, Icon::Phone, 30.0, theme::TEXT_MUTED)
                                .on_hover_text("Appel privé (bientôt)")
                                .clicked()
                            {}
                        } else if icon_button(
                            ui,
                            Icon::Members,
                            30.0,
                            if app.state.show_members {
                                theme::TEXT
                            } else {
                                theme::TEXT_MUTED
                            },
                        )
                        .on_hover_text("Liste des membres")
                        .clicked()
                        {
                            app.state.show_members = !app.state.show_members;
                            app.save_prefs();
                        }
                        let _ = icon_button(ui, Icon::Pin, 30.0, theme::TEXT_MUTED)
                            .on_hover_text("Messages épinglés");
                        let _ = icon_button(ui, Icon::Bell, 30.0, theme::TEXT_MUTED)
                            .on_hover_text("Notifications");

                        ui.add_space(6.0);
                        search_field(app, ui);
                    },
                );
            });

            // Bottom hairline.
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
            ui.painter().rect_filled(rect, 0, theme::DIVIDER);
        });
}

fn search_field(app: &mut FluxerApp, ui: &mut Ui) {
    Frame::default()
        .fill(theme::BG_COMPOSER)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.set_width(178.0);
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                icons::paint(ui.painter(), rect, Icon::Search, theme::TEXT_FAINT, 1.6);
                ui.add(
                    TextEdit::singleline(&mut app.search)
                        .hint_text("Rechercher")
                        .frame(Frame::NONE)
                        .desired_width(140.0),
                );
            });
        });
}

// --- appel vocal -----------------------------------------------------------

/// Full-page call view for the current voice channel: participant stage,
/// floating controls and the channel text chat underneath.
fn voice_call(app: &mut FluxerApp, ui: &mut Ui) {
    egui::CentralPanel::default()
        .frame(Frame::default().fill(theme::BG_PRIMARY))
        .show(ui, |ui| {
            let stage_height = (ui.available_height() * 0.52).clamp(250.0, 460.0);
            let (stage, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), stage_height), Sense::hover());
            ui.painter().rect_filled(stage, 0, theme::BG_PRIMARY);

            // Participants: ourselves first, then the gateway voice states.
            let media = app.state.media_base.clone();
            let mut users: Vec<User> = Vec::new();
            if let Some(me) = app.state.me.clone() {
                users.push(me);
            }
            for id in &app.state.voice_members {
                if users.iter().any(|user| &user.id == id) {
                    continue;
                }
                if let Some(member) = app.state.members.iter().find(|m| &m.user.id == id) {
                    users.push(member.user.clone());
                }
            }

            let avatar_size = 76.0;
            let gap = 22.0;
            let total =
                users.len() as f32 * avatar_size + (users.len().saturating_sub(1)) as f32 * gap;
            let mut x = stage.center().x - total / 2.0;
            let y = stage.top() + stage_height * 0.35;
            for user in &users {
                let rect = Rect::from_min_size(pos2(x, y), Vec2::splat(avatar_size));
                // Green speaking ring, matching the official call stage.
                ui.painter().circle_stroke(
                    rect.center(),
                    avatar_size / 2.0 + 4.0,
                    Stroke::new(2.5, theme::ONLINE),
                );
                paint_avatar(ui, rect, user, &media, None, theme::BG_PRIMARY);
                let name = user.display_name();
                ui.painter().text(
                    pos2(rect.center().x, rect.bottom() + 18.0),
                    Align2::CENTER_CENTER,
                    name,
                    fonts::bold(13.0),
                    theme::TEXT,
                );
                x += avatar_size + gap;
            }

            if users.len() <= 1 {
                ui.painter().text(
                    pos2(stage.center().x, y + avatar_size + 58.0),
                    Align2::CENTER_CENTER,
                    if app.state.voice_connected {
                        "Vous êtes connecté — en attente des autres participants…"
                    } else {
                        "Connexion au serveur vocal…"
                    },
                    FontId::proportional(12.0),
                    theme::TEXT_FAINT,
                );
            }

            // Floating control bar.
            let pill_width = 316.0;
            let pill = Rect::from_center_size(
                pos2(stage.center().x, stage.bottom() - 44.0),
                vec2(pill_width, 56.0),
            );
            ui.painter()
                .rect_filled(pill, CornerRadius::same(16), theme::BG_SIDEBAR_LIGHTER);
            let mut controls_ui =
                ui.new_child(UiBuilder::new().max_rect(pill.shrink2(vec2(12.0, 8.0))));
            controls_ui.horizontal_centered(|ui| {
                let muted = app.state.voice_muted;
                let deafened = app.state.voice_deafened;
                if voice_control(
                    ui,
                    Icon::Mic,
                    if muted { theme::DANGER } else { theme::TEXT },
                    if muted {
                        theme::DANGER.gamma_multiply(0.2)
                    } else {
                        theme::BG_TERTIARY
                    },
                ) {
                    app.state.voice_muted = !muted;
                    app.command(Command::SetVoiceState {
                        muted: app.state.voice_muted,
                        deafened: app.state.voice_deafened,
                    });
                    app.save_prefs();
                }
                if voice_control(
                    ui,
                    Icon::Headphones,
                    if deafened { theme::DANGER } else { theme::TEXT },
                    if deafened {
                        theme::DANGER.gamma_multiply(0.2)
                    } else {
                        theme::BG_TERTIARY
                    },
                ) {
                    app.state.voice_deafened = !deafened;
                    app.command(Command::SetVoiceState {
                        muted: app.state.voice_muted,
                        deafened: app.state.voice_deafened,
                    });
                    app.save_prefs();
                }
                let _ = voice_control(ui, Icon::Camera, theme::TEXT, theme::BG_TERTIARY);
                let _ = voice_control(ui, Icon::Screen, theme::TEXT, theme::BG_TERTIARY);
                let _ = voice_control(ui, Icon::More, theme::TEXT, theme::BG_TERTIARY);
                if voice_control(ui, Icon::Phone, Color32::WHITE, theme::DANGER)
                    && let Some(guild_id) = app.state.voice_guild.clone()
                {
                    app.command(Command::LeaveVoice { guild_id });
                }
            });

            // Text chat of the voice channel.
            let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
            ui.painter().rect_filled(line, 0, theme::DIVIDER);
            voice_chat(app, ui);
        });
}

fn voice_control(ui: &mut Ui, icon: Icon, tint: Color32, fill: Color32) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(36.0), Sense::click());
    ui.painter().rect_filled(rect, CornerRadius::same(18), fill);
    icons::paint(ui.painter(), rect.shrink(9.0), icon, tint, 1.8);
    response.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

/// Message list used below the call stage (no channel intro).
fn voice_chat(app: &mut FluxerApp, ui: &mut Ui) {
    let media = app.state.media_base.clone();
    let compact = app.state.compact;
    let joined_guilds: HashSet<String> =
        app.state.guilds.iter().map(|guild| guild.id.clone()).collect();
    let query = app.search.trim().to_lowercase();
    let mut messages: Vec<Message> = app.state.messages.clone();
    if !query.is_empty() {
        messages.retain(|message| {
            message.content.to_lowercase().contains(&query)
                || message
                    .author
                    .display_name()
                    .to_lowercase()
                    .contains(&query)
        });
    }
    let output = ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(app.scroll_to_bottom)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add_space(6.0);
            let mut previous_day = String::new();
            for (index, message) in messages.iter().enumerate() {
                let day = message.timestamp.chars().take(10).collect::<String>();
                if day != previous_day && !day.is_empty() {
                    day_separator(ui, &day);
                    previous_day = day;
                }
                let grouped = if compact {
                    false
                } else {
                    index
                        .checked_sub(1)
                        .and_then(|i| messages.get(i))
                        .is_some_and(|prev| {
                            prev.author.id == message.author.id
                                && same_group(&prev.timestamp, &message.timestamp)
                        })
                };
                message_row(app, ui, message, &media, compact, grouped, &joined_guilds);
            }
            ui.add_space(8.0);
        });
    let offset = output.state.clamped_offset().y;
    let max_offset = (output.content_size.y - output.inner_rect.height()).max(0.0);
    app.scroll_to_bottom = offset >= max_offset - 4.0;
}

// --- timeline --------------------------------------------------------------

fn chat(app: &mut FluxerApp, ui: &mut Ui) {
    egui::CentralPanel::default()
        .frame(Frame::default().fill(theme::BG_SIDEBAR_LIGHTER))
        .show(ui, |ui| {
            let Some(channel) = current_channel(app).cloned() else {
                empty_chat(ui);
                return;
            };

            let query = app.search.trim().to_lowercase();
            let mut messages: Vec<Message> = app.state.messages.clone();
            if !query.is_empty() {
                messages.retain(|m| {
                    m.content.to_lowercase().contains(&query)
                        || m.author.display_name().to_lowercase().contains(&query)
                });
            }

            let stick = app.scroll_to_bottom;
            let compact = app.state.compact;
            let media = app.state.media_base.clone();
            let joined_guilds: HashSet<String> =
                app.state.guilds.iter().map(|guild| guild.id.clone()).collect();

            let scroll_output = ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(stick)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add_space(8.0);

                    if !query.is_empty() {
                        let (rect, _) = ui
                            .allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
                        ui.painter().text(
                            pos2(rect.left() + 16.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            format!("{} résultat(s) pour « {} »", messages.len(), app.search),
                            FontId::proportional(12.5),
                            theme::TEXT_MUTED,
                        );
                    }

                    channel_intro(ui, &channel, &media);

                    if messages.is_empty() {
                        ui.add_space(10.0);
                        if app.state.loading_messages {
                            ui.vertical_centered(|ui| {
                                ui.spinner();
                            });
                        } else {
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new("Aucun message pour le moment.")
                                        .size(13.0)
                                        .color(theme::TEXT_FAINT),
                                );
                            });
                        }
                    }

                    let mut previous_day = String::new();
                    for (index, message) in messages.iter().enumerate() {
                        let day = message.timestamp.chars().take(10).collect::<String>();
                        if day != previous_day && !day.is_empty() {
                            day_separator(ui, &day);
                            previous_day = day;
                        }
                        let grouped = if compact {
                            false
                        } else {
                            index
                                .checked_sub(1)
                                .and_then(|i| messages.get(i))
                                .is_some_and(|prev| {
                                    prev.author.id == message.author.id
                                        && same_group(&prev.timestamp, &message.timestamp)
                                })
                        };
                        message_row(app, ui, message, &media, compact, grouped, &joined_guilds);
                    }

                    ui.add_space(10.0);
                });

            // Pagination automatique : charger la page précédente dès que
            // l'utilisateur approche du haut de l'historique.
            let offset = scroll_output.state.clamped_offset().y;
            let max_offset =
                (scroll_output.content_size.y - scroll_output.inner_rect.height()).max(0.0);
            if offset < 48.0
                && messages.len() >= 20
                && !app.state.loading_messages
                && app.load_older_armed
            {
                app.load_older_armed = false;
                app.command(Command::LoadOlder);
            } else if offset > 160.0 {
                app.load_older_armed = true;
            }

            // Tant que la vue est au bas de l'historique, les images et les
            // cartes d'invitation qui terminent leur chargement ne doivent pas
            // pousser la dernière ligne hors de l'écran.
            app.scroll_to_bottom = offset >= max_offset - 4.0;
        });
}

fn empty_chat(ui: &mut Ui) {
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() * 0.28);
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(72.0), Sense::hover());
        paint_logo(ui, rect);
        ui.add_space(14.0);
        ui.label(
            RichText::new("Bienvenue sur Fluxer-rs")
                .font(fonts::bold(22.0))
                .color(theme::TEXT),
        );
        ui.add_space(6.0);
        ui.label(
            RichText::new("Choisissez un serveur puis un salon pour commencer.")
                .size(13.5)
                .color(theme::TEXT_MUTED),
        );
    });
}

fn channel_intro(ui: &mut Ui, channel: &Channel, media: &str) {
    let _ = media;
    ui.add_space(18.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 78.0), Sense::hover());
    let circle = Rect::from_center_size(
        pos2(rect.left() + 16.0 + 34.0, rect.center().y),
        Vec2::splat(68.0),
    );
    ui.painter()
        .circle_filled(circle.center(), 34.0, theme::BG_TERTIARY);
    let direct = channel.kind().is_direct();
    if direct {
        if let Some(user) = channel.dm_user() {
            paint_avatar(ui, circle.shrink(2.0), user, media, None, theme::BG_PRIMARY);
        }
    } else {
        icons::paint(
            ui.painter(),
            circle.shrink(19.0),
            channel_icon(channel.kind()),
            theme::TEXT,
            2.4,
        );
    }

    ui.painter().text(
        pos2(circle.right() + 16.0, rect.center().y - 12.0),
        Align2::LEFT_CENTER,
        if direct {
            format!(
                "Bienvenue dans votre conversation avec {} !",
                channel.direct_label()
            )
        } else {
            format!("Bienvenue dans #{} !", channel.label())
        },
        fonts::bold(24.0),
        theme::TEXT,
    );
    let subtitle = channel
        .topic
        .as_deref()
        .filter(|t| !t.is_empty())
        .map(markdown::plain)
        .unwrap_or_else(|| {
            if direct {
                "C'est le début de votre historique de messages privés.".to_string()
            } else {
                format!(
                    "C'est le début du salon #{}. Soyez sympa et discutons.",
                    channel.label()
                )
            }
        });
    ui.painter().text(
        pos2(circle.right() + 16.0, rect.center().y + 14.0),
        Align2::LEFT_CENTER,
        subtitle,
        FontId::proportional(13.0),
        theme::TEXT_MUTED,
    );
    ui.add_space(14.0);
}

fn same_group(a: &str, b: &str) -> bool {
    let (Ok(a), Ok(b)) = (
        chrono::DateTime::parse_from_rfc3339(a),
        chrono::DateTime::parse_from_rfc3339(b),
    ) else {
        return false;
    };
    let delta = b.signed_duration_since(a).num_minutes();
    (0..7).contains(&delta)
}

fn day_separator(ui: &mut Ui, day: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
    let y = rect.center().y;
    let text = pretty_day(day);
    let galley = ui
        .painter()
        .layout_no_wrap(text, fonts::bold(10.5), theme::TEXT_FAINT);
    let half = galley.size().x / 2.0 + 12.0;
    ui.painter().line_segment(
        [pos2(rect.left() + 16.0, y), pos2(rect.center().x - half, y)],
        Stroke::new(1.0, theme::DIVIDER),
    );
    ui.painter().line_segment(
        [
            pos2(rect.center().x + half, y),
            pos2(rect.right() - 16.0, y),
        ],
        Stroke::new(1.0, theme::DIVIDER),
    );
    ui.painter().galley(
        pos2(
            rect.center().x - galley.size().x / 2.0,
            y - galley.size().y / 2.0,
        ),
        galley,
        theme::TEXT_FAINT,
    );
}

fn pretty_day(day: &str) -> String {
    const MONTHS: [&str; 12] = [
        "janvier",
        "février",
        "mars",
        "avril",
        "mai",
        "juin",
        "juillet",
        "août",
        "septembre",
        "octobre",
        "novembre",
        "décembre",
    ];
    match chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d") {
        Ok(date) => {
            use chrono::Datelike;
            let today = chrono::Local::now().date_naive();
            if date == today {
                "Aujourd'hui".to_string()
            } else if today.pred_opt() == Some(date) {
                "Hier".to_string()
            } else {
                format!(
                    "{} {} {}",
                    date.day(),
                    MONTHS[(date.month0() as usize).min(11)],
                    date.year()
                )
            }
        }
        Err(_) => day.to_string(),
    }
}

fn message_row(
    app: &mut FluxerApp,
    ui: &mut Ui,
    message: &Message,
    media: &str,
    compact: bool,
    grouped: bool,
    joined_guilds: &HashSet<String>,
) {
    if compact {
        compact_message(ui, message, media);
        return;
    }

    let width = ui.available_width();
    // Grille officielle : marge 16 px + avatar 40 px + gouttière 12 px.
    let content_x = 16.0 + theme::MESSAGE_AVATAR_SIZE + 12.0;
    let wrap = (width - content_x - 16.0).max(120.0);

    // Les cartes d'invitation sont chargées une seule fois puis mises en cache.
    for code in crate::app::invite_codes(&message.content) {
        app.ensure_invite(&code);
    }

    let mut job = markdown::render(&message.content, TEXT_SIZE, theme::TEXT);
    job.wrap.max_width = wrap;
    let body = if message.content.trim().is_empty() {
        None
    } else {
        Some(ui.painter().layout_job(job))
    };
    let body_h = body.as_ref().map(|g| g.size().y).unwrap_or(0.0);
    let reply_h = if message.referenced_message.is_some() {
        20.0
    } else {
        0.0
    };

    let blocks = rich::plan(
        ui,
        message,
        wrap,
        &app.invites,
        &app.invite_pending,
        joined_guilds,
        media,
    );
    let blocks_h = if blocks.is_empty() {
        0.0
    } else {
        blocks.iter().map(|block| block.height()).sum::<f32>()
            + blocks.len().saturating_sub(1) as f32 * rich::BLOCK_GAP
            + rich::BLOCK_GAP
    };

    let reactions_h = if message.reactions.is_empty() {
        0.0
    } else {
        30.0
    };
    let header_h = if grouped { 0.0 } else { 22.0 };
    // 16 px d'espace entre deux groupes + 2 px de rembourrage officiel.
    let top_pad = if grouped { 2.0 } else { 18.0 };
    let text_h = header_h + reply_h + body_h + blocks_h + reactions_h;
    let height = top_pad + text_h.max(theme::MESSAGE_AVATAR_SIZE) + 2.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());

    if response.hovered() {
        ui.painter().rect_filled(rect, 0, theme::MESSAGE_HOVER);
    }

    let content_top = rect.top() + top_pad;
    if !grouped {
        let avatar_rect = Rect::from_min_size(
            pos2(rect.left() + 16.0, content_top + 2.0),
            Vec2::splat(theme::MESSAGE_AVATAR_SIZE),
        );
        paint_avatar(
            ui,
            avatar_rect,
            &message.author,
            media,
            None,
            theme::BG_PRIMARY,
        );
    }

    let mut y = content_top;
    if !grouped {
        let name = message.author.display_name();
        let color = theme::name_color(&message.author.id);
        let name_rect = ui.painter().text(
            pos2(rect.left() + content_x, y),
            Align2::LEFT_TOP,
            name,
            fonts::bold(NAME_SIZE),
            color,
        );
        let mut x = name_rect.right() + 8.0;
        if message.author.bot.unwrap_or(false) {
            let bot_rect = Rect::from_min_size(pos2(x, y + 1.0), vec2(34.0, 15.0));
            ui.painter()
                .rect_filled(bot_rect, CornerRadius::same(3), theme::ACCENT);
            ui.painter().text(
                bot_rect.center(),
                Align2::CENTER_CENTER,
                "BOT",
                fonts::bold(9.0),
                Color32::WHITE,
            );
            x += 40.0;
        }
        let time = message.short_time();
        let time_rect = ui.painter().text(
            pos2(x, y + 3.0),
            Align2::LEFT_TOP,
            time,
            FontId::proportional(TIME_SIZE),
            theme::TEXT_FAINT,
        );
        if message.is_edited() {
            ui.painter().text(
                pos2(time_rect.right() + 5.0, y + 3.0),
                Align2::LEFT_TOP,
                "(modifié)",
                FontId::proportional(TIME_SIZE),
                theme::TEXT_FAINT,
            );
        }
        if message.pinned.unwrap_or(false) {
            ui.painter().text(
                pos2(time_rect.right() + 70.0, y + 3.0),
                Align2::LEFT_TOP,
                "📌 épinglé",
                FontId::proportional(TIME_SIZE),
                theme::TEXT_MUTED,
            );
        }
        y += header_h;
    }

    if let Some(reference) = &message.referenced_message {
        let author = reference.author.display_name();
        let excerpt: String = reference
            .content
            .replace('\n', " ")
            .chars()
            .take(70)
            .collect();
        let text = format!("↪ {author} : {excerpt}");
        ui.painter()
            .with_clip_rect(Rect::from_min_max(
                pos2(rect.left() + content_x, y),
                pos2(rect.right() - 16.0, y + 18.0),
            ))
            .text(
                pos2(rect.left() + content_x, y + 1.0),
                Align2::LEFT_TOP,
                text,
                FontId::proportional(11.5),
                theme::TEXT_MUTED,
            );
        y += reply_h;
    }

    if let Some(galley) = body {
        let height = galley.size().y;
        ui.painter()
            .galley(pos2(rect.left() + content_x, y), galley, theme::TEXT);
        y += height;
    }

    for block in &blocks {
        y += rich::BLOCK_GAP;
        block.draw(ui, app, pos2(rect.left() + content_x, y), media);
        y += block.height();
    }

    // Pastilles de réactions : chaque clic bascule la réaction de l'utilisateur.
    if !message.reactions.is_empty() {
        let row = Rect::from_min_size(pos2(rect.left() + content_x, y + 3.0), vec2(wrap, 24.0));
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(row)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for reaction in &message.reactions {
                    let label = format!("{} {}", reaction.emoji.name, reaction.count);
                    let (fill, stroke) = if reaction.me {
                        (theme::ACCENT_SOFT, theme::ACCENT)
                    } else {
                        (theme::BG_COMPOSER, theme::BORDER)
                    };
                    let button =
                        Button::new(RichText::new(label).size(12.0).color(if reaction.me {
                            theme::TEXT
                        } else {
                            theme::TEXT_MUTED
                        }))
                        .fill(fill)
                        .stroke(Stroke::new(1.0, stroke))
                        .corner_radius(CornerRadius::same(10))
                        .min_size(vec2(0.0, 22.0));
                    if ui
                        .add(button)
                        .on_hover_text(if reaction.me {
                            "Retirer votre réaction"
                        } else {
                            "Réagir"
                        })
                        .clicked()
                    {
                        app.toggle_reaction(message, &reaction.emoji.query());
                    }
                }
            },
        );
    }

    response.context_menu(|ui| {
        if ui.button("Répondre").clicked() {
            app.start_reply(message);
            ui.close();
        }
        if ui.button("Créer un fil").clicked() {
            app.start_thread(message);
            ui.close();
        }
        let pinned = message.pinned.unwrap_or(false);
        if ui
            .button(if pinned {
                "Désépingler le message"
            } else {
                "Épingler le message"
            })
            .clicked()
        {
            app.toggle_pin(message);
            ui.close();
        }
        let saved = app.is_favorite(&message.id);
        let label = if saved {
            "Retirer des favoris"
        } else {
            "Ajouter aux favoris"
        };
        if app.state.enable_favorites && ui.button(label).clicked() {
            app.toggle_favorite(message);
            ui.close();
        }
        ui.separator();
        ui.label(RichText::new("Réagir").size(11.0).color(theme::TEXT_FAINT));
        ui.horizontal(|ui| {
            for emoji in ["👍", "❤️", "😂", "🎉", "😮", "😢"] {
                if ui
                    .add(Button::new(RichText::new(emoji).size(15.0)).frame(false))
                    .clicked()
                {
                    app.toggle_reaction(message, emoji);
                    ui.close();
                }
            }
        });
        let own = app
            .state
            .me
            .as_ref()
            .is_some_and(|me| me.id == message.author.id);
        if own {
            ui.separator();
            if ui.button("Modifier le message").clicked() {
                app.start_edit(&message.channel_id, &message.id, &message.content);
                ui.close();
            }
            if ui
                .button(RichText::new("Supprimer le message").color(theme::DANGER))
                .clicked()
            {
                app.request_delete(&message.channel_id, &message.id);
                ui.close();
            }
        }
        if !message.author.id.is_empty() {
            ui.separator();
        }
        if ui.button("Copier le texte").clicked() {
            ui.ctx().copy_text(message.content.clone());
            ui.close();
        }
        if !message.author.id.is_empty() && ui.button("Copier l'identifiant de l'auteur").clicked()
        {
            ui.ctx().copy_text(message.author.id.clone());
            ui.close();
        }
    });
}

fn compact_message(ui: &mut Ui, message: &Message, media: &str) {
    let width = ui.available_width();
    let text_x = 88.0;
    let wrap = (width - text_x - 16.0).max(120.0);

    let content = markdown::render(&message.content, 14.0, theme::TEXT);
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap;
    job.append(
        message.author.display_name(),
        0.0,
        egui::TextFormat {
            font_id: fonts::bold(14.0),
            color: theme::name_color(&message.author.id),
            line_height: Some(19.0),
            ..Default::default()
        },
    );
    job.append(
        "  ",
        0.0,
        egui::TextFormat {
            font_id: FontId::proportional(14.0),
            line_height: Some(19.0),
            ..Default::default()
        },
    );
    for section in &content.sections {
        job.append(
            &content.text[section.byte_range.start.0..section.byte_range.end.0],
            0.0,
            section.format.clone(),
        );
    }
    for attachment in &message.attachments {
        job.append(
            &format!("  📎 {}", attachment.filename),
            0.0,
            egui::TextFormat {
                font_id: FontId::proportional(13.0),
                color: theme::LINK,
                line_height: Some(19.0),
                ..Default::default()
            },
        );
    }
    let galley = ui.painter().layout_job(job);
    let height = galley.size().y.max(20.0) + 4.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    if response.hovered() {
        ui.painter().rect_filled(rect, 0, theme::MESSAGE_HOVER);
    }

    ui.painter().text(
        pos2(rect.left() + 16.0, rect.top() + 4.0),
        Align2::LEFT_TOP,
        message.short_time(),
        FontId::proportional(10.5),
        theme::TEXT_FAINT,
    );
    paint_avatar(
        ui,
        Rect::from_min_size(
            pos2(rect.left() + 60.0, rect.top() + 3.0),
            Vec2::splat(20.0),
        ),
        &message.author,
        media,
        None,
        theme::BG_PRIMARY,
    );
    ui.painter().galley(
        pos2(rect.left() + text_x, rect.top() + 2.0),
        galley,
        theme::TEXT,
    );
}


fn composer(app: &mut FluxerApp, ui: &mut Ui) {
    egui::Panel::bottom("composer")
        .frame(
            Frame::default()
                .fill(theme::BG_SIDEBAR_LIGHTER)
                .inner_margin(Margin {
                    left: 10,
                    right: 10,
                    top: 6,
                    bottom: 6,
                }),
        )
        .show(ui, |ui| {
            let Some(channel) = current_channel(app).cloned() else {
                return;
            };

            // Bandeau d'édition : rappelle le message en cours de modification.
            if app.editing.is_some() {
                Frame::default()
                    .fill(theme::BG_SIDEBAR_LIGHTER)
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("Modification du message")
                                    .font(fonts::bold(12.0))
                                    .color(theme::TEXT),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui
                                    .add(
                                        Button::new(RichText::new("Annuler").size(11.5))
                                            .corner_radius(CornerRadius::same(4)),
                                    )
                                    .clicked()
                                {
                                    app.cancel_edit();
                                }
                            });
                        });
                    });
                ui.add_space(4.0);
            }

            // Bandeau de réponse citée.
            let reply_preview = app
                .replying
                .as_ref()
                .map(|reply| (reply.author.clone(), reply.excerpt.clone()));
            if app.editing.is_none()
                && let Some((author, excerpt)) = reply_preview
            {
                Frame::default()
                    .fill(theme::BG_SIDEBAR_LIGHTER)
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("Réponse à {author}"))
                                    .font(fonts::bold(12.0))
                                    .color(theme::TEXT),
                            );
                            ui.label(RichText::new(excerpt).size(11.5).color(theme::TEXT_MUTED));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui
                                    .add(
                                        Button::new(RichText::new("Annuler").size(11.5))
                                            .corner_radius(CornerRadius::same(4)),
                                    )
                                    .clicked()
                                {
                                    app.cancel_reply();
                                }
                            });
                        });
                    });
                ui.add_space(4.0);
            }

            // Ligne d'état au-dessus du composeur, comme `statusRail` côté
            // officiel : uniquement l'indicateur de saisie (24 px réservés).
            let typing_label = app.state.current_channel.as_ref().and_then(|channel| {
                app.typing.get(channel).and_then(|(user_id, instant)| {
                    if instant.elapsed() > std::time::Duration::from_secs(8) {
                        None
                    } else {
                        let name = app
                            .state
                            .members
                            .iter()
                            .find(|member| member.user.id == *user_id)
                            .map(|member| member.display_name().to_string())
                            .unwrap_or_else(|| user_id.clone());
                        Some(format!("{name} écrit…"))
                    }
                })
            });
            if let Some(label) = typing_label {
                let (status, _) = ui.allocate_exact_size(
                    vec2(ui.available_width(), theme::LAYOUT_STATUS_ROW_HEIGHT),
                    Sense::hover(),
                );
                ui.painter().text(
                    pos2(status.left() + 2.0, status.center().y),
                    Align2::LEFT_CENTER,
                    label,
                    FontId::proportional(11.5),
                    theme::TEXT_MUTED,
                );
            }

            Frame::default()
                .fill(theme::BG_COMPOSER)
                .corner_radius(CornerRadius::same(8))
                .stroke(Stroke::new(1.0, theme::BORDER))
                .inner_margin(Margin {
                    left: 8,
                    right: 8,
                    top: 0,
                    bottom: 0,
                })
                .show(ui, |ui| {
                    // Boîte de 58 px de haut au total (56 px de contenu + 2 px
                    // de bordure), comme `--footer-box-height` côté officiel.
                    ui.allocate_ui_with_layout(
                        vec2(
                            ui.available_width(),
                            theme::LAYOUT_COMPOSER_BOX_HEIGHT - 2.0,
                        ),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            let _ = icon_button(ui, Icon::Plus, 32.0, theme::TEXT_MUTED)
                                .on_hover_text("Joindre un fichier");
                            ui.add_space(4.0);
                            let editing = app.editing.is_some();
                            let buffer = if editing {
                                &mut app.edit_draft
                            } else {
                                &mut app.draft
                            };
                            let response = ui.add(
                                TextEdit::multiline(buffer)
                                    .hint_text(if editing {
                                        "Modifier le message…".to_string()
                                    } else if channel.kind().is_direct() {
                                        format!("Envoyer un message à {}", channel.direct_label())
                                    } else {
                                        format!("Envoyer un message dans #{}", channel.label())
                                    })
                                    .desired_rows(1)
                                    .desired_width(f32::INFINITY)
                                    .frame(Frame::NONE)
                                    .text_color(theme::TEXT),
                            );
                            let enter = response.has_focus()
                                && ui.ctx().input(|input| {
                                    input.key_pressed(Key::Enter) && !input.modifiers.shift
                                });
                            if enter {
                                if editing {
                                    app.commit_edit();
                                } else {
                                    app.send_draft();
                                }
                                response.request_focus();
                            }
                            if editing && ui.ctx().input(|input| input.key_pressed(Key::Escape)) {
                                app.cancel_edit();
                            }
                            if !editing && !app.draft.trim().is_empty() {
                                app.indicate_typing();
                            }
                            let _ = icon_button(ui, Icon::Gift, 32.0, theme::TEXT_MUTED)
                                .on_hover_text("Cadeau");
                            let _ = icon_button(ui, Icon::Gif, 32.0, theme::TEXT_MUTED)
                                .on_hover_text("GIF");
                            let _ = icon_button(ui, Icon::Emoji, 32.0, theme::TEXT_MUTED)
                                .on_hover_text("Émojis")
                                .clicked()
                                .then(|| {
                                    app.show_emoji_picker = !app.show_emoji_picker;
                                });
                        },
                    );
                });
        });
}

fn gateway_label(status: GatewayStatus) -> &'static str {
    match status {
        GatewayStatus::Online => "temps réel connecté",
        GatewayStatus::Connecting => "connexion…",
        GatewayStatus::Reconnecting => "reconnexion…",
        GatewayStatus::Offline => "temps réel hors ligne",
    }
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

/// Entrée de navigation des réglages : libellé, icône et onglet cible.
type SettingsEntry = (&'static str, Icon, usize);
/// Groupe de navigation : titre et entrées.
type SettingsGroup = (&'static str, &'static [SettingsEntry]);

fn settings_window(app: &mut FluxerApp, ctx: &egui::Context) {
    if ctx.input(|i| i.key_pressed(Key::Escape)) {
        app.show_settings = false;
    }

    let mut open = app.show_settings;
    egui::Window::new("Réglages")
        .open(&mut open)
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .fixed_size(vec2(1000.0, 700.0))
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .frame(
            Frame::default()
                .fill(theme::BG_SIDEBAR)
                .corner_radius(CornerRadius::same(12))
                .stroke(Stroke::new(1.0, theme::BORDER))
                .inner_margin(Margin::same(0)),
        )
        .show(ctx, |ui| {
            ui.set_min_width(1000.0);
            ui.set_min_height(700.0);
            // Header.
            let (header, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::hover());
            ui.painter().line_segment(
                [header.left_bottom(), header.right_bottom()],
                Stroke::new(1.0, theme::DIVIDER),
            );
            ui.painter().text(
                pos2(header.left() + 20.0, header.center().y),
                Align2::LEFT_CENTER,
                "Réglages",
                fonts::bold(17.0),
                theme::TEXT,
            );
            let close_rect = Rect::from_center_size(
                pos2(header.right() - 26.0, header.center().y),
                Vec2::splat(30.0),
            );
            if icon_button_at(ui, close_rect, Icon::Close, theme::TEXT_MUTED) {
                app.show_settings = false;
            }

            egui::Panel::left("settings_nav")
                .exact_size(272.0)
                .resizable(false)
                .frame(
                    Frame::default()
                        .fill(theme::BG_SIDEBAR)
                        .inner_margin(Margin::symmetric(8, 0)),
                )
                .show(ui, |ui| {
                    ui.add_space(12.0);

                    // Recherche globale dans les réglages.
                    Frame::default()
                        .fill(theme::BG_COMPOSER)
                        .corner_radius(CornerRadius::same(6))
                        .stroke(Stroke::new(1.0, theme::BORDER))
                        .inner_margin(Margin::symmetric(9, 6))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (icon, _) =
                                    ui.allocate_exact_size(Vec2::splat(13.0), Sense::hover());
                                icons::paint(
                                    ui.painter(),
                                    icon,
                                    Icon::Search,
                                    theme::TEXT_FAINT,
                                    1.6,
                                );
                                ui.add(
                                    TextEdit::singleline(&mut app.settings_search)
                                        .hint_text("Rechercher dans les réglages…")
                                        .frame(Frame::NONE)
                                        .desired_width(190.0),
                                );
                            });
                        });
                    ui.add_space(10.0);

                    // Carte du compte connecté.
                    if let Some(me) = app.state.me.clone() {
                        let media = app.state.media_base.clone();
                        let (card, response) = ui.allocate_exact_size(
                            vec2(ui.available_width() - 16.0, 50.0),
                            Sense::click(),
                        );
                        let card = card.translate(vec2(8.0, 0.0));
                        if response.hovered() {
                            ui.painter().rect_filled(
                                card,
                                CornerRadius::same(6),
                                theme::CHANNEL_HOVER,
                            );
                        }
                        paint_avatar(
                            ui,
                            Rect::from_min_size(
                                pos2(card.left() + 6.0, card.center().y - 16.0),
                                Vec2::splat(32.0),
                            ),
                            &me,
                            &media,
                            app.state.presence.get(&me.id).map(String::as_str),
                            theme::BG_SIDEBAR,
                        );
                        ui.painter().text(
                            pos2(card.left() + 46.0, card.center().y - 8.0),
                            Align2::LEFT_CENTER,
                            me.display_name(),
                            fonts::bold(13.5),
                            theme::TEXT,
                        );
                        ui.painter().text(
                            pos2(card.left() + 46.0, card.center().y + 8.0),
                            Align2::LEFT_CENTER,
                            format!("@{}", me.username),
                            FontId::proportional(11.0),
                            theme::TEXT_FAINT,
                        );
                        if response.clicked() {
                            app.settings_tab = 0;
                        }
                    }
                    ui.add_space(6.0);

                    let groups: [SettingsGroup; 5] = [
                        (
                            "VOTRE COMPTE",
                            &[
                                ("Compte", Icon::Members, 0),
                                ("Profil", Icon::Eye, 1),
                                ("Confidentialité", Icon::Eye, 1),
                                ("Connexions", Icon::Inbox, 8),
                            ],
                        ),
                        (
                            "FACTURATION",
                            &[
                                ("Fluxer Plutonium", Icon::Star, 9),
                                ("Cadeaux", Icon::Gift, 9),
                            ],
                        ),
                        (
                            "PARAMÈTRES DE L'APPLICATION",
                            &[
                                ("Apparence", Icon::Eye, 2),
                                ("Notifications", Icon::Bell, 4),
                                ("Discussion", Icon::Inbox, 3),
                                ("Audio et vidéo", Icon::Volume, 5),
                                ("Accessibilité", Icon::Eye, 3),
                                ("Langue", Icon::Compass, 7),
                                ("Raccourcis", Icon::Screen, 6),
                                ("Application Linux", Icon::Settings, 8),
                                ("Avancé", Icon::Settings, 9),
                            ],
                        ),
                        (
                            "DÉVELOPPEUR",
                            &[
                                ("Applications", Icon::Screen, 9),
                                ("Débogueur de contenus intégrés", Icon::Search, 9),
                                ("Système de design", Icon::Compass, 10),
                            ],
                        ),
                        ("À PROPOS", &[("À propos de Fluxer-rs", Icon::Help, 10)]),
                    ];

                    let query = app.settings_search.trim().to_lowercase();
                    ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.add_space(4.0);
                            for (group, entries) in groups {
                                let visible: Vec<_> = entries
                                    .iter()
                                    .filter(|(label, _, _)| {
                                        query.is_empty() || label.to_lowercase().contains(&query)
                                    })
                                    .collect();
                                if visible.is_empty() {
                                    continue;
                                }
                                if query.is_empty() {
                                    ui.add_space(8.0);
                                    ui.label(
                                        RichText::new(group)
                                            .font(fonts::bold(10.0))
                                            .color(theme::TEXT_FAINT),
                                    );
                                    ui.add_space(4.0);
                                }
                                for (label, icon, index) in visible {
                                    let selected = app.settings_tab == *index;
                                    let (rect, response) = ui.allocate_exact_size(
                                        vec2(ui.available_width() - 16.0, 34.0),
                                        Sense::click(),
                                    );
                                    let row = rect.translate(vec2(8.0, 0.0));
                                    if selected {
                                        ui.painter().rect_filled(
                                            row,
                                            CornerRadius::same(6),
                                            theme::CHANNEL_ACTIVE,
                                        );
                                    } else if response.hovered() {
                                        ui.painter().rect_filled(
                                            row,
                                            CornerRadius::same(6),
                                            theme::CHANNEL_HOVER,
                                        );
                                    }
                                    let icon_rect = Rect::from_center_size(
                                        pos2(row.left() + 18.0, row.center().y),
                                        Vec2::splat(16.0),
                                    );
                                    icons::paint(
                                        ui.painter(),
                                        icon_rect,
                                        *icon,
                                        if selected {
                                            theme::TEXT
                                        } else {
                                            theme::TEXT_MUTED
                                        },
                                        1.5,
                                    );
                                    ui.painter().text(
                                        pos2(row.left() + 36.0, row.center().y),
                                        Align2::LEFT_CENTER,
                                        *label,
                                        if selected {
                                            fonts::bold(13.5)
                                        } else {
                                            FontId::proportional(13.5)
                                        },
                                        if selected {
                                            theme::TEXT
                                        } else {
                                            theme::TEXT_MUTED
                                        },
                                    );
                                    if response.clicked() {
                                        app.settings_tab = *index;
                                    }
                                    ui.add_space(1.0);
                                }
                            }
                            ui.add_space(16.0);
                        });
                });

            egui::CentralPanel::default()
                .frame(Frame::default().fill(theme::BG_PRIMARY))
                .show(ui, |ui| {
                    ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width() - 8.0);
                            ui.add_space(16.0);
                            ui.label(
                                RichText::new(settings_title(app.settings_tab))
                                    .font(fonts::bold(21.0))
                                    .color(theme::TEXT),
                            );
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(settings_subtitle(app.settings_tab))
                                    .size(12.5)
                                    .color(theme::TEXT_MUTED),
                            );
                            ui.add_space(16.0);
                            match app.settings_tab {
                                0 => settings_account(app, ui),
                                1 => settings_profile(app, ui),
                                2 => settings_appearance(app, ui),
                                3 => settings_chat(app, ui),
                                4 => settings_notifications(app, ui),
                                5 => settings_voice(app, ui),
                                6 => settings_shortcuts(ui),
                                7 => settings_language(app, ui),
                                8 => settings_connection(app, ui),
                                9 => settings_advanced(app, ui),
                                _ => settings_about(ui),
                            }
                            ui.add_space(20.0);
                        });
                });
        });

    app.show_settings = open;
}

fn settings_title(tab: usize) -> &'static str {
    match tab {
        0 => "Compte",
        1 => "Profil",
        2 => "Apparence",
        3 => "Discussion",
        4 => "Notifications",
        5 => "Audio et vidéo",
        6 => "Raccourcis",
        7 => "Langue",
        8 => "Application Linux",
        9 => "Avancé",
        _ => "À propos",
    }
}

fn settings_subtitle(tab: usize) -> &'static str {
    match tab {
        0 => "Votre session, votre instance et votre identité Fluxer.",
        1 => "Personnalisation du profil et aperçu en direct.",
        2 => "Thème, densité et confort de lecture.",
        3 => "Affichage des messages, favoris et historique.",
        4 => "Alertes de bureau et sons de notification.",
        5 => "Périphériques, sourdine et session vocale.",
        6 => "Raccourcis clavier de l'application native.",
        7 => "Langue de l'interface.",
        8 => "Intégration au bureau GNU/Linux et état de la connexion.",
        9 => "Diagnostic, cache et informations techniques.",
        _ => "À propos de ce client natif.",
    }
}

fn settings_account(app: &mut FluxerApp, ui: &mut Ui) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if let Some(me) = app.state.me.clone() {
            let media = app.state.media_base.clone();
            paint_avatar(
                ui,
                Rect::from_min_size(ui.cursor().min, Vec2::splat(80.0)),
                &me,
                &media,
                app.state.presence.get(&me.id).map(String::as_str),
                theme::BG_SIDEBAR,
            );
            ui.add_space(88.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(me.display_name())
                        .font(fonts::bold(22.0))
                        .color(theme::TEXT),
                );
                ui.label(
                    RichText::new(format!("@{}", me.username))
                        .size(13.0)
                        .color(theme::TEXT_MUTED),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!("Identifiant : {}", me.id))
                        .size(11.5)
                        .color(theme::TEXT_FAINT),
                );
            });
        }
    });

    ui.add_space(24.0);
    settings_separator(ui);
    settings_section(ui, "Session");
    ui.label(
        RichText::new(format!("Instance : {}", app.login.instance))
            .size(13.0)
            .color(theme::TEXT_MUTED),
    );
    ui.label(
        RichText::new(format!("Passerelle : {}", gateway_label(app.state.gateway)))
            .size(13.0)
            .color(theme::TEXT_MUTED),
    );
    ui.add_space(12.0);
    if ui
        .add(Button::new("Importer la session Fluxer installée").min_size(vec2(260.0, 34.0)))
        .clicked()
    {
        app.command(Command::ImportInstalled);
    }
    ui.add_space(8.0);
    if ui
        .add(
            Button::new(RichText::new("Se déconnecter").color(Color32::from_rgb(0xff, 0x9a, 0x9a)))
                .fill(Color32::from_rgb(0x3a, 0x1c, 0x1f))
                .min_size(vec2(180.0, 34.0)),
        )
        .clicked()
    {
        app.logout();
        app.show_settings = false;
    }
}

fn settings_appearance(app: &mut FluxerApp, ui: &mut Ui) {
    settings_section(ui, "Affichage des messages");
    let mut compact = app.state.compact;
    if ui.checkbox(&mut compact, "Mode compact").changed() {
        app.state.compact = compact;
        app.save_compact();
    }
    ui.label(
        RichText::new(
            "Affiche chaque message sur une seule ligne, sans avatar, comme le mode compact officiel.",
        )
        .size(12.0)
        .color(theme::TEXT_FAINT),
    );
    ui.add_space(14.0);
    settings_separator(ui);
    settings_section(ui, "Thème");
    ui.label(
        RichText::new("Thème sombre Fluxer (couleurs du client officiel)")
            .size(13.0)
            .color(theme::TEXT_MUTED),
    );
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        for (name, color) in [
            ("Fond", theme::BG_PRIMARY),
            ("Panneau", theme::BG_SIDEBAR),
            ("Accent", theme::ACCENT),
        ] {
            let (rect, _) = ui.allocate_exact_size(vec2(96.0, 54.0), Sense::hover());
            ui.painter().rect_filled(rect, CornerRadius::same(6), color);
            ui.painter().text(
                pos2(rect.left() + 8.0, rect.bottom() - 12.0),
                Align2::LEFT_CENTER,
                name,
                FontId::proportional(10.5),
                theme::TEXT_MUTED,
            );
        }
    });
    ui.add_space(14.0);
    settings_separator(ui);
    settings_section(ui, "Police");
    ui.label(
        RichText::new(
            "Noto Sans remplace « Fluxer Sans » (woff2 non lisible par egui) ; rendu identique en métriques.",
        )
        .size(12.0)
        .color(theme::TEXT_FAINT),
    );
}

fn settings_profile(app: &mut FluxerApp, ui: &mut Ui) {
    let Some(me) = app.state.me.clone() else {
        return;
    };
    let media = app.state.media_base.clone();

    settings_section(ui, "Type de profil");
    Frame::default()
        .fill(theme::BG_COMPOSER)
        .corner_radius(CornerRadius::same(6))
        .stroke(Stroke::new(1.0, theme::BORDER))
        .inner_margin(Margin::symmetric(10, 7))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Profil global").size(13.0).color(theme::TEXT));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let (icon, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                    icons::paint(
                        ui.painter(),
                        icon,
                        Icon::ChevronDown,
                        theme::TEXT_MUTED,
                        1.6,
                    );
                });
            });
        });

    ui.add_space(14.0);
    settings_separator(ui);

    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            vec2(390.0, 0.0),
            Layout::top_down(Align::Min),
            |ui| {
            ui.set_width(378.0);
            ui.label(
                RichText::new("Personnalisation du profil")
                    .font(fonts::bold(18.0))
                    .color(theme::TEXT),
            );
            ui.label(
                RichText::new("Modifiez l'apparence de votre profil et voyez un aperçu en direct.")
                    .size(12.0)
                    .color(theme::TEXT_MUTED),
            );
            ui.add_space(16.0);

            ui.label(
                RichText::new("Nom d'utilisateur")
                    .font(fonts::bold(12.5))
                    .color(theme::TEXT),
            );
            ui.add_space(4.0);
            let _ = ui
                .add(
                    Button::new(RichText::new("Changer de nom d'utilisateur").size(12.5))
                        .fill(theme::ACCENT)
                        .corner_radius(CornerRadius::same(5))
                        .min_size(vec2(220.0, 30.0)),
                );
            ui.add_space(14.0);

            profile_field(ui, "Nom affiché", &mut app.profile_display);
            profile_field(ui, "Pronoms", &mut app.profile_pronouns);

            ui.label(
                RichText::new("Bio")
                    .font(fonts::bold(12.5))
                    .color(theme::TEXT),
            );
            ui.add_space(4.0);
            ui.add(
                TextEdit::multiline(&mut app.profile_bio)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .hint_text("Quelques mots sur vous…")
                    .text_color(theme::TEXT),
            );
            ui.add_space(10.0);
            if ui
                .add(
                    Button::new(RichText::new("Enregistrer le profil").size(12.5))
                        .fill(theme::ACCENT)
                        .corner_radius(CornerRadius::same(5))
                        .min_size(vec2(190.0, 30.0)),
                )
                .clicked()
            {
                app.save_profile();
            }
            ui.add_space(16.0);

            ui.label(
                RichText::new("Heure locale du profil")
                    .font(fonts::bold(12.5))
                    .color(theme::TEXT),
            );
            ui.label(
                RichText::new(
                    "Définissez votre fuseau horaire une fois pour que Fluxer puisse maintenir à jour votre décalage UTC lorsque l'heure d'été change.",
                )
                .size(11.5)
                .color(theme::TEXT_MUTED),
            );
            ui.add_space(6.0);
            let _ = ui
                .add(
                    Button::new(RichText::new("Modifier l'heure locale du profil").size(12.0))
                        .corner_radius(CornerRadius::same(5))
                        .min_size(vec2(240.0, 28.0)),
                );
            ui.add_space(16.0);

            ui.label(
                RichText::new("Avatar")
                    .font(fonts::bold(12.5))
                    .color(theme::TEXT),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add(
                    TextEdit::singleline(&mut app.avatar_path_input)
                        .hint_text("Chemin local de l'image…")
                        .desired_width(220.0),
                );
                if ui
                    .add(
                        Button::new(RichText::new("Appliquer").size(12.0))
                            .fill(theme::ACCENT)
                            .corner_radius(CornerRadius::same(5)),
                    )
                    .clicked()
                {
                    let path = app.avatar_path_input.clone();
                    let path = std::path::Path::new(&path);
                    app.upload_avatar_file(path);
                }
                if ui
                    .add(
                        Button::new(RichText::new("Supprimer").size(12.0))
                            .corner_radius(CornerRadius::same(5)),
                    )
                    .clicked()
                {
                    app.remove_avatar();
                }
            });
            ui.add_space(14.0);

            ui.label(
                RichText::new("Bannière")
                    .font(fonts::bold(12.5))
                    .color(theme::TEXT),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add(
                    TextEdit::singleline(&mut app.banner_path_input)
                        .hint_text("Chemin local de l'image…")
                        .desired_width(220.0),
                );
                if ui
                    .add(
                        Button::new(RichText::new("Appliquer").size(12.0))
                            .fill(theme::ACCENT)
                            .corner_radius(CornerRadius::same(5)),
                    )
                    .clicked()
                {
                    let path = app.banner_path_input.clone();
                    let path = std::path::Path::new(&path);
                    app.upload_banner_file(path);
                }
                if ui
                    .add(
                        Button::new(RichText::new("Supprimer").size(12.0))
                            .corner_radius(CornerRadius::same(5)),
                    )
                    .clicked()
                {
                    app.remove_banner();
                }
            });
            ui.add_space(10.0);
            ui.label(
                RichText::new(
                    "Les champs ci-dessus sont envoyés au serveur via PATCH /v1/users/@me.",
                )
                .size(11.0)
                .color(theme::TEXT_FAINT),
            );
        });

        ui.add_space(16.0);

        ui.allocate_ui_with_layout(
            vec2(304.0, 0.0),
            Layout::top_down(Align::Min),
            |ui| {
            ui.set_width(304.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("Aperçu du profil")
                        .size(12.0)
                        .color(theme::TEXT_MUTED),
                );
                ui.add_space(6.0);
            });
            Frame::default()
                .fill(theme::BG_SIDEBAR)
                .corner_radius(CornerRadius::same(8))
                .stroke(Stroke::new(1.0, theme::ACCENT.gamma_multiply(0.7)))
                .inner_margin(Margin::same(0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let (banner, _) = ui.allocate_exact_size(
                        vec2(ui.available_width(), 74.0),
                        Sense::hover(),
                    );
                    ui.painter().rect_filled(
                        banner,
                        CornerRadius {
                            nw: 8,
                            ne: 8,
                            sw: 0,
                            se: 0,
                        },
                        theme::ACCENT.gamma_multiply(0.55),
                    );
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.add_space(14.0);
                        let (avatar, _) =
                            ui.allocate_exact_size(Vec2::splat(64.0), Sense::hover());
                        paint_avatar(
                            ui,
                            avatar,
                            &me,
                            &media,
                            app.state.presence.get(&me.id).map(String::as_str),
                            theme::BG_SIDEBAR,
                        );
                        ui.add_space(6.0);
                        ui.vertical(|ui| {
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new(if app.profile_display.is_empty() {
                                    me.display_name()
                                } else {
                                    app.profile_display.as_str()
                                })
                                .font(fonts::bold(18.0))
                                .color(theme::TEXT),
                            );
                            ui.label(
                                RichText::new(format!("@{}", me.username))
                                    .size(11.5)
                                    .color(theme::TEXT_MUTED),
                            );
                        });
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.add_space(14.0);
                        ui.vertical(|ui| {
                            if !app.profile_pronouns.is_empty() {
                                ui.label(
                                    RichText::new(&app.profile_pronouns)
                                        .size(12.0)
                                        .color(theme::TEXT_MUTED),
                                );
                            }
                            if let Some(bio) = me.bio.clone().filter(|bio| !bio.is_empty()) {
                                ui.label(
                                    RichText::new(markdown::plain(&bio))
                                        .size(11.5)
                                        .color(theme::TEXT_MUTED),
                                );
                            }
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new("Heure locale")
                                    .font(fonts::bold(12.0))
                                    .color(theme::TEXT),
                            );
                            ui.label(
                                RichText::new(chrono::Local::now().format("%H:%M").to_string())
                                    .size(11.5)
                                    .color(theme::TEXT_MUTED),
                            );
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new("Membre de Fluxer depuis le")
                                    .font(fonts::bold(12.0))
                                    .color(theme::TEXT),
                            );
                            ui.label(
                                RichText::new(account_created(&me.id))
                                    .size(11.5)
                                    .color(theme::TEXT_MUTED),
                            );
                        });
                    });
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.add_space(14.0);
                        let width = ui.available_width() - 14.0;
                        if ui
                            .add(
                                Button::new(
                                    RichText::new("Envoyer un message")
                                        .size(12.0)
                                        .color(theme::TEXT_MUTED),
                                )
                                .fill(theme::ACCENT.gamma_multiply(0.35))
                                .corner_radius(CornerRadius::same(5))
                                .min_size(vec2(width, 30.0)),
                            )
                            .clicked()
                        {}
                    });
                    ui.add_space(14.0);
                });
            ui.add_space(8.0);
            ui.label(
                RichText::new(format!("Identifiant : {}", me.id))
                    .size(10.5)
                    .color(theme::TEXT_FAINT),
            );
        });
    });
}

fn profile_field(ui: &mut Ui, label: &str, value: &mut String) {
    ui.label(
        RichText::new(label)
            .font(fonts::bold(12.5))
            .color(theme::TEXT),
    );
    ui.add_space(4.0);
    Frame::default()
        .fill(theme::BG_COMPOSER)
        .corner_radius(CornerRadius::same(5))
        .stroke(Stroke::new(1.0, theme::BORDER))
        .inner_margin(Margin::symmetric(9, 7))
        .show(ui, |ui| {
            ui.add(
                TextEdit::singleline(value)
                    .frame(Frame::NONE)
                    .desired_width(f32::INFINITY)
                    .text_color(theme::TEXT),
            );
        });
    ui.add_space(12.0);
}

/// Date de création d'un compte à partir du snowflake (epoch Discord/Fluxer).
fn account_created(id: &str) -> String {
    let Ok(snowflake) = id.parse::<u64>() else {
        return "date inconnue".into();
    };
    let millis = (snowflake >> 22) + 1_420_070_400_000;
    chrono::DateTime::from_timestamp_millis(millis as i64)
        .map(|date| {
            date.with_timezone(&chrono::Local)
                .format("%d %B %Y")
                .to_string()
        })
        .unwrap_or_else(|| "date inconnue".into())
}

fn settings_chat(app: &mut FluxerApp, ui: &mut Ui) {
    settings_section(ui, "Affichage des messages");
    let mut compact = app.state.compact;
    if ui
        .checkbox(&mut compact, "Mode compact (une ligne par message)")
        .changed()
    {
        app.state.compact = compact;
        app.save_prefs();
    }
    let mut show_members = app.state.show_members;
    if ui
        .checkbox(&mut show_members, "Afficher la liste des membres")
        .changed()
    {
        app.state.show_members = show_members;
        app.save_prefs();
    }
    let mut favorites = app.state.enable_favorites;
    if ui
        .checkbox(
            &mut favorites,
            "Afficher les favoris dans le menu des messages",
        )
        .changed()
    {
        app.state.enable_favorites = favorites;
        app.save_prefs();
    }
    let mut reduce_motion = app.state.reduce_motion;
    if ui
        .checkbox(&mut reduce_motion, "Réduire les animations")
        .changed()
    {
        app.state.reduce_motion = reduce_motion;
        app.save_prefs();
    }
    ui.add_space(8.0);
    ui.label(
        RichText::new("Entrée envoie le message, Maj+Entrée insère un saut de ligne.")
            .size(11.5)
            .color(theme::TEXT_FAINT),
    );

    ui.add_space(14.0);
    settings_separator(ui);
    settings_section(ui, "Historique");
    ui.label(
        RichText::new(format!(
            "{} message(s) chargés dans le salon courant.",
            app.state.messages.len()
        ))
        .size(12.5)
        .color(theme::TEXT_MUTED),
    );
    ui.label(
        RichText::new(
            "Le défilement vers le haut charge automatiquement tout l'historique, sans bouton.",
        )
        .size(11.5)
        .color(theme::TEXT_FAINT),
    );
}

fn settings_notifications(app: &mut FluxerApp, ui: &mut Ui) {
    settings_section(ui, "Notifications de bureau");
    let mut enabled = app.state.notifications;
    if ui
        .checkbox(&mut enabled, "Activer les notifications")
        .changed()
    {
        app.state.notifications = enabled;
        app.save_prefs();
    }
    let mut sounds = app.state.notification_sounds;
    if ui
        .checkbox(&mut sounds, "Jouer un son à la réception")
        .changed()
    {
        app.state.notification_sounds = sounds;
        app.save_prefs();
    }
    ui.add_space(8.0);
    ui.label(
        RichText::new(
            "Les notifications système natives (D-Bus / portail XDG) seront branchées sur MESSAGE_CREATE.",
        )
        .size(11.5)
        .color(theme::TEXT_FAINT),
    );
    ui.add_space(14.0);
    settings_separator(ui);
    settings_section(ui, "Compteurs");
    ui.label(
        RichText::new(format!(
            "{} salon(s) avec des messages non lus.",
            app.unread.len()
        ))
        .size(12.5)
        .color(theme::TEXT_MUTED),
    );
    if !app.unread.is_empty() && ui.button("Marquer tout comme lu").clicked() {
        app.unread.clear();
    }
}

fn settings_voice(app: &mut FluxerApp, ui: &mut Ui) {
    settings_section(ui, "Périphériques");
    ui.label(
        RichText::new("Entrée : périphérique par défaut du système")
            .size(12.5)
            .color(theme::TEXT_MUTED),
    );
    ui.label(
        RichText::new("Sortie : périphérique par défaut du système")
            .size(12.5)
            .color(theme::TEXT_MUTED),
    );
    ui.add_space(10.0);
    settings_section(ui, "État par défaut");
    let mut muted = app.state.voice_muted;
    if ui.checkbox(&mut muted, "Micro coupé en arrivant").changed() {
        app.state.voice_muted = muted;
        app.command(Command::SetVoiceState {
            muted,
            deafened: app.state.voice_deafened,
        });
        app.save_prefs();
    }
    let mut deafened = app.state.voice_deafened;
    if ui
        .checkbox(&mut deafened, "Son coupé en arrivant")
        .changed()
    {
        app.state.voice_deafened = deafened;
        app.command(Command::SetVoiceState {
            muted: app.state.voice_muted,
            deafened,
        });
        app.save_prefs();
    }
    ui.add_space(14.0);
    settings_separator(ui);
    settings_section(ui, "Session vocale");
    match app.state.voice_channel.clone() {
        Some(id) => {
            let label = app
                .state
                .channels
                .iter()
                .chain(app.state.private_channels.iter())
                .find(|channel| channel.id == id)
                .map(|channel| channel.label())
                .unwrap_or(id);
            ui.label(
                RichText::new(format!("Connecté à « {label} »"))
                    .size(13.0)
                    .color(theme::ONLINE),
            );
            ui.label(
                RichText::new(format!(
                    "{} participant(s) signalé(s) par la passerelle.",
                    app.state.voice_members.len().max(1)
                ))
                .size(12.0)
                .color(theme::TEXT_MUTED),
            );
            ui.add_space(6.0);
            if ui.button("Quitter le salon vocal").clicked()
                && let Some(guild_id) = app.state.voice_guild.clone()
            {
                app.command(Command::LeaveVoice { guild_id });
            }
        }
        None => {
            ui.label(
                RichText::new("Aucun salon vocal rejoint.")
                    .size(13.0)
                    .color(theme::TEXT_MUTED),
            );
            ui.label(
                RichText::new(
                    "Cliquez sur un salon vocal dans la liste des salons pour le rejoindre.",
                )
                .size(11.5)
                .color(theme::TEXT_FAINT),
            );
        }
    }
    ui.add_space(10.0);
    ui.label(
        RichText::new(
            "La signalisation (op 4, VOICE_STATE_UPDATE, VOICE_SERVER_UPDATE) est active. Le transport audio natif WebRTC/Opus reste à implémenter.",
        )
        .size(11.0)
        .color(theme::TEXT_FAINT),
    );
}

fn settings_shortcuts(ui: &mut Ui) {
    settings_section(ui, "Clavier");
    for (keys, action) in [
        ("Entrée", "Envoyer le message"),
        ("Maj + Entrée", "Nouvelle ligne"),
        ("Échap", "Fermer les réglages"),
        ("Ctrl + F", "Rechercher dans le salon (à venir)"),
        ("Ctrl + Maj + M", "Couper / réactiver le micro (à venir)"),
        ("Ctrl + Maj + D", "Couper / réactiver le son (à venir)"),
    ] {
        ui.horizontal(|ui| {
            Frame::default()
                .fill(theme::BG_TERTIARY)
                .corner_radius(CornerRadius::same(4))
                .inner_margin(Margin::symmetric(6, 2))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(keys)
                            .font(FontId::monospace(11.0))
                            .color(theme::TEXT),
                    );
                });
            ui.add_space(8.0);
            ui.label(RichText::new(action).size(12.5).color(theme::TEXT_MUTED));
        });
        ui.add_space(6.0);
    }
}

fn settings_language(app: &mut FluxerApp, ui: &mut Ui) {
    settings_section(ui, "Langue de l'interface");
    let current = app.state.language.clone();
    egui::ComboBox::from_id_salt("settings_language")
        .selected_text(language_label(&current))
        .width(240.0)
        .show_ui(ui, |ui| {
            for code in ["fr", "en", "de", "es"] {
                if ui
                    .selectable_label(current == code, language_label(code))
                    .clicked()
                    && app.state.language != code
                {
                    app.state.language = code.to_string();
                    app.save_prefs();
                }
            }
        });
    ui.add_space(8.0);
    ui.label(
        RichText::new(
            "L'interface native est actuellement rédigée en français ; le choix est persisté et servira de base à la traduction.",
        )
        .size(11.5)
        .color(theme::TEXT_FAINT),
    );
}

fn language_label(code: &str) -> &'static str {
    match code {
        "en" => "English",
        "de" => "Deutsch",
        "es" => "Español",
        _ => "Français",
    }
}

fn settings_advanced(app: &mut FluxerApp, ui: &mut Ui) {
    settings_section(ui, "Diagnostic");
    for (label, value) in [
        ("Instance", app.login.instance.clone()),
        ("CDN", app.state.media_base.clone()),
        ("Passerelle", gateway_label(app.state.gateway).to_string()),
        ("Serveurs", app.state.guilds.len().to_string()),
        ("Salons", app.state.channels.len().to_string()),
        ("Membres", app.state.members.len().to_string()),
        (
            "Conversations privées",
            app.state.private_channels.len().to_string(),
        ),
        ("Favoris", app.favorites.len().to_string()),
    ] {
        ui.horizontal(|ui| {
            ui.add_sized(
                vec2(170.0, 20.0),
                egui::Label::new(RichText::new(label).size(12.5).color(theme::TEXT_MUTED)),
            );
            ui.label(RichText::new(value).size(12.5).color(theme::TEXT));
        });
        ui.add_space(3.0);
    }
    ui.add_space(12.0);
    if ui.button("Rafraîchir les salons et les membres").clicked() {
        app.command(Command::Refresh);
    }
    ui.add_space(10.0);
    ui.label(
        RichText::new(format!(
            "Fichier de session : {}",
            fluxer_core::session::config_path().display()
        ))
        .size(11.0)
        .color(theme::TEXT_FAINT),
    );
    ui.label(
        RichText::new(
            "Le jeton n'est jamais affiché. Le fichier est créé avec les permissions 0600.",
        )
        .size(11.0)
        .color(theme::TEXT_FAINT),
    );
}

fn settings_connection(app: &mut FluxerApp, ui: &mut Ui) {
    settings_section(ui, "Temps réel");
    ui.horizontal(|ui| {
        let color = match app.state.gateway {
            GatewayStatus::Online => theme::ONLINE,
            GatewayStatus::Connecting | GatewayStatus::Reconnecting => theme::IDLE,
            GatewayStatus::Offline => theme::OFFLINE,
        };
        let (dot, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
        ui.painter().circle_filled(dot.center(), 5.0, color);
        ui.label(
            RichText::new(gateway_label(app.state.gateway))
                .size(13.0)
                .color(theme::TEXT_MUTED),
        );
    });
    ui.add_space(8.0);
    ui.label(
        RichText::new(format!("CDN : {}", app.state.media_base))
            .size(11.5)
            .color(theme::TEXT_FAINT),
    );
    ui.add_space(14.0);
    if ui.button("Rafraîchir les salons").clicked() {
        app.command(Command::Refresh);
    }
    ui.add_space(8.0);
    ui.label(
        RichText::new(format!("Salons chargés : {}", app.state.channels.len()))
            .size(12.0)
            .color(theme::TEXT_FAINT),
    );
    ui.label(
        RichText::new(format!("Membres chargés : {}", app.state.members.len()))
            .size(12.0)
            .color(theme::TEXT_FAINT),
    );
}

fn settings_about(ui: &mut Ui) {
    settings_section(ui, "Fluxer-rs");
    ui.label(
        RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
            .size(13.0)
            .color(theme::TEXT_MUTED),
    );
    ui.add_space(10.0);
    ui.label(
        RichText::new(
            "Client de bureau natif pour Fluxer, écrit intégralement en Rust. Aucun JavaScript, aucun Electron, aucune WebView : le protocole HTTP, la passerelle temps réel et l'interface sont implémentés dans ce binaire.",
        )
        .size(12.5)
        .color(theme::TEXT_MUTED),
    );
    ui.add_space(12.0);
    for line in [
        "Interface : eframe / egui 0.37",
        "Réseau : reqwest + tokio-tungstenite",
        "Rendu : wgpu / Vulkan uniquement",
        "Plateforme : Wayland uniquement (aucun X11, aucun OpenGL)",
        "Polices : Noto Sans / Noto Sans Mono",
    ] {
        ui.label(
            RichText::new(format!("• {line}"))
                .size(12.0)
                .color(theme::TEXT_FAINT),
        );
    }

    ui.add_space(16.0);
    settings_section(ui, "Licence");
    ui.label(
        RichText::new(
            "Fluxer-rs est un logiciel libre distribué sous GNU Affero General Public License, version 3 ou ultérieure (AGPL-3.0-or-later).",
        )
        .size(12.0)
        .color(theme::TEXT_MUTED),
    );
    ui.add_space(6.0);
    ui.label(
        RichText::new(
            "Copyright (C) 2026 Fluxer-rs contributors.\n\nCe programme est distribué SANS AUCUNE GARANTIE, y compris sans la garantie implicite de COMMERCIALISATION ou d'ADÉQUATION À UN USAGE PARTICULIER. Consultez la licence pour plus de détails.\n\nLe texte intégral se trouve dans le fichier LICENSE.txt livré avec le programme et sur https://www.gnu.org/licenses/agpl-3.0.txt.",
        )
        .size(11.5)
        .color(theme::TEXT_FAINT),
    );
}

fn settings_section(ui: &mut Ui, title: &str) {
    ui.label(
        RichText::new(title)
            .font(fonts::bold(11.0))
            .color(theme::TEXT_FAINT),
    );
    ui.add_space(7.0);
}

fn settings_separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0, theme::DIVIDER);
    ui.add_space(14.0);
}

// ---------------------------------------------------------------------------
// Shared widgets
// ---------------------------------------------------------------------------

fn paint_avatar(
    ui: &Ui,
    rect: Rect,
    user: &User,
    media: &str,
    status: Option<&str>,
    background: Color32,
) {
    let media = media.to_string();
    let url = user.avatar_url(&media);
    match url {
        Some(url) => {
            let radius = (rect.width() * 0.5) as u8;
            let mut image = Image::new(url)
                .fit_to_exact_size(rect.size())
                .corner_radius(CornerRadius::same(radius));
            let offline = status == Some("offline");
            if offline {
                image = image.tint(Color32::from_gray(150));
            }
            image.paint_at(ui, rect);
        }
        None => {
            let color = theme::packed_color(user.avatar_color, &user.id);
            let color = if status == Some("offline") {
                color.gamma_multiply(0.55)
            } else {
                color
            };
            ui.painter()
                .circle_filled(rect.center(), rect.width() * 0.5, color);
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                user.initials(),
                fonts::bold(rect.width() * 0.38),
                Color32::WHITE,
            );
        }
    }

    if let Some(status) = status {
        let r = (rect.width() * 0.16).clamp(4.0, 7.0);
        let center = pos2(rect.right() - r, rect.bottom() - r);
        ui.painter().circle_filled(center, r + 2.0, background);
        ui.painter()
            .circle_filled(center, r, theme::status_color(Some(status)));
    }
}

fn icon_button(ui: &mut Ui, icon: Icon, size: f32, tint: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let hovered = response.hovered();
    if hovered {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same((size * 0.25) as u8),
            theme::CHANNEL_HOVER,
        );
    }
    icons::paint(
        ui.painter(),
        rect.shrink(size * 0.26),
        icon,
        if hovered { theme::TEXT } else { tint },
        1.6,
    );
    response.on_hover_cursor(CursorIcon::PointingHand)
}

fn icon_button_at(ui: &mut Ui, rect: Rect, icon: Icon, tint: Color32) -> bool {
    let response = ui.interact(rect, ui.scope_id().with(icon as u8), Sense::click());
    let hovered = response.hovered();
    if hovered {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), theme::CHANNEL_HOVER);
    }
    icons::paint(
        ui.painter(),
        rect.shrink(8.0),
        icon,
        if hovered { theme::TEXT } else { tint },
        1.7,
    );
    response.clicked()
}

fn current_guild(app: &FluxerApp) -> Option<&Guild> {
    let id = app.state.current_guild.as_deref()?;
    app.state.guilds.iter().find(|g| g.id == id)
}

fn current_channel(app: &FluxerApp) -> Option<&Channel> {
    let id = app.state.current_channel.as_deref()?;
    app.state
        .channels
        .iter()
        .chain(app.state.private_channels.iter())
        .find(|c| c.id == id)
}

/// Sélecteur d'émojis minimaliste, comme le bouton « Émojis » du composeur.
fn emoji_picker_window(app: &mut FluxerApp, ctx: &egui::Context) {
    if !app.show_emoji_picker {
        return;
    }
    if ctx.input(|i| i.key_pressed(Key::Escape)) {
        app.show_emoji_picker = false;
        return;
    }
    egui::Window::new("Émojis")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .fixed_size(vec2(280.0, 220.0))
        .anchor(Align2::RIGHT_BOTTOM, vec2(-16.0, -70.0))
        .frame(
            Frame::default()
                .fill(theme::BG_SIDEBAR)
                .corner_radius(CornerRadius::same(10))
                .stroke(Stroke::new(1.0, theme::BORDER))
                .inner_margin(Margin::same(8)),
        )
        .show(ctx, |ui| {
            ui.set_min_width(280.0);
            let emojis = [
                "😀", "😃", "😄", "😁", "😆", "😅", "😂", "🤣", "😊", "😇", "🙂", "🙃", "😉", "😌",
                "😍", "🥰", "😘", "😗", "😙", "😚", "😋", "😛", "😜", "🤪", "😝", "🤑", "🤗", "🤭",
                "🤫", "🤔", "🤐", "🤨", "😐", "😑", "😶", "😏", "😒", "🙄", "😬", "🤥", "😌", "😔",
                "😪", "🤤", "😴", "😷", "🤒", "🤕", "🤢", "🤮", "🤧", "🥵", "🥶", "😵", "🤯", "🤠",
                "🥳", "😎", "🤓", "🧐", "😕", "😟", "🙁", "☹️", "😮", "😯", "😲", "😳", "🥺", "😦",
                "😧", "😨", "😰", "😥", "😢", "😭", "😱", "😖", "😣", "😞", "😓", "😩", "😫", "🥱",
                "😤", "😡", "😠", "🤬", "😈", "👿", "💀", "☠️", "💩", "🤡", "👹", "👺", "👻", "👽",
                "👾", "🤖", "😺", "😸", "😹", "😻", "😼", "😽", "🙀", "😿", "😾", "👍", "👎", "👌",
                "✌️", "🤞", "🤟", "🤘", "🤙", "👈", "👉", "👆", "👇", "☝️", "✋", "🤚", "🖐", "🖖",
                "👋", "🤝", "🙏", "💪", "❤️", "🧡", "💛", "💚", "💙", "💜", "🖤", "🤍", "🤎", "💔",
                "❣️", "💕", "💞", "💓", "💗", "💖", "💘", "💝", "💟",
            ];
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
                        for emoji in emojis {
                            if ui
                                .add(Button::new(RichText::new(emoji).size(18.0)).frame(false))
                                .clicked()
                            {
                                if app.editing.is_some() {
                                    app.edit_draft.push_str(emoji);
                                } else {
                                    app.draft.push_str(emoji);
                                }
                                app.show_emoji_picker = false;
                            }
                        }
                    });
                });
        });
}

/// Centre de notifications : liste des mentions reçues.
fn inbox_window(app: &mut FluxerApp, ctx: &egui::Context) {
    if !app.show_inbox {
        return;
    }
    if ctx.input(|i| i.key_pressed(Key::Escape)) {
        app.show_inbox = false;
        return;
    }
    egui::Window::new("Notifications")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .fixed_size(vec2(360.0, 420.0))
        .anchor(Align2::RIGHT_BOTTOM, vec2(-16.0, -60.0))
        .frame(
            Frame::default()
                .fill(theme::BG_SIDEBAR)
                .corner_radius(CornerRadius::same(10))
                .stroke(Stroke::new(1.0, theme::BORDER))
                .inner_margin(Margin::same(10)),
        )
        .show(ctx, |ui| {
            ui.set_min_width(360.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Mentions")
                        .font(fonts::bold(16.0))
                        .color(theme::TEXT),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icon_button(ui, Icon::Close, 20.0, theme::TEXT_MUTED).clicked() {
                        app.show_inbox = false;
                    }
                });
            });
            ui.add_space(8.0);
            settings_separator(ui);
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if app.mentions.is_empty() {
                        ui.add_space(20.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new("Aucune mention pour le moment.")
                                    .size(12.5)
                                    .color(theme::TEXT_FAINT),
                            );
                        });
                        return;
                    }
                    for mention in app.mentions.clone().iter().rev() {
                        let frame = Frame::default()
                            .fill(if mention.read {
                                theme::BG_SIDEBAR
                            } else {
                                theme::BG_COMPOSER
                            })
                            .corner_radius(CornerRadius::same(6))
                            .inner_margin(Margin::symmetric(10, 6))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(&mention.author)
                                        .font(fonts::bold(12.5))
                                        .color(theme::TEXT),
                                );
                                ui.label(
                                    RichText::new(&mention.excerpt)
                                        .size(11.5)
                                        .color(theme::TEXT_MUTED),
                                );
                            });
                        if frame.response.clicked() {
                            app.open_mention(mention);
                            app.show_inbox = false;
                        }
                        ui.add_space(4.0);
                    }
                });
        });
}
