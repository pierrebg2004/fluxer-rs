// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! Instance discovery (`GET /.well-known/fluxer`).
//!
//! Every Fluxer deployment publishes a discovery document that advertises the
//! API, gateway, media and CDN endpoints. The desktop client caches the document
//! and pins the origins it is allowed to talk to; we do the same.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Default discovery origin for the public Fluxer instance.
pub const PUBLIC_DISCOVERY_ORIGIN: &str = "https://api.fluxer.app";

/// Endpoint table from the discovery document.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Endpoints {
    #[serde(default)]
    pub api: String,
    #[serde(default)]
    pub api_client: Option<String>,
    #[serde(default)]
    pub api_public: Option<String>,
    #[serde(default)]
    pub gateway: String,
    #[serde(default)]
    pub media: Option<String>,
    #[serde(default)]
    pub upload_relay: Option<String>,
    #[serde(default)]
    pub static_cdn: Option<String>,
    #[serde(default)]
    pub marketing: Option<String>,
    #[serde(default)]
    pub invite: Option<String>,
    #[serde(default)]
    pub webapp: Option<String>,
}

impl Endpoints {
    /// Base URL used for authenticated REST calls.
    pub fn api_base(&self) -> &str {
        if self.api.is_empty() {
            PUBLIC_DISCOVERY_ORIGIN
        } else {
            self.api.trim_end_matches('/')
        }
    }

    /// Base URL used for avatars, icons and banners.
    pub fn media_base(&self) -> &str {
        match self.media.as_deref() {
            Some(media) if !media.is_empty() => media.trim_end_matches('/'),
            _ => crate::media::DEFAULT_MEDIA,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Features {
    #[serde(default)]
    pub voice_enabled: bool,
    #[serde(default)]
    pub self_hosted: bool,
    #[serde(default)]
    pub discovery_enabled: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Branding {
    #[serde(default)]
    pub product_name: Option<String>,
    #[serde(default)]
    pub theme_color: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AppPublic {
    #[serde(default)]
    pub branding: Branding,
}

/// The subset of the discovery document the client actually needs.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Discovery {
    #[serde(default)]
    pub api_code_version: u32,
    #[serde(default)]
    pub endpoints: Endpoints,
    #[serde(default)]
    pub features: Features,
    #[serde(default)]
    pub app_public: AppPublic,
}

impl Discovery {
    pub fn product_name(&self) -> &str {
        self.app_public
            .branding
            .product_name
            .as_deref()
            .unwrap_or("Fluxer")
    }
}

/// Normalises user input (`fluxer.app`, `https://api.fluxer.app/`, ...) into an
/// origin suitable for `{origin}/.well-known/fluxer`.
pub fn normalize_origin(input: &str) -> Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(PUBLIC_DISCOVERY_ORIGIN.to_string());
    }
    let with_scheme = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let url = url::Url::parse(&with_scheme)?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(Error::InvalidInstance(with_scheme));
    }
    Ok(url.origin().ascii_serialization())
}

/// Fetches and validates the discovery document for an instance origin.
pub async fn discover(http: &reqwest::Client, origin: &str) -> Result<Discovery> {
    let base = normalize_origin(origin)?;
    let url = format!("{base}/.well-known/fluxer");
    let response = http
        .get(&url)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(Error::Discovery {
            url,
            status: response.status().as_u16(),
        });
    }

    let doc: Discovery = response.json().await?;
    if doc.endpoints.api.is_empty() || doc.endpoints.gateway.is_empty() {
        return Err(Error::DiscoveryUnusable(url));
    }
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_bare_domains() {
        assert_eq!(
            normalize_origin("fluxer.app").unwrap(),
            "https://fluxer.app"
        );
        assert_eq!(
            normalize_origin("https://api.fluxer.app/some/path").unwrap(),
            "https://api.fluxer.app"
        );
        assert_eq!(normalize_origin("").unwrap(), PUBLIC_DISCOVERY_ORIGIN);
    }
}
