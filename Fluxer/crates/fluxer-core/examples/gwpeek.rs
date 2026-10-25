// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! Prints the shape of gateway dispatches (event names + JSON keys only).
//!
//! ```sh
//! cargo run -p fluxer-core --example gwpeek
//! ```

use std::time::Duration;

use fluxer_core::gateway::{GatewayEvent, run_gateway};
use fluxer_core::{discovery, session};
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
    let cancel_task = cancel.clone();
    tokio::spawn(async move { run_gateway(url, token, tx, outgoing_rx, cancel_task).await });

    let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => break,
            event = rx.recv() => {
                let Some(event) = event else { break };
                match event {
                    GatewayEvent::Ready { .. } => println!("READY"),
                    GatewayEvent::Dispatch { event, data } => {
                        let keys: Vec<String> = data.as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default();
                        println!("{event} keys={keys:?}");
                        if event == "READY"
                            && let Some(guilds) = data.get("guilds").and_then(|g| g.as_array())
                        {
                                println!("  guilds={} first_keys={:?}", guilds.len(), guilds.first().and_then(|g| g.as_object()).map(|o| o.keys().cloned().collect::<Vec<_>>()));
                                if let Some(p) = guilds.first().and_then(|g| g.get("presences")) {
                                    println!("  presences={}", p.as_array().map(|a| a.len()).unwrap_or(0));
                                }
                                if let Some(m) = guilds.first().and_then(|g| g.get("members")) {
                                    println!("  members={}", m.as_array().map(|a| a.len()).unwrap_or(0));
                                }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    cancel.cancel();
    Ok(())
}
