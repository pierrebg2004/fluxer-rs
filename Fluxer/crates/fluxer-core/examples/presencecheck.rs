// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! Correlates live PRESENCE_UPDATE user ids with the members of the first
//! guild, to validate how presence is scoped server-side.

use std::collections::HashSet;
use std::time::Duration;

use fluxer_core::gateway::{GatewayEvent, run_gateway};
use fluxer_core::{ApiClient, discovery, session};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = session::import_from_installed_fluxer()?;
    let http = reqwest::Client::new();
    let doc = discovery::discover(&http, &config.instance).await?;
    let api = ApiClient::new(&doc.endpoints, config.token.clone())?;
    let guilds = api.guilds().await?;
    let guild = guilds.first().ok_or("no guild")?;
    println!("guild: {} {}", guild.name, guild.id);
    let members = api.members(&guild.id, 200).await?;
    let ids: HashSet<String> = members.iter().map(|m| m.user.id.clone()).collect();
    println!("members: {}", ids.len());

    let (tx, mut rx) = mpsc::unbounded_channel();
    let (_outgoing_tx, outgoing_rx) = mpsc::unbounded_channel();
    let cancel = CancellationToken::new();
    let url = doc.endpoints.gateway.clone();
    let token = config.token.clone();
    let c2 = cancel.clone();
    tokio::spawn(async move { run_gateway(url, token, tx, outgoing_rx, c2).await });

    let mut seen: HashSet<String> = HashSet::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => break,
            ev = rx.recv() => {
                let Some(ev) = ev else { break };
                if let GatewayEvent::Dispatch { event, data } = ev
                    && event == "PRESENCE_UPDATE"
                    && let Some(id) = data.get("user").and_then(|u| u.get("id")).and_then(|i| i.as_str())
                {
                    seen.insert(id.to_string());
                }
            }
        }
    }
    cancel.cancel();
    let hit = seen.intersection(&ids).count();
    println!("presence ids: {} ; in guild members: {}", seen.len(), hit);
    Ok(())
}
