// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! Realtime gateway client (`wss://gateway.fluxer.app`).
//!
//! Wire protocol (reverse engineered from the official client and validated
//! against the live gateway): JSON payloads `{op, d, s?, t?}`.
//!
//! | op  | name            | direction | meaning                                   |
//! |-----|-----------------|-----------|-------------------------------------------|
//! | 0   | DISPATCH        | S→C       | event, `t` = name, `s` = sequence         |
//! | 1   | HEARTBEAT       | C→S       | keep-alive carrying the last sequence     |
//! | 7   | RECONNECT       | S→C       | drop the socket, resume                   |
//! | 9   | INVALID_SESSION | S→C       | `d=true` resume, `d=false` re-identify     |
//! | 10  | HELLO           | S→C       | `d.heartbeat_interval` in milliseconds    |
//! | 11  | HEARTBEAT_ACK   | S→C       | previous heartbeat accepted               |
//!
//! Connection URL query: `?v=1&encoding=json&stream=1`.
//!
//! The official client additionally negotiates `compress=zstd-stream`, which
//! multiplexes a single zstd stream across WebSocket frames. The server happily
//! serves plain text JSON when the parameter is omitted, so the native client
//! uses the simpler, fully incremental text transport.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::time::{MissedTickBehavior, interval_at, sleep};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_util::sync::CancellationToken;

use crate::error::{Error, Result};
use crate::models::User;

pub const OP_DISPATCH: i64 = 0;
pub const OP_HEARTBEAT: i64 = 1;
pub const OP_IDENTIFY: i64 = 2;
pub const OP_RESUME: i64 = 6;
pub const OP_RECONNECT: i64 = 7;
pub const OP_INVALID_SESSION: i64 = 9;
pub const OP_HELLO: i64 = 10;
pub const OP_HEARTBEAT_ACK: i64 = 11;
/// Souscription paresseuse : listes de membres et présences ciblées.
pub const OP_LAZY_REQUEST: i64 = 14;

const DEFAULT_HEARTBEAT_MS: u64 = 41_250;

#[derive(Debug, Clone)]
// Le variant `Dispatch` transporte un `serde_json::Value` complet : le boxing
// apporterait peu pour un événement transitoire et alourdirait tous les sites.
#[allow(clippy::large_enum_variant)]
pub enum GatewayEvent {
    Connected,
    Hello {
        heartbeat_interval_ms: u64,
    },
    Ready {
        session_id: Option<String>,
        user: Option<User>,
    },
    Dispatch {
        event: String,
        data: Value,
    },
    HeartbeatAck,
    Reconnecting {
        reason: String,
    },
    Closed {
        code: Option<u16>,
        reason: String,
    },
    Fatal(String),
}

#[derive(Debug, Default, Clone)]
struct SessionState {
    session_id: Option<String>,
    seq: i64,
}

enum Outcome {
    Reconnect(String),
    Closed,
}

/// Runs the gateway forever (until `cancel` fires), reconnecting and resuming
/// automatically. Events are pushed to `events`; JSON payloads sent to
/// `outgoing` are written to the socket (op 14 subscriptions, presence, …).
pub async fn run_gateway(
    gateway_url: String,
    token: String,
    events: UnboundedSender<GatewayEvent>,
    mut outgoing: UnboundedReceiver<Value>,
    cancel: CancellationToken,
) {
    let mut session = SessionState::default();
    let mut attempt: u32 = 0;

    loop {
        if cancel.is_cancelled() {
            break;
        }

        let url = build_url(&gateway_url);
        match tokio_tungstenite::connect_async(&url).await {
            Ok((stream, _response)) => {
                attempt = 0;
                let _ = events.send(GatewayEvent::Connected);
                match session_loop(
                    stream,
                    &token,
                    &mut session,
                    &events,
                    &mut outgoing,
                    &cancel,
                )
                .await
                {
                    Ok(Outcome::Closed) => {
                        let _ = events.send(GatewayEvent::Closed {
                            code: None,
                            reason: "socket fermé".into(),
                        });
                    }
                    Ok(Outcome::Reconnect(reason)) => {
                        let _ = events.send(GatewayEvent::Reconnecting { reason });
                    }
                    Err(Error::Unauthorized) => {
                        let _ = events
                            .send(GatewayEvent::Fatal("token refusé par la passerelle".into()));
                        return;
                    }
                    Err(err) => {
                        let _ = events.send(GatewayEvent::Reconnecting {
                            reason: err.to_string(),
                        });
                    }
                }
            }
            Err(err) => {
                let _ = events.send(GatewayEvent::Reconnecting {
                    reason: format!("connexion impossible: {err}"),
                });
            }
        }

        let delay = backoff(attempt);
        attempt = attempt.saturating_add(1);
        tokio::select! {
            _ = sleep(delay) => {}
            _ = cancel.cancelled() => break,
        }
    }
}

fn backoff(attempt: u32) -> Duration {
    let base = 500u64 << attempt.min(5);
    let jitter = (now_nanos() % 400) as u64;
    Duration::from_millis(base + jitter)
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u128 + d.as_millis())
        .unwrap_or(0)
}

fn rand_fraction() -> f64 {
    (now_nanos() % 1_000_000) as f64 / 1_000_000.0
}

fn build_url(base: &str) -> String {
    match url::Url::parse(base) {
        Ok(mut url) => {
            url.query_pairs_mut()
                .append_pair("v", "1")
                .append_pair("encoding", "json")
                .append_pair("stream", "1");
            url.to_string()
        }
        Err(_) => base.to_string(),
    }
}

async fn session_loop<S>(
    stream: tokio_tungstenite::WebSocketStream<S>,
    token: &str,
    session: &mut SessionState,
    events: &UnboundedSender<GatewayEvent>,
    outgoing: &mut UnboundedReceiver<Value>,
    cancel: &CancellationToken,
) -> Result<Outcome>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (mut write, mut read) = stream.split();
    let mut heartbeat: Option<tokio::time::Interval> = None;
    let mut awaiting_ack = false;
    let mut identified = false;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                let _ = write.send(WsMessage::Close(None)).await;
                return Ok(Outcome::Closed);
            }
            Some(payload) = outgoing.recv(), if identified => {
                send_json(&mut write, &payload).await?;
            }
            _ = heartbeat_tick(&mut heartbeat) => {
                if awaiting_ack {
                    return Ok(Outcome::Reconnect(
                        "aucun HEARTBEAT_ACK reçu".into(),
                    ));
                }
                let payload = json!({ "op": OP_HEARTBEAT, "d": session.seq });
                send_json(&mut write, &payload).await?;
                awaiting_ack = true;
            }
            frame = read.next() => {
                let Some(frame) = frame else {
                    return Ok(Outcome::Reconnect("flux fermé".into()));
                };
                let frame = frame?;
                match frame {
                    WsMessage::Text(text) => {
                        if let Some(outcome) = handle_payload(
                            text.as_str(),
                            token,
                            session,
                            events,
                            &mut write,
                            &mut heartbeat,
                            &mut awaiting_ack,
                            &mut identified,
                        ).await? {
                            return Ok(outcome);
                        }
                    }
                    WsMessage::Binary(_) => {
                        // Without `compress=zstd-stream` the gateway never sends
                        // binary frames; ignore them defensively.
                    }
                    WsMessage::Ping(data) => {
                        let _ = write.send(WsMessage::Pong(data)).await;
                    }
                    WsMessage::Close(frame) => {
                        let code = frame.as_ref().map(|f| u16::from(f.code));
                        let reason = frame
                            .as_ref()
                            .map(|f| f.reason.to_string())
                            .unwrap_or_default();
                        let _ = events.send(GatewayEvent::Closed { code, reason });
                        if matches!(code, Some(4004) | Some(4010) | Some(4011) | Some(4012) | Some(4013) | Some(4014)) {
                            return Err(Error::Unauthorized);
                        }
                        return Ok(Outcome::Reconnect(
                            format!("fermeture {code:?}"),
                        ));
                    }
                    _ => {}
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_payload<S>(
    text: &str,
    token: &str,
    session: &mut SessionState,
    events: &UnboundedSender<GatewayEvent>,
    write: &mut futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<S>, WsMessage>,
    heartbeat: &mut Option<tokio::time::Interval>,
    awaiting_ack: &mut bool,
    identified: &mut bool,
) -> Result<Option<Outcome>>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let payload: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };

    let op = payload.get("op").and_then(Value::as_i64).unwrap_or(-1);
    let data = payload.get("d").cloned().unwrap_or(Value::Null);
    if let Some(seq) = payload.get("s").and_then(Value::as_i64) {
        session.seq = seq;
    }

    match op {
        OP_HELLO => {
            let interval_ms = data
                .get("heartbeat_interval")
                .and_then(Value::as_u64)
                .unwrap_or(DEFAULT_HEARTBEAT_MS)
                .max(1_000);
            let period = Duration::from_millis(interval_ms);
            let first = period.mul_f64(rand_fraction());
            let mut ticker = interval_at(tokio::time::Instant::now() + first, period);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            *heartbeat = Some(ticker);
            let _ = events.send(GatewayEvent::Hello {
                heartbeat_interval_ms: interval_ms,
            });

            if session.session_id.is_some() {
                let payload = json!({
                    "op": OP_RESUME,
                    "d": {
                        "token": token,
                        "session_id": session.session_id,
                        "seq": session.seq,
                    }
                });
                send_json(write, &payload).await?;
            } else {
                send_identify(write, token).await?;
            }
            *identified = true;
        }
        OP_HEARTBEAT => {
            let payload = json!({ "op": OP_HEARTBEAT, "d": session.seq });
            send_json(write, &payload).await?;
        }
        OP_HEARTBEAT_ACK => {
            *awaiting_ack = false;
            let _ = events.send(GatewayEvent::HeartbeatAck);
        }
        OP_RECONNECT => {
            return Ok(Some(Outcome::Reconnect("reconnexion demandée".into())));
        }
        OP_INVALID_SESSION => {
            let resumable = data.as_bool().unwrap_or(false);
            if !resumable {
                session.session_id = None;
                session.seq = 0;
            }
            let payload = json!({
                "op": OP_RESUME,
                "d": {
                    "token": token,
                    "session_id": session.session_id,
                    "seq": session.seq,
                }
            });
            if resumable && session.session_id.is_some() {
                send_json(write, &payload).await?;
            } else {
                send_identify(write, token).await?;
            }
            *identified = true;
        }
        OP_DISPATCH => {
            let event = payload
                .get("t")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();

            if event == "READY" {
                if let Some(session_id) = data.get("session_id").and_then(Value::as_str) {
                    session.session_id = Some(session_id.to_string());
                }
                let user = data
                    .get("user")
                    .cloned()
                    .and_then(|value| serde_json::from_value::<User>(value).ok());
                let _ = events.send(GatewayEvent::Ready {
                    session_id: session.session_id.clone(),
                    user,
                });
            }

            let _ = events.send(GatewayEvent::Dispatch { event, data });
        }
        _ => {}
    }

    let _ = identified;
    Ok(None)
}

async fn send_identify<S>(
    write: &mut futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<S>, WsMessage>,
    token: &str,
) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let payload = json!({
        "op": OP_IDENTIFY,
        "d": {
            "token": token,
            "capabilities": 0,
            "properties": {
                "os": std::env::consts::OS,
                "browser": "fluxer-rs",
                "device": "fluxer-rs",
            },
            "presence": { "status": "online" },
        }
    });
    send_json(write, &payload).await
}

async fn send_json<S>(
    write: &mut futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<S>, WsMessage>,
    value: &Value,
) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    write
        .send(WsMessage::Text(value.to_string().into()))
        .await?;
    Ok(())
}

async fn heartbeat_tick(heartbeat: &mut Option<tokio::time::Interval>) {
    match heartbeat {
        Some(ticker) => {
            ticker.tick().await;
        }
        None => std::future::pending::<()>().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_carries_protocol_parameters() {
        let url = build_url("wss://gateway.fluxer.app");
        assert!(url.starts_with("wss://gateway.fluxer.app/?"));
        assert!(url.contains("v=1"));
        assert!(url.contains("encoding=json"));
        assert!(!url.contains("compress="));
    }

    #[test]
    fn backoff_grows_then_caps() {
        assert!(backoff(0) < backoff(3));
        assert!(backoff(10) < Duration::from_secs(40));
    }
}
