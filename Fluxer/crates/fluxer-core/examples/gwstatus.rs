// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

use fluxer_core::gateway::{GatewayEvent, run_gateway};
use fluxer_core::{discovery, session};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = session::import_from_installed_fluxer()?;
    let http = reqwest::Client::new();
    let doc = discovery::discover(&http, &config.instance).await?;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let (_outgoing_tx, outgoing_rx) = mpsc::unbounded_channel();
    let cancel = CancellationToken::new();
    let url = doc.endpoints.gateway.clone();
    let token = config.token.clone();
    let c2 = cancel.clone();
    tokio::spawn(async move { run_gateway(url, token, tx, outgoing_rx, c2).await });
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut counts = std::collections::BTreeMap::new();
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => break,
            ev = rx.recv() => {
                let Some(ev) = ev else { break };
                if let GatewayEvent::Dispatch { event, data } = ev {
                    if event == "PRESENCE_UPDATE" {
                        let st = data.get("status").and_then(|s| s.as_str()).unwrap_or("?");
                        *counts.entry(st.to_string()).or_insert(0usize) += 1;
                    }
                    if event == "READY" {
                        for g in data.get("guilds").and_then(|v| v.as_array()).into_iter().flatten() {
                            if let Some(p) = g.get("presences").and_then(|v| v.as_array()) {
                                for e in p {
                                    let st = e.get("status").and_then(|s| s.as_str()).unwrap_or("?");
                                    *counts.entry(format!("ready:{st}")).or_insert(0usize) += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    cancel.cancel();
    println!("{counts:?}");
    Ok(())
}
