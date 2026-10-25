// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! Maillage vocal pair-à-pair.
//!
//! Chaque paire de participants négocie sa propre `RTCPeerConnection` avec
//! exactement quatre transceivers, dans l'ordre imposé par Fluxer :
//! `microphone`, `camera`, `screen_share`, `screen_share_audio`. Seul l'audio
//! du microphone est transporté pour l'instant ; les trois autres emplacements
//! existent pour rester compatible avec les clients officiels et recevront la
//! vidéo plus tard.
//!
//! La signalisation passe entièrement par la passerelle (`op 13`), avec les
//! messages `offer`, `answer`, `ice`, `need-offer`, `tracks` et `want`.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

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
use serde_json::{Value, json};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tracing::{debug, info, warn};

use webrtc::media_stream::track_local::TrackLocal;
use webrtc::media_stream::track_local::static_sample::TrackLocalStaticSample;
use webrtc::media_stream::track_remote::TrackRemoteEvent;
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceCandidateInit,
    RTCIceGatheringState, RTCPeerConnectionIceEvent, RTCPeerConnectionState,
};
use webrtc::rtp_transceiver::RtpTransceiver;

use crate::audio::{AudioIo, decoder};

const MICROPHONE: usize = 0;
const WATCHDOG: Duration = Duration::from_secs(10);
const MAX_ATTEMPTS: u8 = 3;

/// Serveur ICE transmis par `VOICE_SERVER_UPDATE` (STUN ou TURN).
#[derive(Debug, Clone, Default)]
pub struct IceServer {
    pub urls: Vec<String>,
    pub username: String,
    pub credential: String,
}

/// Paramètres d'une session de maillage.
#[derive(Debug, Clone)]
pub struct MeshConfig {
    pub guild_id: String,
    pub channel_id: String,
    /// Identifiant de connexion vocal de l'utilisateur local.
    pub local_connection_id: String,
    pub ice_servers: Vec<IceServer>,
    pub muted: bool,
    pub deafened: bool,
    /// Canal d'écriture vers la passerelle (messages `op 13`).
    pub gateway: UnboundedSender<Value>,
}

/// Événement remonté par le maillage.
#[derive(Debug, Clone)]
pub enum MeshEvent {
    Connected(String),
    Disconnected(String),
    AudioUnavailable(String),
    Error(String),
}

/// Poignée clonable vers l'acteur du maillage.
#[derive(Clone)]
pub struct Mesh {
    inbox: UnboundedSender<Inner>,
}

impl Mesh {
    /// Démarre le maillage et l'audio natif. Doit être appelé depuis un
    /// contexte Tokio.
    pub fn start(config: MeshConfig) -> Result<(Self, UnboundedReceiver<MeshEvent>), String> {
        if config.local_connection_id.is_empty() {
            return Err("identifiant de connexion vocal absent".into());
        }
        let (inbox, commands) = unbounded_channel();
        let (events, event_rx) = unbounded_channel();
        let (audio, mic_rx, audio_error) = AudioIo::start(config.muted, config.deafened);
        if !audio.available {
            let message = if audio_error.is_empty() {
                "aucun périphérique audio disponible".to_string()
            } else {
                audio_error.clone()
            };
            let _ = events.send(MeshEvent::AudioUnavailable(message));
        }
        let audio = Arc::new(audio);
        let inbox_tx = inbox.clone();
        tokio::spawn(async move {
            Actor::new(config, commands, inbox_tx, events, audio, mic_rx)
                .run()
                .await;
        });
        Ok((Self { inbox }, event_rx))
    }

    /// Transmet un message de signalisation reçu de la passerelle.
    pub fn signal(&self, from: impl Into<String>, data: Value) {
        let _ = self.inbox.send(Inner::Signal {
            from: from.into(),
            data,
        });
    }

    /// Déclare un participant présent dans le salon.
    pub fn peer_joined(&self, connection_id: impl Into<String>, user_id: impl Into<String>) {
        let _ = self.inbox.send(Inner::PeerJoined {
            connection_id: connection_id.into(),
            user_id: user_id.into(),
        });
    }

    /// Déclare un participant parti.
    pub fn peer_left(&self, connection_id: impl Into<String>) {
        let _ = self.inbox.send(Inner::PeerLeft {
            connection_id: connection_id.into(),
        });
    }

    pub fn set_muted(&self, muted: bool) {
        let _ = self.inbox.send(Inner::Muted(muted));
    }

    pub fn set_deafened(&self, deafened: bool) {
        let _ = self.inbox.send(Inner::Deafened(deafened));
    }

    /// Arrête les connexions et les périphériques audio.
    pub fn stop(&self) {
        let _ = self.inbox.send(Inner::Shutdown);
    }
}

enum Inner {
    Signal {
        from: String,
        data: Value,
    },
    PeerJoined {
        connection_id: String,
        user_id: String,
    },
    PeerLeft {
        connection_id: String,
    },
    Muted(bool),
    Deafened(bool),
    PeerState {
        connection_id: String,
        generation: u64,
        connected: bool,
    },
    Watchdog {
        connection_id: String,
        generation: u64,
    },
    Shutdown,
}

struct Peer {
    offerer: bool,
    generation: u64,
    answered: bool,
    connected: bool,
    attempts: u8,
    remote_set: bool,
    pending_ice: Vec<(u64, RTCIceCandidateInit)>,
    pc: Arc<dyn PeerConnection>,
    transceivers: Vec<Arc<dyn RtpTransceiver>>,
    mic: Arc<TrackLocalStaticSample>,
    mic_ssrc: u32,
    mic_payload_type: Option<u8>,
    receiver_started: bool,
}

impl Peer {
    fn transceiver(&self, index: usize) -> Option<Arc<dyn RtpTransceiver>> {
        self.transceivers.get(index).cloned()
    }
}

/// Adaptateur d'événements : candidats ICE et état de la connexion.
struct Handler {
    gateway: UnboundedSender<Value>,
    notify: UnboundedSender<Inner>,
    guild_id: String,
    channel_id: String,
    remote: String,
    generation: Arc<AtomicU64>,
    connected: AtomicBool,
}

impl Handler {
    fn send_signal(&self, data: Value) {
        let payload = json!({
            "op": 13,
            "d": {
                "guild_id": self.guild_id,
                "channel_id": self.channel_id,
                "to": self.remote,
                "data": data,
            }
        });
        let _ = self.gateway.send(payload);
    }
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        let Ok(mut candidate) = event.candidate.to_json() else {
            return;
        };
        // Le maillage négocie en `max-bundle` : toutes les pistes partagent la
        // section `m=0`. Le navigateur envoie `sdpMid: "0"`, et certains
        // serveurs SFU refusent un `sdpMid` vide.
        if candidate.sdp_mid.as_deref().unwrap_or_default().is_empty() {
            candidate.sdp_mid = Some("0".to_string());
        }
        let generation = self.generation.load(Ordering::Relaxed);
        self.send_signal(json!({ "t": "ice", "g": generation, "c": candidate }));
    }

    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let generation = self.generation.load(Ordering::Relaxed);
            self.send_signal(json!({ "t": "ice", "g": generation, "c": null }));
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        let generation = self.generation.load(Ordering::Relaxed);
        match state {
            RTCPeerConnectionState::Connected if !self.connected.swap(true, Ordering::SeqCst) => {
                let _ = self.notify.send(Inner::PeerState {
                    connection_id: self.remote.clone(),
                    generation,
                    connected: true,
                });
            }
            RTCPeerConnectionState::Disconnected
            | RTCPeerConnectionState::Failed
            | RTCPeerConnectionState::Closed
                if self.connected.swap(false, Ordering::SeqCst) =>
            {
                let _ = self.notify.send(Inner::PeerState {
                    connection_id: self.remote.clone(),
                    generation,
                    connected: false,
                });
            }
            _ => {}
        }
    }
}

struct Actor {
    config: MeshConfig,
    inbox: UnboundedReceiver<Inner>,
    inbox_tx: UnboundedSender<Inner>,
    events: UnboundedSender<MeshEvent>,
    gateway: UnboundedSender<Value>,
    audio: Arc<AudioIo>,
    mic_rx: UnboundedReceiver<Vec<u8>>,
    peers: HashMap<String, Peer>,
    last_generation: u64,
}

impl Actor {
    fn new(
        config: MeshConfig,
        inbox: UnboundedReceiver<Inner>,
        inbox_tx: UnboundedSender<Inner>,
        events: UnboundedSender<MeshEvent>,
        audio: Arc<AudioIo>,
        mic_rx: UnboundedReceiver<Vec<u8>>,
    ) -> Self {
        let gateway = config.gateway.clone();
        Self {
            config,
            inbox,
            inbox_tx,
            events,
            gateway,
            audio,
            mic_rx,
            peers: HashMap::new(),
            last_generation: 0,
        }
    }

    async fn run(mut self) {
        info!("maillage vocal démarré ({})", self.config.channel_id);

        loop {
            tokio::select! {
                command = self.inbox.recv() => {
                    let Some(command) = command else { break };
                    match command {
                        Inner::Shutdown => break,
                        Inner::Signal { from, data } => self.on_signal(from, data).await,
                        Inner::PeerJoined { connection_id, user_id } => {
                            self.on_peer_joined(connection_id, user_id).await;
                        }
                        Inner::PeerLeft { connection_id } => self.on_peer_left(&connection_id).await,
                        Inner::Muted(value) => self.audio.set_muted(value),
                        Inner::Deafened(value) => self.audio.set_deafened(value),
                        Inner::PeerState { connection_id, generation, connected } => {
                            self.on_peer_state(&connection_id, generation, connected).await;
                        }
                        Inner::Watchdog { connection_id, generation } => {
                            self.on_watchdog(&connection_id, generation).await;
                        }
                    }
                }
                packet = self.mic_rx.recv(), if !self.peers.is_empty() => {
                    if let Some(packet) = packet {
                        self.broadcast(&packet).await;
                    }
                }
            }
        }

        for (_, peer) in self.peers.drain() {
            let _ = peer.pc.close().await;
        }
        info!("maillage vocal arrêté");
    }

    async fn on_peer_joined(&mut self, connection_id: String, user_id: String) {
        if connection_id.is_empty() || connection_id == self.config.local_connection_id {
            return;
        }
        if self.peers.contains_key(&connection_id) {
            return;
        }
        let offerer = self.config.local_connection_id < connection_id;
        debug!("pair vocal {connection_id} ({user_id}) — offreur : {offerer}");

        let generation = if offerer { self.next_generation() } else { 0 };
        let pc = match self.build_pc(&connection_id, generation).await {
            Ok(pc) => pc,
            Err(error) => {
                warn!("création WebRTC ({connection_id}) : {error}");
                return;
            }
        };
        let ssrc = mic_ssrc(&self.config.local_connection_id, &connection_id);
        let mic = match rebuild_mic(&self.config.local_connection_id, ssrc) {
            Ok(mic) => mic,
            Err(error) => {
                warn!("piste micro ({connection_id}) : {error}");
                return;
            }
        };
        self.peers.insert(
            connection_id.clone(),
            Peer {
                offerer,
                generation,
                answered: false,
                connected: false,
                attempts: 0,
                remote_set: false,
                pending_ice: Vec::new(),
                pc,
                transceivers: Vec::new(),
                mic,
                mic_ssrc: ssrc,
                mic_payload_type: None,
                receiver_started: false,
            },
        );

        if offerer {
            self.offer(&connection_id).await;
        } else {
            self.send_data(&connection_id, json!({ "t": "need-offer", "g": 0 }));
        }
    }

    async fn on_peer_left(&mut self, connection_id: &str) {
        if let Some(peer) = self.peers.remove(connection_id) {
            let _ = peer.pc.close().await;
        }
    }

    async fn on_peer_state(&mut self, connection_id: &str, generation: u64, connected: bool) {
        let Some(peer) = self.peers.get_mut(connection_id) else {
            return;
        };
        if peer.generation != generation {
            // Événement d'une génération déjà remplacée.
            return;
        }
        peer.connected = connected;
        if connected {
            self.configure_sender(connection_id).await;
            self.start_receiver(connection_id).await;
            self.send_tracks(connection_id);
            let _ = self
                .events
                .send(MeshEvent::Connected(connection_id.to_string()));
        } else {
            let _ = self
                .events
                .send(MeshEvent::Disconnected(connection_id.to_string()));
        }
    }

    async fn on_watchdog(&mut self, connection_id: &str, generation: u64) {
        let Some(peer) = self.peers.get_mut(connection_id) else {
            return;
        };
        if peer.generation != generation || peer.connected {
            return;
        }
        if peer.attempts >= MAX_ATTEMPTS {
            let _ = self.events.send(MeshEvent::Error(format!(
                "pair vocal {connection_id} injoignable"
            )));
            return;
        }
        peer.attempts += 1;
        let retry = peer.generation;
        if peer.offerer {
            self.offer(connection_id).await;
        } else {
            self.send_data(connection_id, json!({ "t": "need-offer", "g": retry }));
        }
    }

    async fn on_signal(&mut self, from: String, data: Value) {
        if from.is_empty() || from == self.config.local_connection_id {
            return;
        }
        if !self.peers.contains_key(&from) {
            // Un signal peut arriver avant `VOICE_STATE_UPDATE` : on crée le
            // pair à la volée pour ne pas perdre la négociation.
            self.on_peer_joined(from.clone(), String::new()).await;
        }
        let Some(signal_type) = data.get("t").and_then(Value::as_str) else {
            return;
        };
        match signal_type {
            "offer" => {
                let (Some(generation), Some(sdp)) = (
                    data.get("g").and_then(Value::as_u64),
                    data.get("sdp").and_then(Value::as_str),
                ) else {
                    return;
                };
                self.accept_offer(&from, generation, sdp.to_string()).await;
            }
            "answer" => {
                let (Some(generation), Some(sdp)) = (
                    data.get("g").and_then(Value::as_u64),
                    data.get("sdp").and_then(Value::as_str),
                ) else {
                    return;
                };
                self.accept_answer(&from, generation, sdp.to_string()).await;
            }
            "ice" => {
                let Some(generation) = data.get("g").and_then(Value::as_u64) else {
                    return;
                };
                let candidate = data.get("c");
                if candidate.is_none() || candidate == Some(&Value::Null) {
                    return;
                }
                let Ok(candidate) =
                    serde_json::from_value::<RTCIceCandidateInit>(candidate.unwrap().clone())
                else {
                    return;
                };
                let Some(peer) = self.peers.get_mut(&from) else {
                    return;
                };
                if !peer.remote_set || peer.generation != generation {
                    // L'offre et les candidats peuvent arriver dans le même
                    // souffle : on garde les candidats en attente jusqu'à la
                    // description distante.
                    if peer.generation <= generation {
                        peer.pending_ice.push((generation, candidate));
                    }
                    return;
                }
                let pc = Arc::clone(&peer.pc);
                if let Err(error) = pc.add_ice_candidate(candidate).await {
                    debug!("candidat ICE ignoré ({from}) : {error}");
                }
            }
            "need-offer" => {
                let generation = data.get("g").and_then(Value::as_u64).unwrap_or(0);
                let restart = data.get("restart") == Some(&Value::Bool(true));
                let Some(peer) = self.peers.get(&from) else {
                    return;
                };
                if !peer.offerer {
                    return;
                }
                if !restart && peer.generation == generation && peer.answered {
                    return;
                }
                self.offer(&from).await;
            }
            "tracks" | "want" => {
                // L'audio du micro est toujours émis ; les pistes vidéo
                // seront filtrées ici quand elles seront implémentées.
            }
            _ => {}
        }
    }

    async fn offer(&mut self, connection_id: &str) {
        let generation = self.next_generation();
        let (mic, ssrc) = match self.peers.get_mut(connection_id) {
            Some(peer) => {
                peer.generation = generation;
                peer.answered = false;
                peer.connected = false;
                peer.receiver_started = false;
                peer.remote_set = false;
                peer.pending_ice.clear();
                peer.transceivers.clear();
                (Arc::clone(&peer.mic), peer.mic_ssrc)
            }
            None => return,
        };

        let pc = match self.build_pc(connection_id, generation).await {
            Ok(pc) => pc,
            Err(error) => {
                warn!("création WebRTC ({connection_id}) : {error}");
                return;
            }
        };

        let init = RTCRtpTransceiverInit {
            direction: RTCRtpTransceiverDirection::Sendrecv,
            streams: Vec::new(),
            send_encodings: Vec::new(),
        };
        let mut transceivers = Vec::with_capacity(4);
        match pc
            .add_transceiver_from_track(Arc::clone(&mic) as Arc<dyn TrackLocal>, Some(init.clone()))
            .await
        {
            Ok(transceiver) => transceivers.push(transceiver),
            Err(error) => {
                warn!("transceiver microphone ({connection_id}) : {error}");
                return;
            }
        }
        for kind in [
            RtpCodecKind::Video,
            RtpCodecKind::Video,
            RtpCodecKind::Audio,
        ] {
            match pc.add_transceiver_from_kind(kind, Some(init.clone())).await {
                Ok(transceiver) => transceivers.push(transceiver),
                Err(error) => {
                    warn!("transceiver {kind:?} ({connection_id}) : {error}");
                    return;
                }
            }
        }

        let offer = match pc.create_offer(None).await {
            Ok(offer) => offer,
            Err(error) => {
                warn!("offre SDP ({connection_id}) : {error}");
                return;
            }
        };
        if let Err(error) = pc.set_local_description(offer.clone()).await {
            warn!("description locale ({connection_id}) : {error}");
            return;
        }

        let Some(peer) = self.peers.get_mut(connection_id) else {
            return;
        };
        peer.pc = pc;
        peer.transceivers = transceivers;
        peer.mic = mic;
        peer.mic_ssrc = ssrc;
        self.send_data(
            connection_id,
            json!({ "t": "offer", "g": generation, "sdp": offer.sdp }),
        );
        self.start_receiver(connection_id).await;
        self.configure_sender(connection_id).await;
        self.arm_watchdog(connection_id, generation);
    }

    async fn accept_offer(&mut self, connection_id: &str, generation: u64, sdp: String) {
        let (offerer, current, has_transport) = match self.peers.get(connection_id) {
            Some(peer) => (peer.offerer, peer.generation, !peer.transceivers.is_empty()),
            None => return,
        };
        if offerer {
            return;
        }

        // Même génération déjà négociée : il s'agit d'un redémarrage ICE.
        if current == generation && has_transport {
            let pc = match self.peers.get(connection_id) {
                Some(peer) => Arc::clone(&peer.pc),
                None => return,
            };
            let Ok(description) = RTCSessionDescription::offer(sdp) else {
                return;
            };
            if let Err(error) = pc.set_remote_description(description).await {
                warn!("offre distante ({connection_id}) : {error}");
                return;
            }
            let answer = match pc.create_answer(None).await {
                Ok(answer) => answer,
                Err(error) => {
                    warn!("réponse SDP ({connection_id}) : {error}");
                    return;
                }
            };
            if let Err(error) = pc.set_local_description(answer.clone()).await {
                warn!("description locale ({connection_id}) : {error}");
                return;
            }
            if let Some(peer) = self.peers.get_mut(connection_id) {
                peer.remote_set = true;
            }
            self.send_data(
                connection_id,
                json!({ "t": "answer", "g": generation, "sdp": answer.sdp }),
            );
            self.flush_ice(connection_id).await;
            self.start_receiver(connection_id).await;
            self.configure_sender(connection_id).await;
            return;
        }

        if generation < current {
            return;
        }

        let (mic, ssrc) = match self.peers.get(connection_id) {
            Some(peer) => (Arc::clone(&peer.mic), peer.mic_ssrc),
            None => return,
        };
        let pc = match self.build_pc(connection_id, generation).await {
            Ok(pc) => pc,
            Err(error) => {
                warn!("création WebRTC ({connection_id}) : {error}");
                return;
            }
        };
        let Ok(description) = RTCSessionDescription::offer(sdp) else {
            return;
        };
        if let Err(error) = pc.set_remote_description(description).await {
            warn!("offre distante ({connection_id}) : {error}");
            return;
        }

        let transceivers = pc.get_transceivers().await;
        if transceivers.len() != 4 {
            let _ = self.events.send(MeshEvent::Error(format!(
                "offre vocale invalide : {} transceivers",
                transceivers.len()
            )));
            return;
        }
        for (index, expected) in [
            RtpCodecKind::Audio,
            RtpCodecKind::Video,
            RtpCodecKind::Video,
            RtpCodecKind::Audio,
        ]
        .into_iter()
        .enumerate()
        {
            let Some(receiver) = transceivers[index].receiver().await.ok().flatten() else {
                return;
            };
            if receiver.track().kind().await != expected {
                let _ = self.events.send(MeshEvent::Error(
                    "offre vocale : disposition des pistes incorrecte".into(),
                ));
                return;
            }
        }

        for transceiver in &transceivers {
            if let Err(error) = transceiver
                .set_direction(RTCRtpTransceiverDirection::Sendrecv)
                .await
            {
                warn!("direction transceiver ({connection_id}) : {error}");
                return;
            }
        }

        let Some(sender) = transceivers[MICROPHONE].sender().await.ok().flatten() else {
            return;
        };
        if let Err(error) = sender
            .replace_track(Arc::clone(&mic) as Arc<dyn TrackLocal>)
            .await
        {
            warn!("piste micro ({connection_id}) : {error}");
            return;
        }

        let answer = match pc.create_answer(None).await {
            Ok(answer) => answer,
            Err(error) => {
                warn!("réponse SDP ({connection_id}) : {error}");
                return;
            }
        };
        if let Err(error) = pc.set_local_description(answer.clone()).await {
            warn!("description locale ({connection_id}) : {error}");
            return;
        }

        let Some(peer) = self.peers.get_mut(connection_id) else {
            return;
        };
        peer.generation = generation;
        peer.pc = pc;
        peer.transceivers = transceivers;
        peer.mic = mic;
        peer.mic_ssrc = ssrc;
        peer.answered = false;
        peer.receiver_started = false;
        peer.connected = false;
        peer.remote_set = true;

        self.send_data(
            connection_id,
            json!({ "t": "answer", "g": generation, "sdp": answer.sdp }),
        );
        self.flush_ice(connection_id).await;
        self.start_receiver(connection_id).await;
        self.configure_sender(connection_id).await;
        self.send_tracks(connection_id);
        self.arm_watchdog(connection_id, generation);
    }

    async fn accept_answer(&mut self, connection_id: &str, generation: u64, sdp: String) {
        let pc = match self.peers.get(connection_id) {
            Some(peer) if peer.offerer && peer.generation == generation => Arc::clone(&peer.pc),
            _ => return,
        };
        let Ok(description) = RTCSessionDescription::answer(sdp) else {
            return;
        };
        if let Err(error) = pc.set_remote_description(description).await {
            warn!("réponse distante ({connection_id}) : {error}");
            return;
        }
        if let Some(peer) = self.peers.get_mut(connection_id) {
            peer.answered = true;
            peer.remote_set = true;
        }
        self.flush_ice(connection_id).await;
        self.start_receiver(connection_id).await;
        self.configure_sender(connection_id).await;
        self.send_tracks(connection_id);
    }

    async fn flush_ice(&mut self, connection_id: &str) {
        let (pc, remote_set, generation, pending) = match self.peers.get_mut(connection_id) {
            Some(peer) => (
                Arc::clone(&peer.pc),
                peer.remote_set,
                peer.generation,
                std::mem::take(&mut peer.pending_ice),
            ),
            None => return,
        };
        if !remote_set {
            return;
        }
        for (candidate_generation, candidate) in pending {
            if candidate_generation == generation
                && let Err(error) = pc.add_ice_candidate(candidate).await
            {
                debug!("candidat ICE en attente ignoré ({connection_id}) : {error}");
            }
        }
    }

    fn arm_watchdog(&self, connection_id: &str, generation: u64) {
        let inbox = self.inbox_tx.clone();
        let connection_id = connection_id.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(WATCHDOG).await;
            let _ = inbox.send(Inner::Watchdog {
                connection_id,
                generation,
            });
        });
    }

    async fn configure_sender(&mut self, connection_id: &str) {
        let Some(transceiver) = self
            .peers
            .get(connection_id)
            .and_then(|peer| peer.transceiver(MICROPHONE))
        else {
            return;
        };
        let Some(sender) = transceiver.sender().await.ok().flatten() else {
            return;
        };
        let Ok(parameters) = sender.get_parameters().await else {
            return;
        };
        let payload_type = parameters
            .rtp_parameters
            .codecs
            .first()
            .map(|codec| codec.payload_type);
        if let Some(peer) = self.peers.get_mut(connection_id) {
            peer.mic_payload_type = payload_type;
        }
    }

    async fn start_receiver(&mut self, connection_id: &str) {
        let (ready, transceiver) = match self.peers.get(connection_id) {
            Some(peer) => (!peer.receiver_started, peer.transceiver(MICROPHONE)),
            None => return,
        };
        let Some(transceiver) = transceiver else {
            return;
        };
        if !ready {
            return;
        }
        let Some(receiver) = transceiver.receiver().await.ok().flatten() else {
            return;
        };
        let track = Arc::clone(receiver.track());
        if let Some(peer) = self.peers.get_mut(connection_id) {
            peer.receiver_started = true;
        }
        let audio = Arc::clone(&self.audio);
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

    async fn broadcast(&mut self, packet: &[u8]) {
        let now = Instant::now();
        let targets: Vec<(Arc<TrackLocalStaticSample>, u32, u8)> = self
            .peers
            .values()
            .filter_map(|peer| {
                peer.mic_payload_type
                    .map(|payload_type| (Arc::clone(&peer.mic), peer.mic_ssrc, payload_type))
            })
            .collect();
        for (mic, ssrc, payload_type) in targets {
            let sample = Sample {
                data: packet.to_vec().into(),
                duration: Duration::from_millis(20),
                ..Sample::new(now)
            };
            if let Err(error) = mic.write_sample(ssrc, payload_type, &sample, &[]).await {
                debug!("envoi audio : {error}");
            }
        }
    }

    fn send_tracks(&self, connection_id: &str) {
        let sid = format!("TR_{}_microphone", self.config.local_connection_id);
        self.send_data(
            connection_id,
            json!({
                "t": "tracks",
                "v": 1,
                "tracks": [{
                    "src": "microphone",
                    "sid": sid,
                    "m": true,
                }],
            }),
        );
    }

    fn send_data(&self, connection_id: &str, data: Value) {
        let payload = json!({
            "op": 13,
            "d": {
                "guild_id": self.config.guild_id,
                "channel_id": self.config.channel_id,
                "to": connection_id,
                "data": data,
            }
        });
        let _ = self.gateway.send(payload);
    }

    async fn build_pc(
        &self,
        connection_id: &str,
        generation: u64,
    ) -> Result<Arc<dyn PeerConnection>, String> {
        let mut media_engine = MediaEngine::default();
        media_engine
            .register_default_codecs()
            .map_err(|error| error.to_string())?;
        let registry = register_default_interceptors(Registry::new(), &mut media_engine)
            .map_err(|error| error.to_string())?;
        let configuration = RTCConfigurationBuilder::new()
            .with_ice_servers(
                self.config
                    .ice_servers
                    .iter()
                    .map(|server| RtcIceServer {
                        urls: server.urls.clone(),
                        username: server.username.clone(),
                        credential: server.credential.clone(),
                    })
                    .collect(),
            )
            .with_bundle_policy(RTCBundlePolicy::MaxBundle)
            .build();

        let generation = Arc::new(AtomicU64::new(generation));
        let handler = Arc::new(Handler {
            gateway: self.gateway.clone(),
            notify: self.inbox_tx.clone(),
            guild_id: self.config.guild_id.clone(),
            channel_id: self.config.channel_id.clone(),
            remote: connection_id.to_string(),
            generation,
            connected: AtomicBool::new(false),
        });

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

    fn next_generation(&mut self) -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0);
        self.last_generation = now.max(self.last_generation + 1);
        self.last_generation
    }
}

fn mic_ssrc(local: &str, remote: &str) -> u32 {
    // FNV-1a : un SSRC stable et non nul, sans dépendance supplémentaire.
    let mut hash = 0x811c_9dc5u32;
    for byte in local
        .bytes()
        .chain(b":".iter().copied())
        .chain(remote.bytes())
    {
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

fn rebuild_mic(
    local_connection_id: &str,
    ssrc: u32,
) -> Result<Arc<TrackLocalStaticSample>, String> {
    let stream = format!("PA_{local_connection_id}");
    let track = MediaStreamTrack::new(
        stream,
        format!("TR_{local_connection_id}_microphone"),
        "microphone".to_string(),
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
