// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! `fluxer-core` — a native Rust implementation of the Fluxer client protocol.
//!
//! No JavaScript, no Electron, no WebView: plain Rust talking HTTP and WebSocket.
//!
//! ```no_run
//! # async fn demo() -> fluxer_core::Result<()> {
//! let http = reqwest::Client::new();
//! let discovery = fluxer_core::discovery::discover(&http, "https://api.fluxer.app").await?;
//! let api = fluxer_core::api::ApiClient::new(&discovery.endpoints, std::env::var("FLUXER_TOKEN").unwrap())?;
//! let me = api.me().await?;
//! println!("connecté en tant que {}", me.display_name());
//! # Ok(()) }
//! ```

pub mod api;
pub mod discovery;
pub mod error;
pub mod gateway;
pub mod media;
pub mod models;
pub mod session;

pub use api::ApiClient;
pub use discovery::{Discovery, Endpoints};
pub use error::{Error, Result};
pub use models::{
    Attachment, Channel, ChannelKind, DiscoveryGuild, Embed, EmbedAuthor, EmbedField,
    EmbedFooter, EmbedMedia, Emoji, Guild, GuildDetail, InviteChannel, InviteGuild, InvitePreview,
    Member, Message, Reaction, Role, User,
};
pub use session::Config;

/// Default public instance origin (discovery endpoint).
pub const DEFAULT_INSTANCE: &str = discovery::PUBLIC_DISCOVERY_ORIGIN;
