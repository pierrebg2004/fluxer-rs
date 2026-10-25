// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! Error types shared by the HTTP client, the gateway and the config store.

use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("réseau: {0}")]
    Http(#[from] reqwest::Error),

    #[error("websocket: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("url invalide: {0}")]
    Url(#[from] url::ParseError),

    #[error("session expirée ou invalide")]
    Unauthorized,

    #[error("accès refusé par l'API (HTTP {status}): {message}")]
    Forbidden { status: u16, message: String },

    #[error("l'API a répondu HTTP {status}: {message}")]
    Api { status: u16, message: String },

    #[error("limite de débit atteinte, réessayez dans {retry_after:.1}s")]
    RateLimited { retry_after: f64 },

    #[error("instance invalide: {0}")]
    InvalidInstance(String),

    #[error("découverte d'instance impossible ({url}, HTTP {status})")]
    Discovery { url: String, status: u16 },

    #[error("le document de découverte de {0} est inutilisable")]
    DiscoveryUnusable(String),

    #[error("stockage local: {0}")]
    Storage(String),

    #[error("aucune session Fluxer installée n'a été trouvée")]
    NoInstalledSession,

    #[error("erreur interne: {0}")]
    Internal(String),
}

impl Error {
    pub fn is_fatal(&self) -> bool {
        matches!(self, Error::Unauthorized)
    }

    pub fn user_hint(&self) -> String {
        match self {
            Error::Unauthorized => {
                "Token refusé. Réouvrez la session dans Fluxer puis réimportez-la.".into()
            }
            Error::RateLimited { retry_after } => {
                format!("Trop de requêtes, nouvelle tentative dans {retry_after:.0}s.")
            }
            other => other.to_string(),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Small helper to get a readable description out of an API error body.
pub fn describe_body(status: u16, body: &str) -> Error {
    let parsed: Option<serde_json::Value> = serde_json::from_str(body).ok();
    let message = parsed
        .as_ref()
        .and_then(|v| v.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or(body)
        .to_string();

    match status {
        401 => Error::Unauthorized,
        403 => Error::Forbidden { status, message },
        429 => Error::RateLimited {
            retry_after: parsed
                .as_ref()
                .and_then(|v| v.get("retry_after"))
                .and_then(|v| v.as_f64())
                .unwrap_or(1.0),
        },
        _ => Error::Api { status, message },
    }
}

impl fmt::Display for crate::models::ChannelKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Text => "texte",
            Self::DirectMessage => "message privé",
            Self::GroupDm => "groupe privé",
            Self::Voice => "vocal",
            Self::Category => "catégorie",
            Self::Announcement => "annonce",
            Self::AnnouncementThread => "fil d'annonces",
            Self::PublicThread => "fil public",
            Self::PrivateThread => "fil privé",
            Self::Forum => "forum",
            Self::Media => "média",
            Self::Link => "lien",
            Self::PersonalNotes => "notes personnelles",
            Self::Other(raw) => return write!(f, "type {raw}"),
        };
        f.write_str(label)
    }
}
