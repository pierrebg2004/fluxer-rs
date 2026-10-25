// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! Client LiveKit (SFU) natif.
//!
//! Fluxer utilise un serveur LiveKit pour la voix de guilde : la passerelle
//! envoie `VOICE_SERVER_UPDATE` avec une URL et un JWT, puis toute la
//! négociation passe par le WebSocket de signalisation LiveKit.
//!
//! Le mode « double PeerConnection » (`/rtc` v0) est utilisé : le client est
//! l'offreur de la connexion *publisher* (son micro) et le répondeur de la
//! connexion *subscriber* (l'audio des autres participants). Aucune
//! dépendance C n'est nécessaire, `livekit-signaling` s'appuie sur le même
//! `webrtc` que le maillage pair-à-pair.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::{Duration, Instant};

use livekit_protocol as proto;
use livekit_signaling::{SignalClient, SignalEvent, SignalOptions};
use rtc::interceptor::Registry;
use rtc::media::Sample;
use rtc::media_stream::MediaStreamTrack;
use rtc::peer_connection::configuration::interceptor_registry::register_default_interceptors;
use rtc::peer_connection::configuration::media_engine::{MIME_TYPE_OPUS, MediaEngine};
use rtc::peer_connection::configuration::{RTCBundlePolicy, RTCConfigurationBuilder};
use rtc::peer_connection::sdp::RTCSessionDescription;
use rtc::peer_connection::transport::RTCIceServer as RtcIceServer;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodingParameters, RTCRtpEncodingParameters, RtpCodecKind,
};
use rtc::rtp_transceiver::{RTCRtpTransceiverDirection, RTCRtpTransceiverInit};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tracing::{debug, info, warn};

use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_local::static_sample::TrackLocalStaticSample;
use webrtc::media_stream::track_remote::TrackRemoteEvent;
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceCandidateInit,
    RTCIceGatheringState, RTCPeerConnectionIceEvent, RTCPeerConnectionState,
};

use crate::audio::{AudioIo, decoder};

/// Nom de piste attendu par les clients officiels.
const MICROPHONE: &str = "microphone";

/// Paramètres d'une session SFU.
#[derive(Debug, Clone)]
pub struct SfuConfig {
    pub endpoint: String,
    pub token: String,
    pub guild_id: String,
    pub channel_id: String,
    pub muted: bool,
    pub deafened: bool,
}

/// Événement remonté par la session SFU.
#[derive(Debug, Clone)]
pub enum SfuEvent {
    Connected,
    Disconnected,
    AudioUnavailable(String),
    Error(String),
}

/// Poignée clonable vers l'acteur SFU.
#[derive(Clone)]
pub struct Sfu {
    inbox: UnboundedSender<Inner>,
}

impl Sfu {
    /// Ouvre la session LiveKit et l'audio natif. À appeler depuis un contexte
    /// Tokio.
    pub fn start(config: SfuConfig) -> Result<(Self, UnboundedReceiver<SfuEvent>), String> {
        if config.endpoint.is_empty() || config.token.is_empty() {
            return Err("session vocale incomplète".into());
        }
        let (inbox, commands) = unbounded_channel();
        let (events, event_rx) = unbounded_channel();
        let notify = inbox.clone();
        tokio::spawn(async move {
            run(config, commands, notify, events).await;
        });
        Ok((Self { inbox }, event_rx))
    }

    pub fn set_muted(&self, muted: bool) {
        let _ = self.inbox.send(Inner::Muted(muted));
    }

    pub fn set_deafened(&self, deafened: bool) {
        let _ = self.inbox.send(Inner::Deafened(deafened));
    }

    pub fn stop(&self) {
        let _ = self.inbox.send(Inner::Shutdown);
    }
}

enum Inner {
    Muted(bool),
    Deafened(bool),
    PublisherConnected,
    SubscriberConnected,
    Shutdown,
}

async fn run(
    config: SfuConfig,
    mut inbox: UnboundedReceiver<Inner>,
    notify: UnboundedSender<Inner>,
    events: UnboundedSender<SfuEvent>,
) {
    let (audio, mut mic_rx, audio_error) = AudioIo::start(config.muted, config.deafened);
    if !audio.available {
        let message = if audio_error.is_empty() {
            "aucun périphérique audio disponible".to_string()
        } else {
            audio_error
        };
        let _ = events.send(SfuEvent::AudioUnavailable(message));
    }
    let audio = Arc::new(audio);

    let mut options = SignalOptions::default();
    options.auto_subscribe = true;
    // Mode double PeerConnection : compatible avec tous les déploiements
    // LiveKit et avec la version de protocole utilisée par Fluxer.
    options.single_peer_connection = false;

    let (client, join, mut signals) =
        match SignalClient::connect(&config.endpoint, &config.token, options, None).await {
            Ok(connected) => connected,
            Err(error) => {
                let _ = events.send(SfuEvent::Error(format!(
                    "connexion vocale impossible : {error}"
                )));
                return;
            }
        };
    let client = Arc::new(client);
    let identity = join
        .participant
        .as_ref()
        .map(|participant| participant.identity.clone())
        .filter(|identity| !identity.is_empty())
        .unwrap_or_else(|| "fluxer".to_string());
    let ice_servers: Vec<RtcIceServer> = join
        .ice_servers
        .iter()
        .map(|server| RtcIceServer {
            urls: server.urls.clone(),
            username: server.username.clone(),
            credential: server.credential.clone(),
        })
        .collect();

    info!(
        "session SFU ouverte (guilde {}, salon {}, {} serveur(s) ICE)",
        config.guild_id,
        config.channel_id,
        ice_servers.len()
    );

    let ssrc = track_ssrc(&identity);
    let mic = match build_mic(&identity, ssrc) {
        Ok(mic) => mic,
        Err(error) => {
            let _ = events.send(SfuEvent::Error(format!("piste micro : {error}")));
            let _ = client.close().await;
            return;
        }
    };

    // --- connexion publisher : le client offre, le SFU répond -------------
    let publisher = match build_pc(
        &ice_servers,
        Arc::new(Handler {
            client: Arc::clone(&client),
            target: proto::SignalTarget::Publisher as i32,
            notify: notify.clone(),
        }),
    )
    .await
    {
        Ok(pc) => pc,
        Err(error) => {
            let _ = events.send(SfuEvent::Error(format!("création WebRTC : {error}")));
            let _ = client.close().await;
            return;
        }
    };

    let init = RTCRtpTransceiverInit {
        direction: RTCRtpTransceiverDirection::Sendonly,
        streams: Vec::new(),
        send_encodings: Vec::new(),
    };
    if let Err(error) = publisher
        .add_transceiver_from_track(Arc::clone(&mic) as Arc<dyn TrackLocal>, Some(init))
        .await
    {
        let _ = events.send(SfuEvent::Error(format!("transceiver micro : {error}")));
        let _ = client.close().await;
        return;
    }

    let cid = format!("TR_{identity}_microphone");
    client
        .send(proto::signal_request::Message::AddTrack(
            proto::AddTrackRequest {
                cid,
                name: MICROPHONE.to_string(),
                r#type: proto::TrackType::Audio as i32,
                source: proto::TrackSource::Microphone as i32,
                muted: config.muted,
                stream: format!("PA_{identity}"),
                ..Default::default()
            },
        ))
        .await;

    let offer = match publisher.create_offer(None).await {
        Ok(offer) => offer,
        Err(error) => {
            let _ = events.send(SfuEvent::Error(format!("offre SDP : {error}")));
            let _ = client.close().await;
            return;
        }
    };
    if let Err(error) = publisher.set_local_description(offer.clone()).await {
        let _ = events.send(SfuEvent::Error(format!("description locale : {error}")));
        let _ = client.close().await;
        return;
    }
    client
        .send(proto::signal_request::Message::Offer(
            proto::SessionDescription {
                r#type: "offer".to_string(),
                sdp: offer.sdp,
                id: 0,
                mid_to_track_id: Default::default(),
            },
        ))
        .await;

    let mut subscriber: Option<Arc<dyn PeerConnection>> = None;
    let mut started: HashSet<String> = HashSet::new();
    let mut published_sid: Option<String> = None;
    let payload_type = Arc::new(AtomicU8::new(0));
    let publisher_ready = Arc::new(AtomicBool::new(false));
    let subscriber_ready = Arc::new(AtomicBool::new(false));
    let mut reported_connected = false;

    loop {
        tokio::select! {
            command = inbox.recv() => {
                let Some(command) = command else { break };
                match command {
                    Inner::Shutdown => break,
                    Inner::Muted(value) => {
                        audio.set_muted(value);
                        if let Some(sid) = published_sid.as_ref() {
                            client.send(proto::signal_request::Message::Mute(
                                proto::MuteTrackRequest { sid: sid.clone(), muted: value },
                            )).await;
                        }
                    }
                    Inner::Deafened(value) => audio.set_deafened(value),
                    Inner::PublisherConnected => {
                        publisher_ready.store(true, Ordering::Relaxed);
                    }
                    Inner::SubscriberConnected => {
                        subscriber_ready.store(true, Ordering::Relaxed);
                    }
                }
            }
            packet = mic_rx.recv(), if payload_type.load(Ordering::Relaxed) != 0 => {
                let Some(packet) = packet else { continue };
                let sample = Sample {
                    data: packet.into(),
                    duration: Duration::from_millis(20),
                    ..Sample::new(Instant::now())
                };
                if let Err(error) = mic
                    .write_sample(ssrc, payload_type.load(Ordering::Relaxed), &sample, &[])
                    .await
                {
                    debug!("envoi audio SFU : {error}");
                }
            }
            event = signals.recv() => {
                let Some(event) = event else {
                    let _ = events.send(SfuEvent::Disconnected);
                    break;
                };
                match event {
                    SignalEvent::Close(reason) => {
                        warn!("signalisation vocale fermée : {reason}");
                        let _ = events.send(SfuEvent::Disconnected);
                        break;
                    }
                    SignalEvent::Message(message) => match *message {
                        proto::signal_response::Message::Answer(answer) => {
                            let Ok(description) = RTCSessionDescription::answer(answer.sdp.clone()) else {
                                continue;
                            };
                            if let Err(error) = publisher.set_remote_description(description).await {
                                warn!("réponse SFU : {error}");
                                continue;
                            }
                            if let Some(payload) = opus_payload_type(&answer.sdp) {
                                payload_type.store(payload, Ordering::Relaxed);
                            } else {
                                configure_sender(&publisher, &payload_type).await;
                            }
                        }
                        proto::signal_response::Message::Offer(offer) => {
                            let pc = match subscriber.as_ref() {
                                Some(pc) => Arc::clone(pc),
                                None => match build_pc(
                                    &ice_servers,
                                    Arc::new(Handler {
                                        client: Arc::clone(&client),
                                        target: proto::SignalTarget::Subscriber as i32,
                                        notify: notify.clone(),
                                    }),
                                ).await {
                                    Ok(pc) => {
                                        subscriber = Some(Arc::clone(&pc));
                                        pc
                                    }
                                    Err(error) => {
                                        let _ = events.send(SfuEvent::Error(format!(
                                            "création WebRTC abonné : {error}"
                                        )));
                                        continue;
                                    }
                                },
                            };
                            let Ok(description) = RTCSessionDescription::offer(offer.sdp) else {
                                continue;
                            };
                            if let Err(error) = pc.set_remote_description(description).await {
                                warn!("offre SFU : {error}");
                                continue;
                            }
                            let answer = match pc.create_answer(None).await {
                                Ok(answer) => answer,
                                Err(error) => {
                                    warn!("réponse abonné : {error}");
                                    continue;
                                }
                            };
                            if let Err(error) = pc.set_local_description(answer.clone()).await {
                                warn!("description locale abonné : {error}");
                                continue;
                            }
                            client.send(proto::signal_request::Message::Answer(
                                proto::SessionDescription {
                                    r#type: "answer".to_string(),
                                    sdp: answer.sdp,
                                    id: 0,
                                    mid_to_track_id: Default::default(),
                                },
                            )).await;
                            start_receivers(&pc, &mut started, &audio).await;
                        }
                        proto::signal_response::Message::Trickle(trickle) => {
                            let Some(candidate) = parse_candidate(&trickle.candidate_init) else {
                                continue;
                            };
                            let target = trickle.target();
                            let pc = if target == proto::SignalTarget::Publisher {
                                Some(Arc::clone(&publisher))
                            } else {
                                subscriber.as_ref().map(Arc::clone)
                            };
                            if let Some(pc) = pc
                                && let Err(error) = pc.add_ice_candidate(candidate).await
                            {
                                debug!("candidat ICE SFU ignoré : {error}");
                            }
                        }
                        proto::signal_response::Message::TrackPublished(response) => {
                            if let Some(track) = response.track {
                                published_sid = Some(track.sid);
                            }
                        }
                        proto::signal_response::Message::Leave(leave) => {
                            debug!("le SFU demande de quitter le salon : {:?}", leave.reason());
                            let _ = events.send(SfuEvent::Disconnected);
                            break;
                        }
                        proto::signal_response::Message::RefreshToken(token) => {
                            // Le client de signalisation conserve déjà le jeton
                            // pour ses propres reprises ; rien d'autre à faire.
                            let _ = token;
                        }
                        _ => {}
                    },
                }
            }
        }

        if !reported_connected
            && (publisher_ready.load(Ordering::Relaxed) || subscriber_ready.load(Ordering::Relaxed))
            && payload_type.load(Ordering::Relaxed) != 0
        {
            reported_connected = true;
            let _ = events.send(SfuEvent::Connected);
        }
    }

    if let Some(subscriber) = subscriber {
        let _ = subscriber.close().await;
    }
    let _ = publisher.close().await;
    let _ = client.close().await;
    info!("session SFU fermée");
}

/// Construit la connexion WebRTC et son moteur de codecs.
async fn build_pc(
    ice_servers: &[RtcIceServer],
    handler: Arc<dyn PeerConnectionEventHandler>,
) -> Result<Arc<dyn PeerConnection>, String> {
    let mut media_engine = MediaEngine::default();
    media_engine
        .register_default_codecs()
        .map_err(|error| error.to_string())?;
    let registry = register_default_interceptors(Registry::new(), &mut media_engine)
        .map_err(|error| error.to_string())?;
    let configuration = RTCConfigurationBuilder::new()
        .with_ice_servers(ice_servers.to_vec())
        .with_bundle_policy(RTCBundlePolicy::MaxBundle)
        .build();

    let connection = PeerConnectionBuilder::new()
        .with_configuration(configuration)
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .with_handler(handler)
        .with_udp_addrs(vec!["0.0.0.0:0".to_string()])
        .build()
        .await
        .map_err(|error| error.to_string())?;
    Ok(Arc::new(connection))
}

/// Retrouve le type de charge Opus négocié dans une description SDP.
fn opus_payload_type(sdp: &str) -> Option<u8> {
    sdp.lines().find_map(|line| {
        let line = line.trim();
        let rest = line.strip_prefix("a=rtpmap:")?;
        let (payload, codec) = rest.split_once(' ')?;
        codec
            .to_ascii_lowercase()
            .starts_with("opus/")
            .then(|| payload.parse().ok())
            .flatten()
    })
}

/// Récupère le type de charge depuis les paramètres du sender si l'analyse du
/// SDP n'a rien donné.
async fn configure_sender(pc: &Arc<dyn PeerConnection>, payload_type: &Arc<AtomicU8>) {
    for transceiver in pc.get_transceivers().await {
        let Ok(Some(sender)) = transceiver.sender().await else {
            continue;
        };
        let Ok(parameters) = sender.get_parameters().await else {
            continue;
        };
        if let Some(codec) = parameters.rtp_parameters.codecs.iter().find(|codec| {
            codec
                .rtp_codec
                .mime_type
                .eq_ignore_ascii_case(MIME_TYPE_OPUS)
        }) && codec.payload_type != 0
        {
            payload_type.store(codec.payload_type, Ordering::Relaxed);
            return;
        }
    }
}

/// Démarre le décodage de chaque nouvelle piste audio reçue.
async fn start_receivers(
    pc: &Arc<dyn PeerConnection>,
    started: &mut HashSet<String>,
    audio: &Arc<AudioIo>,
) {
    let transceivers = pc.get_transceivers().await;
    for (index, transceiver) in transceivers.iter().enumerate() {
        let mid = transceiver
            .mid()
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| format!("m{index}"));
        if started.contains(&mid) {
            continue;
        }
        let Ok(Some(receiver)) = transceiver.receiver().await else {
            continue;
        };
        let track = Arc::clone(receiver.track());
        if track.kind().await != RtpCodecKind::Audio {
            continue;
        }
        started.insert(mid);
        let audio = Arc::clone(audio);
        tokio::spawn(async move {
            let Some(mut decoder) = decoder() else {
                return;
            };
            while let Some(event) = track.poll().await {
                match event {
                    TrackRemoteEvent::OnRtpPacket(packet) => {
                        audio.play_packet(&packet.payload, &mut decoder);
                    }
                    TrackRemoteEvent::OnEnded | TrackRemoteEvent::OnError => break,
                    _ => {}
                }
            }
        });
    }
}

/// Candidat ICE au format attendu par le serveur LiveKit (camelCase).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CandidateJson {
    candidate: String,
    #[serde(default)]
    sdp_mid: Option<String>,
    #[serde(default)]
    sdp_mline_index: Option<u16>,
    #[serde(default)]
    username_fragment: Option<String>,
}

fn parse_candidate(raw: &str) -> Option<RTCIceCandidateInit> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    let candidate = value.get("candidate")?.as_str()?.to_string();
    let sdp_mid = value
        .get("sdpMid")
        .or_else(|| value.get("sdp_mid"))
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let sdp_mline_index = value
        .get("sdpMLineIndex")
        .or_else(|| value.get("sdp_m_line_index"))
        .and_then(|value| value.as_u64())
        .map(|index| index as u16);
    let username_fragment = value
        .get("usernameFragment")
        .or_else(|| value.get("username_fragment"))
        .and_then(|value| value.as_str())
        .map(str::to_string);
    Some(RTCIceCandidateInit {
        candidate,
        sdp_mid: Some(
            sdp_mid
                .filter(|mid| !mid.is_empty())
                .unwrap_or_else(|| "0".to_string()),
        ),
        sdp_mline_index,
        username_fragment,
        url: None,
    })
}

/// Adaptateur d'événements : candidats ICE et état des connexions.
struct Handler {
    client: Arc<SignalClient>,
    target: i32,
    notify: UnboundedSender<Inner>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        let Ok(candidate) = event.candidate.to_json() else {
            return;
        };
        let json = serde_json::to_string(&CandidateJson {
            candidate: candidate.candidate,
            sdp_mid: candidate
                .sdp_mid
                .filter(|mid| !mid.is_empty())
                .or_else(|| Some("0".to_string())),
            sdp_mline_index: candidate.sdp_mline_index,
            username_fragment: candidate.username_fragment,
        })
        .ok();
        let Some(json) = json else { return };
        self.client
            .send(proto::signal_request::Message::Trickle(
                proto::TrickleRequest {
                    candidate_init: json,
                    target: self.target,
                    r#final: false,
                },
            ))
            .await;
    }

    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state != RTCIceGatheringState::Complete {
            return;
        }
        self.client
            .send(proto::signal_request::Message::Trickle(
                proto::TrickleRequest {
                    candidate_init: String::new(),
                    target: self.target,
                    r#final: true,
                },
            ))
            .await;
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        match state {
            RTCPeerConnectionState::Connected => {
                let message = if self.target == proto::SignalTarget::Publisher as i32 {
                    Inner::PublisherConnected
                } else {
                    Inner::SubscriberConnected
                };
                let _ = self.notify.send(message);
            }
            RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed => {
                let _ = self.notify.send(Inner::Shutdown);
            }
            _ => {}
        }
    }
}

fn track_ssrc(identity: &str) -> u32 {
    // FNV-1a : SSRC stable et non nul, sans dépendance supplémentaire.
    let mut hash = 0x811c_9dc5u32;
    for byte in identity.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash | 1
}

fn opus_codec() -> RTCRtpCodec {
    RTCRtpCodec {
        mime_type: MIME_TYPE_OPUS.to_string(),
        clock_rate: 48_000,
        channels: 2,
        sdp_fmtp_line: "minptime=10;useinbandfec=1".to_string(),
        rtcp_feedback: Vec::new(),
    }
}

fn build_mic(identity: &str, ssrc: u32) -> Result<Arc<TrackLocalStaticSample>, String> {
    let stream = format!("PA_{identity}");
    let track = MediaStreamTrack::new(
        stream,
        format!("TR_{identity}_microphone"),
        MICROPHONE.to_string(),
        RtpCodecKind::Audio,
        vec![RTCRtpEncodingParameters {
            rtp_coding_parameters: RTCRtpCodingParameters {
                ssrc: Some(ssrc),
                ..Default::default()
            },
            codec: opus_codec(),
            ..Default::default()
        }],
    );
    TrackLocalStaticSample::new(Instant::now(), track)
        .map(Arc::new)
        .map_err(|error| error.to_string())
}
