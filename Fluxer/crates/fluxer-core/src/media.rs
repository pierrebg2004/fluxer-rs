// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! CDN helpers for the media endpoint advertised by discovery.
//!
//! Verified layout on the public instance:
//!   {media}/avatars/{user_id}/{hash}.webp
//!   {media}/icons/{guild_id}/{hash}.webp
//!   {media}/banners/{guild_id}/{hash}.webp

pub const DEFAULT_MEDIA: &str = "https://fluxerusercontent.com";

pub fn avatar_url(media: &str, user_id: &str, hash: &str) -> String {
    format!("{}/avatars/{}/{}.webp", base(media), user_id, hash)
}

pub fn guild_icon_url(media: &str, guild_id: &str, hash: &str) -> String {
    format!("{}/icons/{}/{}.webp", base(media), guild_id, hash)
}

pub fn guild_banner_url(media: &str, guild_id: &str, hash: &str) -> String {
    format!("{}/banners/{}/{}.webp", base(media), guild_id, hash)
}

/// Bannière réduite utilisée en tête des cartes d'invitation.
pub fn guild_embed_splash_url(media: &str, guild_id: &str, hash: &str) -> String {
    format!(
        "{}/embed-splashes/{}/{}.webp",
        base(media),
        guild_id,
        hash
    )
}

/// Attachment URLs are absolute on the wire; this only normalises the base.
pub fn absolute(media: &str, path_or_url: &str) -> String {
    if path_or_url.starts_with("http://") || path_or_url.starts_with("https://") {
        path_or_url.to_string()
    } else {
        format!("{}/{}", base(media), path_or_url.trim_start_matches('/'))
    }
}

fn base(media: &str) -> &str {
    if media.is_empty() {
        DEFAULT_MEDIA
    } else {
        media.trim_end_matches('/')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_avatar_url() {
        assert_eq!(
            avatar_url("https://fluxerusercontent.com/", "42", "abc"),
            "https://fluxerusercontent.com/avatars/42/abc.webp"
        );
    }

    #[test]
    fn falls_back_to_public_media() {
        assert_eq!(
            guild_icon_url("", "1", "dead"),
            "https://fluxerusercontent.com/icons/1/dead.webp"
        );
    }
}
