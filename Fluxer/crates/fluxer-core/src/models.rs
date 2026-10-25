// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! Public data model of the Fluxer API.
//!
//! The wire format is JSON and closely follows the Discord family of APIs:
//! snowflake identifiers are strings, timestamps are RFC 3339 strings and most
//! collection endpoints return bare arrays.

use serde::{Deserialize, Serialize};

/// Channel types supported by Fluxer (`type` field of a channel).
///
/// The values mirror the constants used by the official client:
/// `GUILD_TEXT=0`, `DM=1`, `GUILD_VOICE=2`, `GROUP_DM=3`,
/// `GUILD_CATEGORY=4`, `GUILD_ANNOUNCEMENT=5`, `ANNOUNCEMENT_THREAD=10`,
/// `PUBLIC_THREAD=11`, `PRIVATE_THREAD=12`, `GUILD_FORUM=15`,
/// `GUILD_MEDIA=16`, `GUILD_LINK=998` and `DM_PERSONAL_NOTES=999`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    Text,
    DirectMessage,
    Voice,
    GroupDm,
    Category,
    Announcement,
    AnnouncementThread,
    PublicThread,
    PrivateThread,
    Forum,
    Media,
    /// Lien externe affiché comme un salon (par exemple `fluxer.gg/fr`).
    Link,
    /// Bloc-notes personnel, stocké comme un MP avec soi-même.
    PersonalNotes,
    Other(i64),
}

impl ChannelKind {
    pub fn from_raw(raw: i64) -> Self {
        match raw {
            0 => Self::Text,
            1 => Self::DirectMessage,
            2 => Self::Voice,
            3 => Self::GroupDm,
            4 => Self::Category,
            5 => Self::Announcement,
            10 => Self::AnnouncementThread,
            11 => Self::PublicThread,
            12 => Self::PrivateThread,
            15 => Self::Forum,
            16 => Self::Media,
            998 => Self::Link,
            999 => Self::PersonalNotes,
            other => Self::Other(other),
        }
    }

    /// Valeur numérique envoyée à l'API.
    pub fn raw(self) -> i64 {
        match self {
            Self::Text => 0,
            Self::DirectMessage => 1,
            Self::Voice => 2,
            Self::GroupDm => 3,
            Self::Category => 4,
            Self::Announcement => 5,
            Self::AnnouncementThread => 10,
            Self::PublicThread => 11,
            Self::PrivateThread => 12,
            Self::Forum => 15,
            Self::Media => 16,
            Self::Link => 998,
            Self::PersonalNotes => 999,
            Self::Other(raw) => raw,
        }
    }

    /// Channels rendered as a message timeline. Les salons de lien ouvrent une
    /// URL dans le navigateur et ne contiennent pas de messages.
    pub fn is_readable(self) -> bool {
        matches!(
            self,
            Self::Text
                | Self::Announcement
                | Self::DirectMessage
                | Self::GroupDm
                | Self::AnnouncementThread
                | Self::PublicThread
                | Self::PrivateThread
                | Self::PersonalNotes
        )
    }

    /// Salon de conversation privée (MP simple, groupe ou bloc-notes).
    pub fn is_direct(self) -> bool {
        matches!(
            self,
            Self::DirectMessage | Self::GroupDm | Self::PersonalNotes
        )
    }

    pub fn is_thread(self) -> bool {
        matches!(
            self,
            Self::AnnouncementThread | Self::PublicThread | Self::PrivateThread
        )
    }

    pub fn is_forum(self) -> bool {
        matches!(self, Self::Forum | Self::Media)
    }

    pub fn is_link(self) -> bool {
        matches!(self, Self::Link)
    }

    /// Libellé français du type, utilisé par les infobulles et les réglages.
    pub fn label(self) -> &'static str {
        match self {
            Self::Text => "Salon textuel",
            Self::DirectMessage => "Message privé",
            Self::Voice => "Salon vocal",
            Self::GroupDm => "Groupe de MP",
            Self::Category => "Catégorie",
            Self::Announcement => "Salon d'annonces",
            Self::AnnouncementThread => "Fil d'annonces",
            Self::PublicThread => "Fil public",
            Self::PrivateThread => "Fil privé",
            Self::Forum => "Forum",
            Self::Media => "Salon média",
            Self::Link => "Salon de lien",
            Self::PersonalNotes => "Notes personnelles",
            Self::Other(_) => "Salon",
        }
    }
}

fn default_string() -> String {
    String::new()
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct User {
    pub id: String,
    #[serde(default = "default_string")]
    pub username: String,
    #[serde(default)]
    pub discriminator: Option<String>,
    #[serde(default)]
    pub global_name: Option<String>,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub avatar_color: Option<i64>,
    #[serde(default)]
    pub accent_color: Option<i64>,
    #[serde(default)]
    pub banner_color: Option<i64>,
    #[serde(default)]
    pub bio: Option<String>,
    #[serde(default)]
    pub pronouns: Option<String>,
    #[serde(default)]
    pub banner: Option<String>,
    #[serde(default)]
    pub bot: Option<bool>,
}

impl User {
    /// Name shown in the UI, mirroring what the official client displays.
    pub fn display_name(&self) -> &str {
        self.global_name
            .as_deref()
            .filter(|name| !name.is_empty())
            .unwrap_or(&self.username)
    }

    pub fn initials(&self) -> String {
        let source = self.display_name().trim();
        let mut words = source
            .split_whitespace()
            .filter_map(|word| word.chars().next());
        match (words.next(), words.next()) {
            (Some(a), Some(b)) => format!("{a}{b}").to_uppercase(),
            (Some(a), None) => a.to_uppercase().to_string(),
            _ => "?".into(),
        }
    }

    pub fn avatar_url(&self, media: &str) -> Option<String> {
        self.avatar
            .as_deref()
            .filter(|hash| !hash.is_empty())
            .map(|hash| crate::media::avatar_url(media, &self.id, hash))
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Guild {
    pub id: String,
    #[serde(default = "default_string")]
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub banner: Option<String>,
    #[serde(default)]
    pub banner_width: Option<i64>,
    #[serde(default)]
    pub banner_height: Option<i64>,
    #[serde(default)]
    pub splash: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub owner_id: Option<String>,
    #[serde(default)]
    pub permissions: Option<String>,
    #[serde(default)]
    pub member_count: Option<i64>,
    #[serde(default)]
    pub online_count: Option<i64>,
    #[serde(default)]
    pub features: Vec<String>,
}

impl Guild {
    pub fn initials(&self) -> String {
        let source = self.name.trim();
        let mut chars = source.chars();
        match (chars.next(), chars.next()) {
            (Some(a), Some(b)) => format!("{a}{b}").to_uppercase(),
            (Some(a), None) => a.to_uppercase().to_string(),
            _ => "?".into(),
        }
    }

    pub fn icon_url(&self, media: &str) -> Option<String> {
        self.icon
            .as_deref()
            .filter(|hash| !hash.is_empty())
            .map(|hash| crate::media::guild_icon_url(media, &self.id, hash))
    }

    pub fn banner_url(&self, media: &str) -> Option<String> {
        self.banner
            .as_deref()
            .filter(|hash| !hash.is_empty())
            .map(|hash| crate::media::guild_banner_url(media, &self.id, hash))
    }
}

/// Guilde publique renvoyée par l'annuaire de découverte.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct DiscoveryGuild {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub banner: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub category_type: Option<i64>,
    #[serde(default)]
    pub primary_language: Option<String>,
    #[serde(default)]
    pub custom_tags: Vec<String>,
    #[serde(default)]
    pub member_count: Option<i64>,
    #[serde(default)]
    pub online_count: Option<i64>,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub verification_level: Option<i64>,
    #[serde(default)]
    pub visits_enabled: Option<bool>,
}

impl DiscoveryGuild {
    pub fn initials(&self) -> String {
        let source = self.name.trim();
        let mut chars = source.chars();
        match (chars.next(), chars.next()) {
            (Some(a), Some(b)) => format!("{a}{b}").to_uppercase(),
            (Some(a), None) => a.to_uppercase().to_string(),
            _ => "?".into(),
        }
    }

    pub fn icon_url(&self, media: &str) -> Option<String> {
        self.icon
            .as_deref()
            .filter(|hash| !hash.is_empty())
            .map(|hash| crate::media::guild_icon_url(media, &self.id, hash))
    }
}

/// A guild role. `color == 0` means "default text colour".
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Role {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub color: i64,
    #[serde(default)]
    pub position: i64,
    #[serde(default)]
    pub hoist: bool,
    #[serde(default)]
    pub mentionable: bool,
    #[serde(default)]
    pub permissions: Option<String>,
}

/// A guild member as returned by `GET /v1/guilds/{guild}/members`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Member {
    #[serde(default)]
    pub user: User,
    #[serde(default)]
    pub nick: Option<String>,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub joined_at: Option<String>,
    #[serde(default)]
    pub mute: bool,
    #[serde(default)]
    pub deaf: bool,
}

impl Member {
    pub fn display_name(&self) -> &str {
        self.nick
            .as_deref()
            .filter(|nick| !nick.is_empty())
            .unwrap_or_else(|| self.user.display_name())
    }
}

/// Full guild object (`GET /v1/guilds/{id}`), richer than the list entry.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct GuildDetail {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub banner: Option<String>,
    #[serde(default)]
    pub banner_width: Option<i64>,
    #[serde(default)]
    pub banner_height: Option<i64>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub member_count: Option<i64>,
    #[serde(default)]
    pub online_count: Option<i64>,
    #[serde(default)]
    pub roles: Vec<Role>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Channel {
    pub id: String,
    #[serde(default)]
    pub r#type: i64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub guild_id: Option<String>,
    #[serde(default)]
    pub position: Option<i64>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub topic: Option<String>,
    #[serde(default)]
    pub nsfw: Option<bool>,
    /// Participants d'un MP / groupe de MP.
    #[serde(default)]
    pub recipients: Vec<User>,
    #[serde(default)]
    pub last_message_id: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    /// Cible d'un salon de lien (`type == 998`).
    #[serde(default)]
    pub url: Option<String>,
}

impl Channel {
    pub fn kind(&self) -> ChannelKind {
        ChannelKind::from_raw(self.r#type)
    }

    pub fn label(&self) -> String {
        if let Some(name) = self.name.clone().filter(|name| !name.is_empty()) {
            return name;
        }
        if self.kind().is_direct() {
            return self.direct_label();
        }
        format!("salon-{}", &self.id[..self.id.len().min(6)])
    }

    /// Nom affiché d'une conversation privée, dérivé des destinataires.
    pub fn direct_label(&self) -> String {
        match self.recipients.as_slice() {
            [] => "Conversation privée".to_string(),
            [only] => only.display_name().to_string(),
            many => many
                .iter()
                .map(|user| user.display_name())
                .collect::<Vec<_>>()
                .join(", "),
        }
    }

    /// Avatar d'une conversation privée (premier destinataire).
    pub fn dm_user(&self) -> Option<&User> {
        self.recipients.first()
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Attachment {
    pub id: String,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub proxy_url: Option<String>,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub width: Option<i64>,
    #[serde(default)]
    pub height: Option<i64>,
    /// Durée d'une vidéo ou d'un son, en secondes.
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub nsfw: bool,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub flags: i64,
}

impl Attachment {
    /// Aperçu visuel possible : l'API fournit toujours les dimensions des
    /// images et des vidéos, ce qui permet de réserver la bonne place avant
    /// le décodage asynchrone.
    pub fn visual_size(&self) -> Option<(f32, f32)> {
        let width = self.width? as f32;
        let height = self.height? as f32;
        if width <= 0.0 || height <= 0.0 {
            return None;
        }
        let kind = self.content_type.as_deref().unwrap_or_default();
        if kind.starts_with("image/") || kind.starts_with("video/") {
            Some((width, height))
        } else {
            None
        }
    }

    pub fn is_video(&self) -> bool {
        self.content_type
            .as_deref()
            .is_some_and(|kind| kind.starts_with("video/"))
    }

    pub fn is_image(&self) -> bool {
        self.content_type
            .as_deref()
            .is_some_and(|kind| kind.starts_with("image/"))
    }

    /// Média réellement affichable en ligne : on ne décode pas les vidéos dans
    /// la timeline, elles ouvrent le lecteur externe.
    pub fn display_url(&self) -> Option<&str> {
        self.proxy_url
            .as_deref()
            .filter(|url| !url.is_empty())
            .or_else(|| self.url.as_deref().filter(|url| !url.is_empty()))
    }
}

/// Média d'un embed (image, vignette, vidéo ou audio).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct EmbedMedia {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub proxy_url: Option<String>,
    #[serde(default)]
    pub width: Option<i64>,
    #[serde(default)]
    pub height: Option<i64>,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub duration: Option<f64>,
}

impl EmbedMedia {
    pub fn src(&self) -> Option<&str> {
        self.proxy_url
            .as_deref()
            .filter(|url| !url.is_empty())
            .or_else(|| self.url.as_deref().filter(|url| !url.is_empty()))
    }

    pub fn dimensions(&self) -> Option<(f32, f32)> {
        let width = self.width? as f32;
        let height = self.height? as f32;
        (width > 0.0 && height > 0.0).then_some((width, height))
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct EmbedAuthor {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub proxy_icon_url: Option<String>,
}

impl EmbedAuthor {
    pub fn icon(&self) -> Option<&str> {
        self.proxy_icon_url
            .as_deref()
            .filter(|url| !url.is_empty())
            .or_else(|| self.icon_url.as_deref().filter(|url| !url.is_empty()))
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct EmbedFooter {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub proxy_icon_url: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct EmbedField {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub inline: bool,
}

/// Carte enrichie renvoyée par l'API pour un lien (OpenGraph, YouTube, GIF…).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Embed {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default = "default_embed_type")]
    pub r#type: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub color: Option<i64>,
    #[serde(default)]
    pub timestamp: Option<String>,
    #[serde(default)]
    pub author: Option<EmbedAuthor>,
    #[serde(default)]
    pub provider: Option<EmbedAuthor>,
    #[serde(default)]
    pub footer: Option<EmbedFooter>,
    #[serde(default)]
    pub image: Option<EmbedMedia>,
    #[serde(default)]
    pub thumbnail: Option<EmbedMedia>,
    #[serde(default)]
    pub video: Option<EmbedMedia>,
    #[serde(default)]
    pub audio: Option<EmbedMedia>,
    #[serde(default)]
    pub fields: Vec<EmbedField>,
}

fn default_embed_type() -> String {
    "rich".into()
}

impl Embed {
    /// Un embed sans titre, texte, auteur ni média n'est jamais affiché par le
    /// client officiel (doublon d'un lien déjà rendu par Markdown).
    pub fn has_content(&self) -> bool {
        self.title.as_ref().is_some_and(|title| !title.is_empty())
            || self
                .description
                .as_ref()
                .is_some_and(|description| !description.is_empty())
            || self.author.is_some()
            || self.footer.is_some()
            || !self.fields.is_empty()
            || self.image.is_some()
            || self.thumbnail.is_some()
            || self.video.is_some()
            || self.audio.is_some()
    }

    pub fn border_color(&self) -> Option<i64> {
        self.color.filter(|color| *color > 0)
    }
}

/// Guilde réduite renvoyée par `GET /v1/invites/{code}`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct InviteGuild {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub banner: Option<String>,
    #[serde(default)]
    pub splash: Option<String>,
    #[serde(default)]
    pub embed_splash: Option<String>,
    #[serde(default)]
    pub features: Vec<String>,
}

impl InviteGuild {
    pub fn icon_url(&self, media: &str) -> Option<String> {
        self.icon
            .as_deref()
            .filter(|hash| !hash.is_empty())
            .map(|hash| crate::media::guild_icon_url(media, &self.id, hash))
    }

    pub fn splash_url(&self, media: &str) -> Option<String> {
        self.embed_splash
            .as_deref()
            .filter(|hash| !hash.is_empty())
            .map(|hash| crate::media::guild_embed_splash_url(media, &self.id, hash))
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct InviteChannel {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub r#type: Option<i64>,
}

/// Aperçu d'invitation affiché sous forme de carte dans la timeline.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct InvitePreview {
    pub code: String,
    #[serde(default)]
    pub guild: Option<InviteGuild>,
    #[serde(default)]
    pub channel: Option<InviteChannel>,
    #[serde(default)]
    pub member_count: i64,
    #[serde(default)]
    pub presence_count: i64,
    #[serde(default)]
    pub expires_at: Option<String>,
}

/// Emoji d'une réaction : soit un emoji Unicode (`name` seul), soit un emoji
/// personnalisé de guilde (`name:id`).
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Emoji {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub animated: Option<bool>,
}

impl Emoji {
    /// Clé stable utilisée pour fusionner les réactions : `id:name`.
    pub fn key(&self) -> String {
        format!("{}:{}", self.id.as_deref().unwrap_or_default(), self.name)
    }

    /// Forme attendue dans l'URL REST : `nom` ou `nom:id`.
    pub fn query(&self) -> String {
        match &self.id {
            Some(id) if !id.is_empty() => format!("{}:{}", self.name, id),
            _ => self.name.clone(),
        }
    }
}

impl From<&str> for Emoji {
    fn from(name: &str) -> Self {
        // Un emoji personnalisé s'écrit `nom:identifiant` dans l'interface.
        match name.split_once(':') {
            Some((name, id)) if !name.is_empty() && !id.is_empty() => Self {
                id: Some(id.to_string()),
                name: name.to_string(),
                animated: None,
            },
            _ => Self {
                id: None,
                name: name.to_string(),
                animated: None,
            },
        }
    }
}

/// Compteur de réactions pour un emoji donné, sur un message.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Reaction {
    #[serde(default)]
    pub emoji: Emoji,
    #[serde(default)]
    pub count: i64,
    #[serde(default)]
    pub me: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Message {
    pub id: String,
    #[serde(default)]
    pub channel_id: String,
    #[serde(default)]
    pub guild_id: Option<String>,
    #[serde(default)]
    pub author: User,
    #[serde(default = "default_string")]
    pub content: String,
    #[serde(default)]
    pub timestamp: String,
    #[serde(default)]
    pub edited_timestamp: Option<String>,
    #[serde(default)]
    pub pinned: Option<bool>,
    #[serde(default)]
    pub tts: Option<bool>,
    #[serde(default)]
    pub r#type: Option<i64>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(default)]
    pub mentions: Vec<User>,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
    #[serde(default)]
    pub embeds: Vec<Embed>,
    #[serde(default)]
    pub referenced_message: Option<Box<Message>>,
}

impl Message {
    /// `HH:MM` in local time, falling back to the raw wire value.
    pub fn short_time(&self) -> String {
        chrono::DateTime::parse_from_rfc3339(&self.timestamp)
            .map(|dt| dt.with_timezone(&chrono::Local).format("%H:%M").to_string())
            .unwrap_or_else(|_| self.timestamp.chars().take(16).collect())
    }

    pub fn is_edited(&self) -> bool {
        self.edited_timestamp.is_some()
    }

    /// Applique un delta de réaction reçu de la passerelle ou du REST.
    ///
    /// `me` permet de mettre à jour l'état local sans connaître l'identifiant
    /// du compte courant : `Some(true)` ajoute la réaction propre, `Some(false)`
    /// la retire, `None` laisse l'état inchangé (réaction d'un autre membre).
    pub fn apply_reaction(&mut self, emoji: &Emoji, delta: i64, me: Option<bool>) {
        let key = emoji.key();
        if let Some(existing) = self.reactions.iter_mut().find(|r| r.emoji.key() == key) {
            existing.count = (existing.count + delta).max(0);
            if let Some(me) = me {
                existing.me = me;
            }
            if existing.count == 0 {
                self.reactions.retain(|r| r.emoji.key() != key);
            }
        } else if delta > 0 {
            self.reactions.push(Reaction {
                emoji: emoji.clone(),
                count: delta,
                me: me.unwrap_or(false),
            });
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReadyPayload {
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub user: Option<User>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_kinds_map_to_readable() {
        assert!(ChannelKind::from_raw(0).is_readable());
        assert!(ChannelKind::from_raw(5).is_readable());
        assert!(ChannelKind::from_raw(11).is_readable());
        assert!(!ChannelKind::from_raw(2).is_readable());
        assert!(!ChannelKind::from_raw(4).is_readable());
        assert!(ChannelKind::from_raw(15).is_forum());
    }

    #[test]
    fn display_name_prefers_global_name() {
        let user: User =
            serde_json::from_str(r#"{"id":"1","username":"alice","global_name":"Alice W."}"#)
                .unwrap();
        assert_eq!(user.display_name(), "Alice W.");
        assert_eq!(user.initials(), "AW");
    }
}
