// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! Connectivity probe: validates discovery, REST and the realtime gateway.
//!
//! ```sh
//! cargo run -p fluxer-core --example probe
//! FLUXER_TOKEN=... cargo run -p fluxer-core --example probe
//! ```
//!
//! Nothing sensitive is printed: only counts, identifiers lengths, event names.

use std::time::Duration;

use fluxer_core::gateway::{GatewayEvent, run_gateway};
use fluxer_core::{ApiClient, ChannelKind, discovery, session};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = match std::env::var("FLUXER_TOKEN") {
        Ok(token) => fluxer_core::Config {
            instance: std::env::var("FLUXER_INSTANCE")
                .unwrap_or_else(|_| fluxer_core::DEFAULT_INSTANCE.to_string()),
            token,
            ..Default::default()
        },
        Err(_) => session::import_from_installed_fluxer()?,
    };

    if config.token.is_empty() {
        eprintln!("aucun token: définissez FLUXER_TOKEN ou installez Fluxer");
        std::process::exit(1);
    }

    let http = reqwest::Client::new();
    let doc = discovery::discover(&http, &config.instance).await?;
    println!("discovery ok: {} endpoints", doc.endpoints.api_base());
    println!("gateway: {}", doc.endpoints.gateway);

    let api = ApiClient::new(&doc.endpoints, config.token.clone())?;
    let me = api.me().await?;
    println!("me ok: id_len={} name={}", me.id.len(), me.display_name());

    let guilds = api.guilds().await?;
    println!("guilds: {}", guilds.len());

    let mut channels_found = 0usize;
    let mut messages_found = 0usize;
    if let Some(guild) = guilds.first() {
        let channels = api.channels(&guild.id).await?;
        channels_found = channels.len();
        let readable: Vec<_> = channels.iter().filter(|c| c.kind().is_readable()).collect();
        println!(
            "channels: {} (lisibles: {}, catégories: {})",
            channels.len(),
            readable.len(),
            channels
                .iter()
                .filter(|c| c.kind() == ChannelKind::Category)
                .count()
        );
        if let Some(channel) = readable.first() {
            let messages = api.messages(&channel.id, 5, None).await?;
            messages_found = messages.len();
            println!("messages: {}", messages.len());
            if let Some(message) = messages.first() {
                println!("  clés: {} pièces jointes", message.attachments.len());
            }
        }
    }
    println!(
        "résumé: guilds={} channels={} messages={}",
        guilds.len(),
        channels_found,
        messages_found
    );

    // --- gateway -----------------------------------------------------------
    let (tx, mut rx) = mpsc::unbounded_channel();
    let (_outgoing_tx, outgoing_rx) = mpsc::unbounded_channel();
    let cancel = CancellationToken::new();
    let gateway_url = doc.endpoints.gateway.clone();
    let token = config.token.clone();
    let cancel_task = cancel.clone();
    tokio::spawn(async move {
        run_gateway(gateway_url, token, tx, outgoing_rx, cancel_task).await;
    });

    println!("gateway: connexion pendant 15s…");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => break,
            event = rx.recv() => {
                let Some(event) = event else { break };
                match event {
                    GatewayEvent::Connected => println!("  [gw] connecté"),
                    GatewayEvent::Hello { heartbeat_interval_ms } => println!("  [gw] HELLO (heartbeat {heartbeat_interval_ms}ms)"),
                    GatewayEvent::Ready { session_id, user } => println!(
                        "  [gw] READY session={} user={}",
                        session_id.is_some(),
                        user.map(|u| u.display_name().to_string()).unwrap_or_default()
                    ),
                    GatewayEvent::HeartbeatAck => println!("  [gw] heartbeat ack"),
                    GatewayEvent::Dispatch { event, data } => println!(
                        "  [gw] dispatch {event} ({} octets)",
                        data.to_string().len()
                    ),
                    GatewayEvent::Reconnecting { reason } => println!("  [gw] reconnexion: {reason}"),
                    GatewayEvent::Closed { code, reason } => println!("  [gw] fermé {code:?} {reason}"),
                    GatewayEvent::Fatal(reason) => {
                        println!("  [gw] fatal: {reason}");
                        break;
                    }
                }
            }
        }
    }
    cancel.cancel();
    tokio::time::sleep(Duration::from_millis(200)).await;
    println!("terminé");
    Ok(())
}
