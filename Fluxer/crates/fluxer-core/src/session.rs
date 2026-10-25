// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! Session persistence and one-click import from the installed Fluxer desktop app.
//!
//! The official desktop client stores each signed-in account in
//! `~/.config/fluxer/desktop-app-store.sqlite3` (table `accounts`, column
//! `record_json`). Electron's cookie encryption is disabled in that build, so the
//! token is plain JSON — we duplicate the database before reading it so we never
//! touch the live, locked file.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::discovery::PUBLIC_DISCOVERY_ORIGIN;
use crate::error::{Error, Result};

/// Message kept in the local « favoris » list.
///
/// The official client reserves the word « favoris » for GIFs and calls; this
/// native client also lets the user bookmark any message. Everything is stored
/// in the session file so the list survives restarts without a server round
/// trip.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SavedMessage {
    #[serde(default)]
    pub message_id: String,
    #[serde(default)]
    pub channel_id: String,
    /// Human readable channel name at the time of saving.
    #[serde(default)]
    pub channel_label: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Origin used for `/.well-known/fluxer` discovery.
    #[serde(default = "default_instance")]
    pub instance: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub user_id: Option<String>,
    /// Presentation preference persisted by the GUI.
    #[serde(default)]
    pub compact: bool,
    #[serde(default = "default_true")]
    pub show_members: bool,
    #[serde(default = "default_true")]
    pub enable_favorites: bool,
    #[serde(default)]
    pub favorites: Vec<SavedMessage>,
    #[serde(default = "default_true")]
    pub notifications: bool,
    #[serde(default = "default_true")]
    pub notification_sounds: bool,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f32,
    #[serde(default)]
    pub reduce_motion: bool,
    /// Microphone muted by default when joining a voice channel.
    #[serde(default)]
    pub voice_muted: bool,
    /// Headphones (output) muted by default.
    #[serde(default)]
    pub voice_deafened: bool,
}

fn default_instance() -> String {
    PUBLIC_DISCOVERY_ORIGIN.to_string()
}

fn default_true() -> bool {
    true
}

fn default_language() -> String {
    "fr".to_string()
}

fn default_ui_scale() -> f32 {
    1.0
}

impl Default for Config {
    fn default() -> Self {
        Self {
            instance: default_instance(),
            token: String::new(),
            user_id: None,
            compact: false,
            show_members: true,
            enable_favorites: true,
            favorites: Vec::new(),
            notifications: true,
            notification_sounds: true,
            language: default_language(),
            ui_scale: default_ui_scale(),
            reduce_motion: false,
            voice_muted: false,
            voice_deafened: false,
        }
    }
}

impl Config {
    pub fn is_authenticated(&self) -> bool {
        !self.token.is_empty()
    }

    /// Adds or removes a message from the favorites list. Returns `true` when
    /// the message is now saved.
    pub fn toggle_favorite(&mut self, message: &SavedMessage) -> bool {
        if let Some(index) = self
            .favorites
            .iter()
            .position(|saved| saved.message_id == message.message_id)
        {
            self.favorites.remove(index);
            false
        } else {
            self.favorites.insert(0, message.clone());
            true
        }
    }

    pub fn is_favorite(&self, message_id: &str) -> bool {
        self.favorites
            .iter()
            .any(|saved| saved.message_id == message_id)
    }
}

pub fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME")
        && !dir.is_empty()
    {
        return PathBuf::from(dir).join("fluxer-rs");
    }
    home().join(".config").join("fluxer-rs")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

pub fn home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

pub fn load() -> Config {
    let Ok(raw) = fs::read_to_string(config_path()) else {
        return Config {
            instance: default_instance(),
            ..Config::default()
        };
    };
    serde_json::from_str(&raw).unwrap_or_else(|_| Config {
        instance: default_instance(),
        ..Config::default()
    })
}

pub fn save(config: &Config) -> Result<()> {
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| Error::Storage(e.to_string()))?;
    let raw = serde_json::to_string_pretty(config)?;
    let path = dir.join("config.json");
    fs::write(&path, raw).map_err(|e| Error::Storage(e.to_string()))?;
    restrict_permissions(&path);
    Ok(())
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) {}

/// Absolute path of the official client's account database.
pub fn installed_fluxer_db() -> PathBuf {
    home()
        .join(".config")
        .join("fluxer")
        .join("desktop-app-store.sqlite3")
}

/// Imports the most recently used account from the official desktop client.
pub fn import_from_installed_fluxer() -> Result<Config> {
    let db = installed_fluxer_db();
    if !db.exists() {
        return Err(Error::NoInstalledSession);
    }

    // Copy first: the live database is opened with a lock by the running app.
    let temp = std::env::temp_dir().join(format!(
        "fluxer-rs-import-{}-{}.sqlite3",
        std::process::id(),
        chrono::Utc::now().timestamp()
    ));
    fs::copy(&db, &temp).map_err(|e| Error::Storage(e.to_string()))?;

    let result = read_account(&temp);
    let _ = fs::remove_file(&temp);
    result
}

fn read_account(db: &Path) -> Result<Config> {
    let conn = rusqlite::Connection::open_with_flags(
        db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| Error::Storage(e.to_string()))?;

    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT storage_key, record_json FROM accounts ORDER BY last_active DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();

    let (storage_key, record_json) = row.ok_or(Error::NoInstalledSession)?;
    let record: serde_json::Value =
        serde_json::from_str(&record_json).map_err(|e| Error::Storage(e.to_string()))?;

    let token = record
        .get("token")
        .and_then(|v| v.as_str())
        .ok_or(Error::NoInstalledSession)?
        .to_string();
    let user_id = record
        .get("userId")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let instance = instance_from_storage_key(&storage_key);
    Ok(Config {
        instance,
        token,
        user_id,
        ..Config::default()
    })
}

/// `storage_key` looks like `https://web.fluxer.app/api::1472687146114043966`.
fn instance_from_storage_key(key: &str) -> String {
    let api_base = key.split("::").next().unwrap_or(key);
    let Ok(url) = url::Url::parse(api_base) else {
        return default_instance();
    };
    match url.host_str() {
        Some("web.fluxer.app") => PUBLIC_DISCOVERY_ORIGIN.to_string(),
        Some(host) if host.starts_with("web.") => {
            let api_host = host.replacen("web.", "api.", 1);
            format!("{}://{}", url.scheme(), api_host)
        }
        Some(_) => url.origin().ascii_serialization(),
        None => default_instance(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_official_web_base_to_public_discovery_origin() {
        assert_eq!(
            instance_from_storage_key("https://web.fluxer.app/api::123"),
            "https://api.fluxer.app"
        );
    }

    #[test]
    fn maps_self_hosted_web_base_to_api_host() {
        assert_eq!(
            instance_from_storage_key("https://web.example.org/api::1"),
            "https://api.example.org"
        );
    }
}
