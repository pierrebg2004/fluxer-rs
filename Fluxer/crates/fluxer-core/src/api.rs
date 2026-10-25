// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! Authenticated REST client for the Fluxer HTTP API.
//!
//! Route layout (verified against the live service):
//!   GET    /v1/users/@me
//!   GET    /v1/users/@me/guilds
//!   GET    /v1/guilds/{guild}/channels
//!   GET    /v1/channels/{channel}/messages?limit=50[&before={message}]
//!   POST   /v1/channels/{channel}/messages   {"content": "..."}
//!
//! Authentication is the raw account token in the `Authorization` header (no
//! `Bearer` prefix), exactly like the official desktop client.

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::discovery::Endpoints;
use crate::error::{Error, Result, describe_body};
use crate::media;
use crate::models::{
    Channel, DiscoveryGuild, Guild, GuildDetail, InvitePreview, Member, Message, Role, User,
};

const USER_AGENT: &str = concat!("fluxer-rs/", env!("CARGO_PKG_VERSION"));

#[derive(serde::Deserialize)]
struct DiscoveryPage {
    #[serde(default)]
    guilds: Vec<DiscoveryGuild>,
    #[serde(default)]
    total: i64,
}

#[derive(Clone)]
pub struct ApiClient {
    http: reqwest::Client,
    base: String,
    gateway: String,
    media: String,
    token: String,
}

impl ApiClient {
    pub fn new(endpoints: &Endpoints, token: impl Into<String>) -> Result<Self> {
        // Le serveur refuse les écritures sans `Origin` autorisé
        // (`INVALID_API_ORIGIN`). L'application web déclare son origine dans le
        // document de découverte ; on la reprend telle quelle, avec l'API comme
        // repli pour les instances qui n'exposent pas d'application web.
        let origin = endpoints
            .webapp
            .as_deref()
            .filter(|webapp| !webapp.is_empty())
            .unwrap_or_else(|| endpoints.api_base())
            .trim_end_matches('/')
            .to_string();
        let mut headers = reqwest::header::HeaderMap::new();
        if let Ok(value) = reqwest::header::HeaderValue::from_str(&origin) {
            headers.insert(reqwest::header::ORIGIN, value);
        }
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .default_headers(headers)
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()?;

        Ok(Self {
            http,
            base: endpoints.api_base().to_string(),
            gateway: endpoints.gateway.clone(),
            media: endpoints.media_base().to_string(),
            token: token.into(),
        })
    }

    pub fn gateway_url(&self) -> &str {
        &self.gateway
    }

    pub fn api_base(&self) -> &str {
        &self.base
    }

    pub fn media_base(&self) -> &str {
        &self.media
    }

    /// Convenience: absolute URL for an avatar hash (or the raw URL when the
    /// field already carries one).
    pub fn media_url(&self, path: &str) -> String {
        media::absolute(&self.media, path)
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base, path);
        let response = self
            .http
            .get(&url)
            .header(reqwest::header::AUTHORIZATION, &self.token)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await?;
        self.decode(response).await
    }

    async fn post<T: DeserializeOwned>(&self, path: &str, body: serde_json::Value) -> Result<T> {
        let url = format!("{}{}", self.base, path);
        let response = self
            .http
            .post(&url)
            .header(reqwest::header::AUTHORIZATION, &self.token)
            .header(reqwest::header::ACCEPT, "application/json")
            .json(&body)
            .send()
            .await?;
        self.decode(response).await
    }

    async fn patch<T: DeserializeOwned>(&self, path: &str, body: serde_json::Value) -> Result<T> {
        let url = format!("{}{}", self.base, path);
        let response = self
            .http
            .patch(&url)
            .header(reqwest::header::AUTHORIZATION, &self.token)
            .header(reqwest::header::ACCEPT, "application/json")
            .json(&body)
            .send()
            .await?;
        self.decode(response).await
    }

    /// `DELETE` renvoie généralement `204 No Content` : on ne désérialise rien.
    async fn delete(&self, path: &str) -> Result<()> {
        let url = format!("{}{}", self.base, path);
        let response = self
            .http
            .delete(&url)
            .header(reqwest::header::AUTHORIZATION, &self.token)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await?;
        let status = response.status();
        if status.as_u16() == 429 {
            let retry = response
                .headers()
                .get("retry-after")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<f64>().ok())
                .unwrap_or(1.0);
            return Err(Error::RateLimited { retry_after: retry });
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(describe_body(status.as_u16(), &body));
        }
        Ok(())
    }

    async fn decode<T: DeserializeOwned>(&self, response: reqwest::Response) -> Result<T> {
        let status = response.status();
        if status.as_u16() == 429 {
            let retry = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(1.0);
            return Err(Error::RateLimited { retry_after: retry });
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(describe_body(status.as_u16(), &body));
        }
        Ok(response.json::<T>().await?)
    }

    pub async fn me(&self) -> Result<User> {
        self.get("/v1/users/@me").await
    }

    pub async fn guilds(&self) -> Result<Vec<Guild>> {
        self.get("/v1/users/@me/guilds").await
    }

    pub async fn channels(&self, guild_id: &str) -> Result<Vec<Channel>> {
        self.get(&format!("/v1/guilds/{guild_id}/channels")).await
    }

    /// Conversations privées (MP simples et groupes) de l'utilisateur connecté.
    pub async fn private_channels(&self) -> Result<Vec<Channel>> {
        self.get("/v1/users/@me/channels").await
    }

    /// Accepte une invitation de guilde ou de groupe privé.
    pub async fn accept_invite(&self, code: &str) -> Result<Value> {
        self.post(&format!("/v1/invites/{code}"), json!({})).await
    }

    /// Aperçu public d'une invitation (carte affichée dans la timeline).
    pub async fn invite(&self, code: &str) -> Result<InvitePreview> {
        self.get(&format!("/v1/invites/{code}")).await
    }

    /// Communautés publiques mises en avant par l'annuaire de l'instance.
    ///
    /// L'API plafonne `limit` à 24 entrées par page.
    pub async fn discovery_guilds(&self, offset: u16) -> Result<(Vec<DiscoveryGuild>, i64)> {
        let page: DiscoveryPage = self
            .get(&format!("/v1/discovery/guilds?limit=24&offset={offset}"))
            .await?;
        Ok((page.guilds, page.total))
    }

    /// Rejoint une communauté publique depuis l'annuaire de découverte.
    pub async fn join_discovery_guild(&self, guild_id: &str) -> Result<()> {
        self.post::<Value>(&format!("/v1/discovery/guilds/{guild_id}/join"), json!({}))
            .await?;
        Ok(())
    }

    pub async fn guild_detail(&self, guild_id: &str) -> Result<GuildDetail> {
        self.get(&format!("/v1/guilds/{guild_id}")).await
    }

    pub async fn members(&self, guild_id: &str, limit: u16) -> Result<Vec<Member>> {
        self.get(&format!(
            "/v1/guilds/{guild_id}/members?limit={}",
            limit.min(1000)
        ))
        .await
    }

    /// Newest-first page of messages; pass `before` to page backwards.
    pub async fn messages(
        &self,
        channel_id: &str,
        limit: u16,
        before: Option<&str>,
    ) -> Result<Vec<Message>> {
        let mut path = format!(
            "/v1/channels/{channel_id}/messages?limit={}",
            limit.min(100)
        );
        if let Some(before) = before {
            path.push_str("&before=");
            path.push_str(before);
        }
        self.get(&path).await
    }

    pub async fn send_message(
        &self,
        channel_id: &str,
        content: &str,
        reply_to: Option<&str>,
    ) -> Result<Message> {
        // The nonce is echoed back through the gateway so the UI can de-duplicate
        // the optimistic copy with the authoritative event.
        let nonce = format!(
            "fluxer-rs-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        );
        let mut body = json!({ "content": content, "nonce": nonce, "tts": false });
        if let Some(reply_to) = reply_to {
            body["message_reference"] = json!({
                "message_id": reply_to,
                "channel_id": channel_id,
                "type": 0,
            });
        }
        self.post(&format!("/v1/channels/{channel_id}/messages"), body)
            .await
    }

    /// Modifie le contenu d'un message dont l'utilisateur est l'auteur.
    pub async fn edit_message(
        &self,
        channel_id: &str,
        message_id: &str,
        content: &str,
    ) -> Result<Message> {
        self.patch(
            &format!("/v1/channels/{channel_id}/messages/{message_id}"),
            json!({ "content": content }),
        )
        .await
    }

    /// Supprime un message. L'écho `MESSAGE_DELETE` de la passerelle met aussi
    /// l'interface à jour, mais on applique la réponse REST immédiatement.
    pub async fn delete_message(&self, channel_id: &str, message_id: &str) -> Result<()> {
        self.delete(&format!("/v1/channels/{channel_id}/messages/{message_id}"))
            .await
    }

    /// Ajoute une réaction. Le serveur renvoie `204 No Content` : aucun corps
    /// n'est désérialisé, l'état est mis à jour par l'écho de la passerelle.
    pub async fn add_reaction(
        &self,
        channel_id: &str,
        message_id: &str,
        emoji: &str,
    ) -> Result<()> {
        self.put_empty(&format!(
            "/v1/channels/{channel_id}/messages/{message_id}/reactions/{}/@me",
            encode_segment(emoji)
        ))
        .await
    }

    /// Retire la réaction de l'utilisateur courant.
    pub async fn remove_reaction(
        &self,
        channel_id: &str,
        message_id: &str,
        emoji: &str,
    ) -> Result<()> {
        self.delete(&format!(
            "/v1/channels/{channel_id}/messages/{message_id}/reactions/{}/@me",
            encode_segment(emoji)
        ))
        .await
    }

    /// Épingle ou désépingle un message (204 sans corps).
    pub async fn set_pinned(&self, channel_id: &str, message_id: &str, pinned: bool) -> Result<()> {
        let path = format!("/v1/channels/{channel_id}/pins/{message_id}");
        if pinned {
            self.put_empty(&path).await
        } else {
            self.delete(&path).await
        }
    }

    /// Envoie un fichier en pièce jointe avec un éventuel texte.
    /// Le serveur attend un corps `multipart/form-data` avec les parties
    /// `payload_json` et `files[0]`. Le texte peut être vide.
    pub async fn upload_attachment(
        &self,
        channel_id: &str,
        filename: &str,
        content_type: Option<&str>,
        bytes: Vec<u8>,
        content: &str,
    ) -> Result<Message> {
        let nonce = format!(
            "fluxer-rs-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        );
        let payload = json!({
            "content": content,
            "nonce": nonce,
            "tts": false,
            "attachments": [{ "id": 0, "filename": filename }],
        });
        let part = reqwest::multipart::Part::bytes(bytes)
            .file_name(filename.to_string())
            .mime_str(content_type.unwrap_or("application/octet-stream"))?;
        let form = reqwest::multipart::Form::new()
            .text("payload_json", payload.to_string())
            .part("files[0]", part);
        let url = format!("{}/v1/channels/{channel_id}/messages", self.base);
        let response = self
            .http
            .post(&url)
            .header(reqwest::header::AUTHORIZATION, &self.token)
            .header(reqwest::header::ACCEPT, "application/json")
            .multipart(form)
            .send()
            .await?;
        self.decode(response).await
    }

    /// Modifie les champs du profil courant. Le serveur peut exiger une
    /// vérification « sudo » : dans ce cas l'erreur est remontée telle quelle.
    pub async fn update_profile(&self, body: serde_json::Value) -> Result<User> {
        self.patch("/v1/users/@me", body).await
    }

    /// `PUT` sans corps dont la réponse `204` ne contient rien.
    async fn put_empty(&self, path: &str) -> Result<()> {
        let url = format!("{}{}", self.base, path);
        let response = self
            .http
            .put(&url)
            .header(reqwest::header::AUTHORIZATION, &self.token)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await?;
        let status = response.status();
        if status.as_u16() == 429 {
            let retry = response
                .headers()
                .get("retry-after")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<f64>().ok())
                .unwrap_or(1.0);
            return Err(Error::RateLimited { retry_after: retry });
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(describe_body(status.as_u16(), &body));
        }
        Ok(())
    }

    /// Modifie le nom/sujet d'un salon.
    pub async fn update_channel(
        &self,
        channel_id: &str,
        name: Option<&str>,
        topic: Option<&str>,
    ) -> Result<Channel> {
        let mut body = json!({});
        if let Some(name) = name {
            body["name"] = Value::String(name.to_string());
        }
        if let Some(topic) = topic {
            body["topic"] = Value::String(topic.to_string());
        }
        self.patch(&format!("/v1/channels/{channel_id}"), body)
            .await
    }

    /// Crée un post dans un salon forum/média.
    pub async fn start_forum_post(
        &self,
        channel_id: &str,
        name: &str,
        content: &str,
    ) -> Result<Channel> {
        self.post(
            &format!("/v1/channels/{channel_id}/threads"),
            json!({
                "name": name,
                "auto_archive_duration": 1440,
                "message": { "content": content }
            }),
        )
        .await
    }

    /// Signale une activité de frappe dans un salon (204 sans corps).
    pub async fn indicate_typing(&self, channel_id: &str) -> Result<()> {
        self.post_empty(&format!("/v1/channels/{channel_id}/typing"))
            .await
    }

    /// Liste les rôles d'un serveur.
    pub async fn guild_roles(&self, guild_id: &str) -> Result<Vec<Role>> {
        self.get(&format!("/v1/guilds/{guild_id}/roles")).await
    }

    /// Crée un rôle dans un serveur.
    pub async fn create_guild_role(&self, guild_id: &str, name: &str, color: i64) -> Result<Role> {
        self.post(
            &format!("/v1/guilds/{guild_id}/roles"),
            json!({ "name": name, "color": color, "permissions": "0" }),
        )
        .await
    }

    /// Met à jour un rôle (nom, couleur).
    pub async fn update_guild_role(
        &self,
        guild_id: &str,
        role_id: &str,
        name: Option<&str>,
        color: Option<i64>,
    ) -> Result<Role> {
        let mut body = json!({});
        if let Some(name) = name {
            body["name"] = Value::String(name.to_string());
        }
        if let Some(color) = color {
            body["color"] = Value::Number(color.into());
        }
        self.patch(&format!("/v1/guilds/{guild_id}/roles/{role_id}"), body)
            .await
    }

    /// Supprime un rôle.
    pub async fn delete_guild_role(&self, guild_id: &str, role_id: &str) -> Result<()> {
        self.delete(&format!("/v1/guilds/{guild_id}/roles/{role_id}"))
            .await
    }

    /// Assigne un rôle à un membre.
    pub async fn add_member_role(
        &self,
        guild_id: &str,
        user_id: &str,
        role_id: &str,
    ) -> Result<()> {
        self.put_empty(&format!(
            "/v1/guilds/{guild_id}/members/{user_id}/roles/{role_id}"
        ))
        .await
    }

    /// Retire un rôle d'un membre.
    pub async fn remove_member_role(
        &self,
        guild_id: &str,
        user_id: &str,
        role_id: &str,
    ) -> Result<()> {
        self.delete(&format!(
            "/v1/guilds/{guild_id}/members/{user_id}/roles/{role_id}"
        ))
        .await
    }

    /// Marque un salon comme lu jusqu'à un message donné (204 sans corps).
    pub async fn ack_message(
        &self,
        channel_id: &str,
        message_id: &str,
        mention_count: i64,
        manual: bool,
    ) -> Result<()> {
        self.post_empty_with_body(
            &format!("/v1/channels/{channel_id}/messages/{message_id}/ack"),
            json!({ "mention_count": mention_count, "manual": manual }),
        )
        .await
    }

    /// `POST` avec corps dont la réponse 204 ne contient rien.
    async fn post_empty_with_body(&self, path: &str, body: Value) -> Result<()> {
        let url = format!("{}{}", self.base, path);
        let response = self
            .http
            .post(&url)
            .header(reqwest::header::AUTHORIZATION, &self.token)
            .header(reqwest::header::ACCEPT, "application/json")
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        if status.as_u16() == 429 {
            return Ok(());
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(describe_body(status.as_u16(), &body));
        }
        Ok(())
    }

    /// Crée un fil public à partir d'un message existant.
    pub async fn start_thread_from_message(
        &self,
        channel_id: &str,
        message_id: &str,
        name: &str,
    ) -> Result<Channel> {
        self.post(
            &format!("/v1/channels/{channel_id}/messages/{message_id}/threads"),
            json!({ "name": name, "auto_archive_duration": 1440 }),
        )
        .await
    }

    /// `POST` sans corps dont la réponse 204 ne contient rien.
    async fn post_empty(&self, path: &str) -> Result<()> {
        let url = format!("{}{}", self.base, path);
        let response = self
            .http
            .post(&url)
            .header(reqwest::header::AUTHORIZATION, &self.token)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await?;
        let status = response.status();
        if status.as_u16() == 429 {
            return Ok(());
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(describe_body(status.as_u16(), &body));
        }
        Ok(())
    }
}

/// Encode un segment de chemin (emoji Unicode, emoji personnalisé `nom:id`).
///
/// On n'encode que ce qui est nécessaire : les octets non réservés restent
/// lisibles, le reste devient `%XX`, comme attendu par les routeurs HTTP.
fn encode_segment(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
