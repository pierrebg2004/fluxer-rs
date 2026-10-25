// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! Application state, update pump and the `eframe` entry point glue.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use eframe::egui;
use fluxer_core::session::SavedMessage;
use fluxer_core::{
    Channel, DiscoveryGuild, Emoji, Guild, GuildDetail, InvitePreview, Member, Message, User,
};
use tokio::sync::mpsc::UnboundedSender;

use crate::engine::{self, Command, GatewayStatus, Update};

/// Top-level destination selected on the guild rail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    /// Servers, channels and the classic timeline.
    #[default]
    Guilds,
    /// Private messages home (conversations list).
    Dms,
    /// Locally bookmarked messages.
    Favorites,
    /// Public community directory.
    Discovery,
}

/// Message auquel l'utilisateur répond (aperçu local avant envoi).
#[derive(Debug, Clone)]
pub struct ReplyRef {
    pub message_id: String,
    pub author: String,
    pub excerpt: String,
}

/// Mention reçue, pour le centre de notifications.
#[derive(Debug, Clone)]
pub struct Mention {
    pub message_id: String,
    pub channel_id: String,
    pub author: String,
    pub excerpt: String,
    pub read: bool,
}

pub struct FluxerApp {
    commands: UnboundedSender<Command>,
    updates: Receiver<Update>,
    pub state: State,
    pub login: LoginForm,
    pub draft: String,
    /// Message en cours de modification : identifiant + salon d'origine.
    pub editing: Option<(String, String)>,
    pub edit_draft: String,
    /// Message dont la suppression attend confirmation.
    pub confirm_delete: Option<(String, String)>,
    /// Message auquel la prochaine réponse est rattachée.
    pub replying: Option<ReplyRef>,
    /// Profil affiché dans la carte d'un membre.
    pub profile_popup: Option<User>,
    /// Garde anti-rafale pour le chargement de l'historique au défilement.
    pub load_older_armed: bool,
    /// Indicateur de frappe par salon : (utilisateur, instant).
    pub typing: HashMap<String, (String, Instant)>,
    /// Dernière notification de frappe envoyée au serveur.
    last_typing: Option<Instant>,
    pub search: String,
    pub show_settings: bool,
    pub show_emoji_picker: bool,
    pub settings_tab: usize,
    pub settings_search: String,
    pub unread: HashMap<String, usize>,
    pub mentions: Vec<Mention>,
    pub show_inbox: bool,
    pub collapsed: HashSet<String>,
    pub scroll_to_bottom: bool,
    pub view: View,
    pub favorites: Vec<SavedMessage>,
    pub favorites_query: String,
    pub favorites_selected: Option<String>,
    /// Champs éditables de l'aperçu de profil (non envoyés au serveur pour
    /// l'instant ; le PATCH suivra).
    pub profile_display: String,
    pub profile_pronouns: String,
    pub profile_bio: String,
    pub avatar_path_input: String,
    pub banner_path_input: String,
    pub show_avatar_dialog: bool,
    pub show_banner_dialog: bool,
    pub show_join_guild: bool,
    pub join_invite: String,
    pub join_busy: bool,
    /// Aperçus d'invitation chargés, indexés par code (`None` = code invalide).
    pub invites: HashMap<String, Option<InvitePreview>>,
    /// Codes dont le chargement est déjà en vol.
    pub invite_pending: HashSet<String>,
    screenshot_start: std::time::Instant,
    screenshot_taken: bool,
}

pub struct State {
    pub me: Option<User>,
    pub guilds: Vec<Guild>,
    pub detail: Option<GuildDetail>,
    pub members: Vec<Member>,
    pub presence: HashMap<String, String>,
    pub channels: Vec<Channel>,
    pub messages: Vec<Message>,
    pub current_guild: Option<String>,
    pub current_channel: Option<String>,
    pub media_base: String,
    pub status: String,
    pub error: Option<String>,
    pub gateway: GatewayStatus,
    pub loading_messages: bool,
    pub has_more_before: bool,
    pub loading_channels: bool,
    pub loading_members: bool,
    pub show_members: bool,
    pub compact: bool,
    pub private_channels: Vec<Channel>,
    pub discovery: Vec<DiscoveryGuild>,
    pub discovery_loading: bool,
    pub discovery_total: i64,
    /// Annuaire demandé automatiquement en mode capture (`FLUXER_VIEW`).
    pub discovery_auto_requested: bool,
    // Persistent preferences, mirrored from `Config`.
    pub enable_favorites: bool,
    pub notifications: bool,
    pub notification_sounds: bool,
    pub language: String,
    pub ui_scale: f32,
    pub reduce_motion: bool,
    // Voice.
    pub voice_channel: Option<String>,
    pub voice_guild: Option<String>,
    pub voice_members: Vec<String>,
    pub voice_connected: bool,
    pub voice_muted: bool,
    pub voice_deafened: bool,
}

pub struct LoginForm {
    pub instance: String,
    pub token: String,
    pub advanced: bool,
    pub show_token: bool,
    pub busy: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            me: None,
            guilds: Vec::new(),
            detail: None,
            members: Vec::new(),
            presence: HashMap::new(),
            channels: Vec::new(),
            messages: Vec::new(),
            current_guild: None,
            current_channel: None,
            media_base: fluxer_core::media::DEFAULT_MEDIA.to_string(),
            status: "prêt".into(),
            error: None,
            gateway: GatewayStatus::Offline,
            loading_messages: false,
            has_more_before: false,
            loading_channels: false,
            loading_members: false,
            show_members: true,
            compact: false,
            private_channels: Vec::new(),
            discovery: Vec::new(),
            discovery_loading: false,
            discovery_total: 0,
            discovery_auto_requested: false,
            enable_favorites: true,
            notifications: true,
            notification_sounds: true,
            language: "fr".into(),
            ui_scale: 1.0,
            reduce_motion: false,
            voice_channel: None,
            voice_guild: None,
            voice_members: Vec::new(),
            voice_connected: false,
            voice_muted: false,
            voice_deafened: false,
        }
    }
}

#[allow(dead_code)]
impl FluxerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::theme::apply(&cc.egui_ctx);

        let config = fluxer_core::session::load();
        let handle = engine::spawn(cc.egui_ctx.clone());
        // Debug helper: `FLUXER_SETTINGS=profile` opens the settings window on
        // a given tab so it can be captured headlessly.
        let debug_settings = std::env::var("FLUXER_SETTINGS").ok();
        let settings_tab = debug_settings
            .as_deref()
            .map(debug_settings_tab)
            .unwrap_or(0);

        Self {
            commands: handle.commands,
            updates: handle.updates,
            state: State {
                compact: config.compact,
                show_members: config.show_members,
                enable_favorites: config.enable_favorites,
                notifications: config.notifications,
                notification_sounds: config.notification_sounds,
                language: config.language.clone(),
                ui_scale: config.ui_scale,
                reduce_motion: config.reduce_motion,
                voice_muted: config.voice_muted,
                voice_deafened: config.voice_deafened,
                ..State::default()
            },
            login: LoginForm {
                instance: config.instance,
                token: String::new(),
                advanced: false,
                show_token: false,
                busy: false,
            },
            draft: String::new(),
            editing: None,
            edit_draft: String::new(),
            confirm_delete: None,
            replying: None,
            profile_popup: None,
            load_older_armed: true,
            typing: HashMap::new(),
            last_typing: None,
            search: String::new(),
            show_settings: debug_settings.is_some(),
            show_emoji_picker: false,
            settings_tab,
            settings_search: String::new(),
            unread: HashMap::new(),
            mentions: Vec::new(),
            show_inbox: false,
            collapsed: HashSet::new(),
            scroll_to_bottom: true,
            view: match std::env::var("FLUXER_VIEW").as_deref() {
                Ok("dms") => View::Dms,
                Ok("favorites") => View::Favorites,
                Ok("discovery") => View::Discovery,
                _ => View::Guilds,
            },
            favorites: config.favorites.clone(),
            favorites_query: String::new(),
            favorites_selected: None,
            profile_display: String::new(),
            profile_pronouns: String::new(),
            profile_bio: String::new(),
            avatar_path_input: String::new(),
            banner_path_input: String::new(),
            show_avatar_dialog: false,
            show_banner_dialog: false,
            show_join_guild: false,
            join_invite: String::new(),
            join_busy: false,
            invites: HashMap::new(),
            invite_pending: HashSet::new(),
            screenshot_start: std::time::Instant::now(),
            screenshot_taken: false,
        }
    }

    pub fn command(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    /// Accepte une invitation saisie dans la modale « Rejoindre un serveur ».
    pub fn submit_invite(&mut self) {
        let Some(code) = invite_code(&self.join_invite) else {
            self.state.error = Some("Invitation invalide.".into());
            return;
        };
        self.join_busy = true;
        self.state.status = "Acceptation de l'invitation…".into();
        self.command(Command::JoinInvite { code });
    }

    /// Demande l'aperçu d'une invitation citée dans un message (une seule
    /// requête par code, résultat mis en cache).
    pub fn ensure_invite(&mut self, code: &str) {
        if self.invites.contains_key(code) || !self.invite_pending.insert(code.to_string()) {
            return;
        }
        self.command(Command::LoadInvite {
            code: code.to_string(),
        });
    }

    /// Crée un post dans un salon forum/média.
    pub fn start_forum_post(&mut self, channel: &Channel, content: &str) {
        let name = if content.trim().is_empty() {
            format!("Post de {}", channel.label())
        } else {
            content.chars().take(50).collect()
        };
        self.command(Command::StartForumPost {
            channel_id: channel.id.clone(),
            name,
            content: content.to_string(),
        });
    }

    /// Crée un fil à partir d'un message.
    pub fn start_thread(&mut self, message: &Message) {
        let name = if message.content.trim().is_empty() {
            format!("Fil de {}", message.author.display_name())
        } else {
            message.content.chars().take(50).collect()
        };
        self.command(Command::StartThread {
            channel_id: message.channel_id.clone(),
            message_id: message.id.clone(),
            name,
        });
    }
    pub fn indicate_typing(&mut self) {
        if self
            .last_typing
            .is_none_or(|instant| instant.elapsed() > Duration::from_secs(5))
            && let Some(channel_id) = self.state.current_channel.clone()
        {
            self.command(Command::IndicateTyping { channel_id });
            self.last_typing = Some(Instant::now());
        }
    }

    pub fn send_draft(&mut self) {
        let content = self.draft.trim_end().to_string();
        if content.trim().is_empty() {
            self.draft.clear();
            return;
        }
        let reply_to = self.replying.as_ref().map(|r| r.message_id.clone());
        self.command(Command::SendMessage { content, reply_to });
        self.draft.clear();
        self.replying = None;
    }

    /// Prépare une réponse citée à un message.
    pub fn start_reply(&mut self, message: &Message) {
        let excerpt = message.content.replace('\n', " ");
        self.replying = Some(ReplyRef {
            message_id: message.id.clone(),
            author: message.author.display_name().to_string(),
            excerpt: excerpt.chars().take(80).collect(),
        });
    }

    pub fn cancel_reply(&mut self) {
        self.replying = None;
    }

    /// Entre en mode édition pour un message de l'utilisateur.
    pub fn start_edit(&mut self, channel_id: &str, message_id: &str, content: &str) {
        self.editing = Some((message_id.to_string(), channel_id.to_string()));
        self.edit_draft = content.to_string();
    }

    pub fn cancel_edit(&mut self) {
        self.editing = None;
        self.edit_draft.clear();
    }

    /// Valide la modification ; un contenu vide annule l'édition.
    pub fn commit_edit(&mut self) {
        let Some((message_id, channel_id)) = self.editing.clone() else {
            return;
        };
        let content = self.edit_draft.trim_end().to_string();
        if content.trim().is_empty() {
            self.cancel_edit();
            return;
        }
        self.command(Command::EditMessage {
            channel_id,
            message_id,
            content,
        });
        self.cancel_edit();
    }

    pub fn request_delete(&mut self, channel_id: &str, message_id: &str) {
        self.confirm_delete = Some((message_id.to_string(), channel_id.to_string()));
    }

    pub fn confirm_delete_message(&mut self) {
        if let Some((message_id, channel_id)) = self.confirm_delete.take() {
            self.command(Command::DeleteMessage {
                channel_id,
                message_id,
            });
        }
    }

    /// Ajoute ou retire la réaction de l'utilisateur courant sur un message.
    pub fn toggle_reaction(&mut self, message: &Message, emoji: &str) {
        let key = Emoji::from(emoji).key();
        let active = message
            .reactions
            .iter()
            .any(|reaction| reaction.emoji.key() == key && reaction.me);
        self.command(Command::ToggleReaction {
            channel_id: message.channel_id.clone(),
            message_id: message.id.clone(),
            emoji: emoji.to_string(),
            active: !active,
        });
    }

    /// Épingle ou désépingle un message dans le salon courant.
    pub fn toggle_pin(&mut self, message: &Message) {
        self.command(Command::SetPinned {
            channel_id: message.channel_id.clone(),
            message_id: message.id.clone(),
            pinned: !message.pinned.unwrap_or(false),
        });
    }

    /// Envoie un fichier (glisser-déposer) avec le texte en cours de rédaction.
    pub fn send_attachment(
        &mut self,
        filename: String,
        content_type: Option<String>,
        bytes: Vec<u8>,
    ) {
        let Some(channel_id) = self.state.current_channel.clone() else {
            return;
        };
        let content = self.draft.trim_end().to_string();
        self.command(Command::UploadAttachment {
            channel_id,
            filename,
            content_type,
            bytes,
            content,
        });
        self.draft.clear();
    }

    /// Enregistre les champs du profil via `PATCH /v1/users/@me`.
    pub fn save_profile(&mut self) {
        let display = self.profile_display.trim().to_string();
        let pronouns = self.profile_pronouns.trim().to_string();
        let bio = self.profile_bio.trim().to_string();
        self.command(Command::UpdateProfile {
            display_name: (!display.is_empty()).then_some(display),
            pronouns: Some(pronouns),
            bio: Some(bio),
            avatar: None,
            banner: None,
        });
        self.state.status = "Enregistrement du profil…".to_string();
    }

    /// Téléverse un nouvel avatar depuis un chemin de fichier local.
    pub fn upload_avatar_file(&mut self, path: &std::path::Path) {
        match file_to_data_uri(path) {
            Ok(data_uri) => {
                self.command(Command::UpdateProfile {
                    display_name: None,
                    pronouns: None,
                    bio: None,
                    avatar: Some(Some(data_uri)),
                    banner: None,
                });
                self.state.status = "Téléversement de l'avatar…".to_string();
                self.show_avatar_dialog = false;
                self.avatar_path_input.clear();
            }
            Err(err) => {
                self.state.status = format!("Erreur d'avatar : {err}");
            }
        }
    }

    /// Supprime l'avatar actuel du compte.
    pub fn remove_avatar(&mut self) {
        self.command(Command::UpdateProfile {
            display_name: None,
            pronouns: None,
            bio: None,
            avatar: Some(None),
            banner: None,
        });
        self.state.status = "Suppression de l'avatar…".to_string();
    }

    /// Téléverse une nouvelle bannière depuis un chemin de fichier local.
    pub fn upload_banner_file(&mut self, path: &std::path::Path) {
        match file_to_data_uri(path) {
            Ok(data_uri) => {
                self.command(Command::UpdateProfile {
                    display_name: None,
                    pronouns: None,
                    bio: None,
                    avatar: None,
                    banner: Some(Some(data_uri)),
                });
                self.state.status = "Téléversement de la bannière…".to_string();
                self.show_banner_dialog = false;
                self.banner_path_input.clear();
            }
            Err(err) => {
                self.state.status = format!("Erreur de bannière : {err}");
            }
        }
    }

    /// Supprime la bannière actuelle du compte.
    pub fn remove_banner(&mut self) {
        self.command(Command::UpdateProfile {
            display_name: None,
            pronouns: None,
            bio: None,
            avatar: None,
            banner: Some(None),
        });
        self.state.status = "Suppression de la bannière…".to_string();
    }

    /// Ouvre le salon d'une mention et la marque comme lue.
    pub fn open_mention(&mut self, mention: &Mention) {
        for entry in &mut self.mentions {
            if entry.message_id == mention.message_id {
                entry.read = true;
            }
        }
        self.command(Command::SelectChannel(mention.channel_id.clone()));
    }

    pub fn logout(&mut self) {
        self.command(Command::Logout);
        let media = self.state.media_base.clone();
        let compact = self.state.compact;
        self.state = State {
            media_base: media,
            compact,
            ..State::default()
        };
        self.login.busy = false;
        self.unread.clear();
        self.search.clear();
        self.cancel_edit();
        self.cancel_reply();
        self.confirm_delete = None;
        self.show_join_guild = false;
        self.join_invite.clear();
        self.join_busy = false;
        self.view = View::Guilds;
    }

    /// Persists the current presentation preference.
    pub fn save_compact(&mut self) {
        let mut config = fluxer_core::session::load();
        config.compact = self.state.compact;
        let _ = fluxer_core::session::save(&config);
    }

    /// Persists the presentation and notification preferences.
    pub fn save_prefs(&mut self) {
        let mut config = fluxer_core::session::load();
        config.compact = self.state.compact;
        config.show_members = self.state.show_members;
        config.enable_favorites = self.state.enable_favorites;
        config.notifications = self.state.notifications;
        config.notification_sounds = self.state.notification_sounds;
        config.language = self.state.language.clone();
        config.ui_scale = self.state.ui_scale;
        config.reduce_motion = self.state.reduce_motion;
        config.voice_muted = self.state.voice_muted;
        config.voice_deafened = self.state.voice_deafened;
        config.favorites = self.favorites.clone();
        let _ = fluxer_core::session::save(&config);
    }

    /// Adds or removes a message from the favorites list.
    pub fn toggle_favorite(&mut self, message: &Message) {
        let channel_label = self
            .state
            .channels
            .iter()
            .chain(self.state.private_channels.iter())
            .find(|channel| channel.id == message.channel_id)
            .map(|channel| {
                if channel.kind().is_direct() {
                    channel.direct_label()
                } else {
                    format!("#{}", channel.label())
                }
            })
            .unwrap_or_else(|| message.channel_id.clone());
        let saved = SavedMessage {
            message_id: message.id.clone(),
            channel_id: message.channel_id.clone(),
            channel_label,
            author: message.author.display_name().to_string(),
            content: message.content.clone(),
            timestamp: message.timestamp.clone(),
        };
        if let Some(index) = self
            .favorites
            .iter()
            .position(|entry| entry.message_id == saved.message_id)
        {
            self.favorites.remove(index);
        } else {
            self.favorites.insert(0, saved);
        }
        self.save_prefs();
    }

    pub fn is_favorite(&self, message_id: &str) -> bool {
        self.favorites
            .iter()
            .any(|entry| entry.message_id == message_id)
    }

    /// Opens the channel a favorite came from.
    pub fn open_favorite(&mut self, saved: &SavedMessage) {
        let is_dm = self
            .state
            .private_channels
            .iter()
            .any(|channel| channel.id == saved.channel_id);
        if is_dm {
            self.view = View::Dms;
            self.command(Command::SelectDm(saved.channel_id.clone()));
        } else {
            self.view = View::Guilds;
            self.command(Command::SelectChannel(saved.channel_id.clone()));
        }
    }

    fn pump(&mut self) {
        while let Ok(update) = self.updates.try_recv() {
            self.apply(update);
        }
    }

    fn apply(&mut self, update: Update) {
        match update {
            Update::Status(status) => self.state.status = status,
            Update::Error(error) => {
                self.state.error = Some(error);
                self.login.busy = false;
                self.join_busy = false;
            }
            Update::LoggedIn(me) => {
                if self.profile_display.is_empty() {
                    self.profile_display = me.display_name().to_string();
                }
                self.profile_pronouns = me.pronouns.clone().unwrap_or_default();
                self.profile_bio = me.bio.clone().unwrap_or_default();
                self.state.me = Some(me);
                self.state.error = None;
                self.login.busy = false;
                self.login.token.clear();
            }
            Update::Guilds(guilds) => {
                self.state.guilds = guilds;
                // Open the first server on startup, like the official client,
                // instead of showing an empty channel list.
                if self.state.current_guild.is_none() {
                    // Debug helper: `FLUXER_GUILD=<id>` picks a specific
                    // server (useful to capture the call view).
                    let requested = std::env::var("FLUXER_GUILD").ok();
                    let selected = requested
                        .as_deref()
                        .and_then(|id| self.state.guilds.iter().find(|guild| guild.id == id))
                        .or_else(|| self.state.guilds.first());
                    if let Some(guild) = selected {
                        let id = guild.id.clone();
                        self.state.current_guild = Some(id.clone());
                        self.command(Command::SelectGuild(id));
                    }
                }
            }
            Update::GuildDetail(detail) => {
                self.state.detail = Some(*detail);
            }
            Update::Members(members) => {
                self.state.members = members;
            }
            Update::Channels(channels) => {
                if let Some(guild_id) = channels.first().and_then(|c| c.guild_id.clone()) {
                    self.state.current_guild = Some(guild_id);
                }
                // Debug helper: `FLUXER_VIEW=voice` joins the first voice
                // channel so the call view can be captured headlessly.
                if std::env::var("FLUXER_VIEW").as_deref() == Ok("voice")
                    && self.state.voice_channel.is_none()
                    && let Some(channel) = channels
                        .iter()
                        .find(|channel| channel.kind() == fluxer_core::ChannelKind::Voice)
                    && let Some(guild_id) = channel.guild_id.clone()
                {
                    self.command(Command::JoinVoice {
                        guild_id,
                        channel_id: channel.id.clone(),
                    });
                }
                self.state.channels = channels;
                // Debug helper: `FLUXER_CHANNEL=<id>` opens a precise channel
                // after the guild's default one (visual regression captures).
                if let Ok(requested) = std::env::var("FLUXER_CHANNEL")
                    && self.state.current_channel.as_deref() != Some(requested.as_str())
                    && self
                        .state
                        .channels
                        .iter()
                        .any(|channel| channel.id == requested)
                {
                    self.command(Command::SelectChannel(requested));
                }
            }
            Update::HasMoreBefore(has_more) => {
                self.state.has_more_before = has_more;
            }
            Update::Messages {
                channel_id,
                messages,
                prepend,
            } => {
                if self.state.current_channel.as_deref() != Some(channel_id.as_str()) {
                    // Une page d'historique d'un ancien salon peut arriver après
                    // un changement de salon : on l'ignore au lieu de revenir en
                    // arrière dans la navigation.
                    if prepend {
                        return;
                    }
                    self.state.current_channel = Some(channel_id.clone());
                    self.search.clear();
                    self.replying = None;
                }
                if prepend {
                    let mut merged = messages;
                    merged.append(&mut self.state.messages);
                    self.state.messages = merged;
                } else {
                    self.state.messages = messages;
                    self.scroll_to_bottom = true;
                }
                // Garantit l'ordre chronologique quel que soit l'ordre d'arrivée
                // des pages (les identifiants sont des snowflakes croissants).
                self.state
                    .messages
                    .sort_by(|a, b| a.id.len().cmp(&b.id.len()).then_with(|| a.id.cmp(&b.id)));
                self.unread.remove(&channel_id);
            }
            Update::MessageCreated(message) => {
                let current = self
                    .state
                    .current_channel
                    .as_deref()
                    .map(|id| id == message.channel_id)
                    .unwrap_or(false);
                if current {
                    if !self.state.messages.iter().any(|m| m.id == message.id) {
                        self.state.messages.push(message.clone());
                        self.scroll_to_bottom = true;
                    }
                } else {
                    *self.unread.entry(message.channel_id.clone()).or_insert(0) += 1;
                }
                // Centre de notifications : on retient les messages qui nous
                // mentionnent (mention directe, pas @everyone).
                if let Some(me) = &self.state.me {
                    let mentioned = message.mentions.iter().any(|user| user.id == me.id);
                    if mentioned {
                        let excerpt: String = message.content.chars().take(120).collect();
                        self.mentions.push(Mention {
                            message_id: message.id.clone(),
                            channel_id: message.channel_id.clone(),
                            author: message.author.display_name().to_string(),
                            excerpt,
                            read: false,
                        });
                    }
                }
            }
            Update::MessageUpdated(message) => {
                if let Some(existing) = self.state.messages.iter_mut().find(|m| m.id == message.id)
                {
                    *existing = message.clone();
                }
                if self
                    .editing
                    .as_ref()
                    .is_some_and(|(id, _)| id == &message.id)
                {
                    self.cancel_edit();
                }
            }
            Update::MessageDeleted {
                channel_id,
                message_id,
            } => {
                if self.state.current_channel.as_deref() == Some(channel_id.as_str()) {
                    self.state.messages.retain(|m| m.id != message_id);
                }
                if self
                    .editing
                    .as_ref()
                    .is_some_and(|(id, _)| id == &message_id)
                {
                    self.cancel_edit();
                }
                if self
                    .confirm_delete
                    .as_ref()
                    .is_some_and(|(id, _)| id == &message_id)
                {
                    self.confirm_delete = None;
                }
            }
            Update::ReactionChanged {
                channel_id,
                message_id,
                emoji,
                delta,
                me,
            } => {
                if self.state.current_channel.as_deref() == Some(channel_id.as_str())
                    && let Some(message) =
                        self.state.messages.iter_mut().find(|m| m.id == message_id)
                {
                    message.apply_reaction(&emoji, delta, me);
                }
            }
            Update::ReactionRemovedAll {
                channel_id,
                message_id,
            } => {
                if self.state.current_channel.as_deref() == Some(channel_id.as_str())
                    && let Some(message) =
                        self.state.messages.iter_mut().find(|m| m.id == message_id)
                {
                    message.reactions.clear();
                }
            }
            Update::MessagePinned {
                channel_id,
                message_id,
                pinned,
            } => {
                if self.state.current_channel.as_deref() == Some(channel_id.as_str())
                    && let Some(message) =
                        self.state.messages.iter_mut().find(|m| m.id == message_id)
                {
                    message.pinned = Some(pinned);
                }
            }
            Update::ProfileUpdated(user) => {
                self.profile_display = user.display_name().to_string();
                self.profile_pronouns = user.pronouns.clone().unwrap_or_default();
                self.profile_bio = user.bio.clone().unwrap_or_default();
                self.state.me = Some(user);
            }
            Update::Discovery {
                guilds,
                total,
                append,
            } => {
                if append {
                    for guild in guilds {
                        if !self
                            .state
                            .discovery
                            .iter()
                            .any(|known| known.id == guild.id)
                        {
                            self.state.discovery.push(guild);
                        }
                    }
                } else {
                    self.state.discovery = guilds;
                }
                self.state.discovery_total = total;
                self.state.discovery_loading = false;
            }
            Update::DiscoveryLoading(loading) => self.state.discovery_loading = loading,
            Update::InviteAccepted {
                guild_id,
                dm_channel_id,
            } => {
                self.join_busy = false;
                self.show_join_guild = false;
                self.join_invite.clear();
                if let Some(guild_id) = guild_id {
                    self.view = View::Guilds;
                    self.command(Command::SelectGuild(guild_id));
                } else if let Some(channel_id) = dm_channel_id {
                    self.view = View::Dms;
                    self.command(Command::SelectDm(channel_id));
                }
                self.state.status = "Invitation acceptée.".into();
            }
            Update::InvitePreview { code, invite } => {
                self.invite_pending.remove(&code);
                self.invites.insert(code, invite);
            }
            Update::PrivateChannels(channels) => {
                self.state.private_channels = channels;
                // Opening the DM home directly (debug or first click) selects
                // the most recent conversation instead of an empty panel.
                let current_is_dm = self
                    .state
                    .current_channel
                    .as_deref()
                    .map(|id| {
                        self.state
                            .private_channels
                            .iter()
                            .any(|channel| channel.id == id)
                    })
                    .unwrap_or(false);
                if self.view == View::Dms
                    && !current_is_dm
                    && let Some(first) = self
                        .state
                        .private_channels
                        .iter()
                        .max_by(|a, b| a.last_message_id.cmp(&b.last_message_id))
                {
                    self.command(Command::SelectDm(first.id.clone()));
                }
            }
            Update::Voice {
                guild_id,
                channel_id,
            } => {
                self.state.voice_guild = guild_id;
                self.state.voice_channel = channel_id;
                if self.state.voice_channel.is_none() {
                    self.state.voice_members.clear();
                }
            }
            Update::VoiceMembers(members) => self.state.voice_members = members,
            Update::VoiceConnected(connected) => self.state.voice_connected = connected,
            Update::VoiceSelf { muted, deafened } => {
                self.state.voice_muted = muted;
                self.state.voice_deafened = deafened;
            }
            Update::Gateway(status) => self.state.gateway = status,
            Update::MediaBase(base) => self.state.media_base = base,
            Update::Presence { user_id, status } => {
                self.state.presence.insert(user_id, status);
            }
            Update::LoadingChannels(loading) => self.state.loading_channels = loading,
            Update::LoadingMessages(loading) => self.state.loading_messages = loading,
            Update::LoadingMembers(loading) => self.state.loading_members = loading,
            Update::TypingStarted {
                channel_id,
                user_id,
            } => {
                if !self.state.me.as_ref().is_some_and(|me| me.id == user_id) {
                    self.typing.insert(channel_id, (user_id, Instant::now()));
                }
            }
        }
    }
}

impl eframe::App for FluxerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.pump();
        // Glisser-déposer natif : les fichiers sont envoyés en pièce jointe
        // dans le salon courant (taille limitée pour rester réactif).
        let dropped = ui.ctx().input(|input| input.raw.dropped_files.clone());
        for file in dropped {
            let path = file.path().to_path_buf();
            let Some(name) = path
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
            else {
                continue;
            };
            match file.bytes() {
                Ok(bytes) if bytes.len() <= 25 * 1024 * 1024 => {
                    let mime = mime_for(&name);
                    self.send_attachment(name, mime, bytes);
                }
                Ok(_) => {
                    self.state.status = "Fichier ignoré : 25 Mio maximum.".to_string();
                }
                Err(error) => {
                    self.state.status = format!("Impossible de lire {name} : {error}");
                }
            }
        }
        // Debug helper: keep the requested settings tab pinned (the headless
        // compositor can otherwise deliver stray clicks).
        if let Ok(name) = std::env::var("FLUXER_SETTINGS") {
            self.show_settings = true;
            self.settings_tab = debug_settings_tab(&name);
        }
        // Debug helper: keep the requested destination pinned as well, for the
        // same headless-capture reason.
        if let Ok(name) = std::env::var("FLUXER_VIEW") {
            match name.as_str() {
                "dms" => self.view = View::Dms,
                "favorites" => self.view = View::Favorites,
                "discovery" => {
                    self.view = View::Discovery;
                    if !self.state.discovery_auto_requested
                        && self.state.discovery.is_empty()
                        && !self.state.discovery_loading
                    {
                        self.state.discovery_auto_requested = true;
                        self.command(Command::LoadDiscovery { offset: 0 });
                    }
                }
                "guilds" => self.view = View::Guilds,
                _ => {}
            }
        }
        self.maybe_screenshot(ui.ctx());
        crate::ui::draw(self, ui);
    }
}

/// Encode un fichier image local en data URI base64 pour l'API REST.
#[allow(dead_code)]
pub fn file_to_data_uri(path: &std::path::Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Impossible de lire le fichier: {e}"))?;
    if bytes.len() > 10 * 1024 * 1024 {
        return Err("Le fichier dépasse la limite de 10 Mio".to_string());
    }
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => "image/png",
    };
    let b64 = base64_encode(&bytes);
    Ok(format!("data:{mime};base64,{b64}"))
}

/// Encodage base64 standard (RFC 4648) 100 % Rust, sans dépendance externe.
#[allow(dead_code)]
pub fn base64_encode(bytes: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        out.push(CHARSET[(b0 >> 2) as usize] as char);
        out.push(CHARSET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(CHARSET[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(CHARSET[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Type MIME approximatif déduit de l'extension, pour le téléversement.
fn mime_for(name: &str) -> Option<String> {
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())?;
    let mime = match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "md" => "text/markdown",
        "json" => "application/json",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    };
    Some(mime.to_string())
}

fn debug_settings_tab(name: &str) -> usize {
    match name {
        "profile" => 1,
        "appearance" => 2,
        "chat" => 3,
        "notifications" => 4,
        "voice" => 5,
        "shortcuts" => 6,
        "language" => 7,
        "linux" => 8,
        "advanced" => 9,
        "about" => 10,
        _ => 0,
    }
}

#[allow(dead_code)]
impl FluxerApp {
    /// Debug helper used for visual regression: `FLUXER_SCREENSHOT=/tmp/x.png`
    /// asks the app to save its own framebuffer after a few seconds. This is a
    /// pure-Rust capture, independent of the Wayland compositor.
    fn maybe_screenshot(&mut self, ctx: &egui::Context) {
        if self.screenshot_taken {
            return;
        }
        let Ok(path) = std::env::var("FLUXER_SCREENSHOT") else {
            return;
        };
        if self.screenshot_start.elapsed() < std::time::Duration::from_secs(8) {
            return;
        }
        self.screenshot_taken = true;

        let callback =
            egui::ScreenshotCallback::new(move |image: std::sync::Arc<egui::ColorImage>| {
                let raw: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
                match image::save_buffer(
                    &path,
                    &raw,
                    image.width() as u32,
                    image.height() as u32,
                    image::ColorType::Rgba8,
                ) {
                    Ok(()) => eprintln!("screenshot enregistrée: {path}"),
                    Err(err) => eprintln!("échec de la capture: {err}"),
                }
            });
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(callback));
    }
}

/// Codes d'invitation cités dans le texte d'un message (`fluxer.gg/<code>`,
/// `fluxer.app/invite/<code>`…). Le client officiel affiche une carte pour
/// chacun.
pub fn invite_codes(content: &str) -> Vec<String> {
    const BASES: [&str; 6] = [
        "fluxer.gg/",
        "fluxer.app/invite/",
        "fluxer.com/invite/",
        "fluxer.app/i/",
        "canary.fluxer.app/invite/",
        "web.fluxer.app/invite/",
    ];
    let mut found = Vec::new();
    for raw in content.split_whitespace() {
        let token = raw.trim_matches(|c: char| {
            matches!(
                c,
                '(' | ')' | '[' | ']' | '<' | '>' | '"' | '\'' | ',' | '.' | ';' | '!'
            )
        });
        let Some(position) = BASES
            .iter()
            .filter_map(|base| token.find(base).map(|index| (index, base.len())))
            .min_by_key(|(index, _)| *index)
        else {
            continue;
        };
        let rest = &token[position.0 + position.1..];
        let candidate = rest.split(['/', '?', '#']).next().unwrap_or(rest);
        let valid = candidate.len() >= 2
            && candidate
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));
        if valid {
            let code = candidate.to_string();
            if !found.contains(&code) {
                found.push(code);
            }
        }
    }
    found
}

/// Extrait le code d'une invitation collée sous forme d'URL ou de code brut.
fn invite_code(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = trimmed
        .split(['?', '#'])
        .next()
        .unwrap_or(trimmed)
        .trim_end_matches('/');
    let candidate = path.rsplit('/').find(|part| !part.is_empty())?;
    let valid = candidate.len() >= 2
        && candidate
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));
    valid.then(|| candidate.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_invite_codes() {
        assert_eq!(invite_code("abc123").as_deref(), Some("abc123"));
        assert_eq!(
            invite_code("https://fluxer.gg/abc123?event=1").as_deref(),
            Some("abc123")
        );
        assert_eq!(
            invite_code("https://web.fluxer.app/invite/AbC-9").as_deref(),
            Some("AbC-9")
        );
        assert_eq!(invite_code("  "), None);
        assert_eq!(invite_code("bad code"), None);
    }

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }
}
