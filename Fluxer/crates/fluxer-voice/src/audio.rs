// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! Capture et restitution audio : Opus (pur Rust) + ALSA.
//!
//! La capture tourne dans un thread bloquant dédié : elle lit des trames de
//! 20 ms, encode en Opus et les pousse vers le maillage WebRTC. La lecture
//! consomme un mixeur partagé alimenté par les décodeurs distants.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use rusty_opus::{Application, OpusDecoder, OpusEncoder};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tracing::{info, warn};

use crate::alsa::{Devices, FRAME_SAMPLES, Pcm};

const CHANNELS: usize = 2;
const SAMPLE_RATE: i32 = 48_000;
/// Marge du mixeur : 200 ms. Au-delà, les trames les plus anciennes sont
/// jetées pour éviter de prendre du retard sur le direct.
const MIXER_CAPACITY: usize = 48_000 * CHANNELS / 5;

/// Mixeur stéréo entrelacé partagé entre les pistes distantes et la lecture.
#[derive(Default)]
pub struct Mixer {
    samples: VecDeque<f32>,
}

impl Mixer {
    /// Ajoute une trame décodée sans écraser les autres participants.
    ///
    /// Les trames des différents pairs sont alignées sur la même horloge de
    /// 20 ms : on additionne donc au début de la file, et on complète la file
    /// quand la trame est plus longue que ce qui reste à restituer.
    pub fn push(&mut self, frame: &[f32]) {
        for (index, sample) in frame.iter().enumerate() {
            if let Some(slot) = self.samples.get_mut(index) {
                *slot += *sample;
            } else {
                self.samples.push_back(*sample);
            }
        }
        while self.samples.len() > MIXER_CAPACITY {
            self.samples.pop_front();
        }
    }

    /// Récupère exactement `count` échantillons, complétés par du silence.
    fn take(&mut self, count: usize) -> Vec<f32> {
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            out.push(self.samples.pop_front().unwrap_or(0.0));
        }
        out
    }
}

/// Périphériques audio d'une session vocale.
///
/// Le récepteur des paquets du micro est rendu séparément par [`AudioIo::start`]
/// afin que le reste de la structure puisse être partagé (`Arc`) entre les
/// boucles de réception des pairs.
pub struct AudioIo {
    mixer: Arc<Mutex<Mixer>>,
    muted: Arc<AtomicBool>,
    deafened: Arc<AtomicBool>,
    closed: Arc<AtomicBool>,
    capture_thread: Option<JoinHandle<()>>,
    playback_thread: Option<JoinHandle<()>>,
    /// Vrai si au moins un périphérique a pu être ouvert.
    pub available: bool,
}

impl AudioIo {
    /// Ouvre les périphériques et démarre les deux threads. Une capture ou une
    /// lecture indisponible n'empêche pas la signalisation vocale : le client
    /// reste connecté, simplement sans micro ou sans son.
    pub fn start(muted: bool, deafened: bool) -> (Self, UnboundedReceiver<Vec<u8>>, String) {
        let (mic_tx, mic_rx) = unbounded_channel();
        let mixer = Arc::new(Mutex::new(Mixer::default()));
        let muted = Arc::new(AtomicBool::new(muted));
        let deafened = Arc::new(AtomicBool::new(deafened));
        let closed = Arc::new(AtomicBool::new(false));

        let devices = match Devices::open() {
            Ok(devices) => devices,
            Err(error) => {
                warn!("audio indisponible : {error}");
                return (
                    Self {
                        mixer,
                        muted,
                        deafened,
                        closed,
                        capture_thread: None,
                        playback_thread: None,
                        available: false,
                    },
                    mic_rx,
                    error,
                );
            }
        };

        let mut available = false;
        let capture_thread = devices.capture.map(|pcm| {
            available = true;
            spawn_capture(pcm, mic_tx, muted_clone(&muted), closed_clone(&closed))
        });
        let playback_thread = devices.playback.map(|pcm| {
            available = true;
            spawn_playback(
                pcm,
                mixer.clone(),
                deafened_clone(&deafened),
                closed_clone(&closed),
            )
        });

        (
            Self {
                mixer,
                muted,
                deafened,
                closed,
                capture_thread,
                playback_thread,
                available,
            },
            mic_rx,
            String::new(),
        )
    }

    pub fn set_muted(&self, value: bool) {
        self.muted.store(value, Ordering::Relaxed);
    }

    pub fn set_deafened(&self, value: bool) {
        self.deafened.store(value, Ordering::Relaxed);
    }

    /// Décode un paquet Opus distant et l'ajoute au mixeur de lecture.
    pub fn play_packet(&self, packet: &[u8], decoder: &mut OpusDecoder) {
        let mut pcm = vec![0.0f32; FRAME_SAMPLES * CHANNELS];
        match decoder.decode(packet, FRAME_SAMPLES, &mut pcm) {
            Ok(frames) => {
                let samples = frames * CHANNELS;
                if let Ok(mut mixer) = self.mixer.lock() {
                    mixer.push(&pcm[..samples.min(pcm.len())]);
                }
            }
            Err(_) => {
                // Une perte de paquet est normale ; on insère une trame de
                // compensation pour masquer le trou.
                let _ = decoder.decode(&[], FRAME_SAMPLES, &mut pcm);
            }
        }
    }
}

impl Drop for AudioIo {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Relaxed);
        if let Some(thread) = self.capture_thread.take() {
            let _ = thread.join();
        }
        if let Some(thread) = self.playback_thread.take() {
            let _ = thread.join();
        }
    }
}

fn muted_clone(muted: &Arc<AtomicBool>) -> Arc<AtomicBool> {
    muted.clone()
}

fn deafened_clone(deafened: &Arc<AtomicBool>) -> Arc<AtomicBool> {
    deafened.clone()
}

fn closed_clone(closed: &Arc<AtomicBool>) -> Arc<AtomicBool> {
    closed.clone()
}

fn spawn_capture(
    pcm: Pcm,
    mic_tx: UnboundedSender<Vec<u8>>,
    muted: Arc<AtomicBool>,
    closed: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("fluxer-voice-capture".into())
        .spawn(move || {
            let mut encoder = match OpusEncoder::new(SAMPLE_RATE, CHANNELS, Application::Voip) {
                Ok(encoder) => encoder,
                Err(error) => {
                    warn!("encodeur Opus indisponible : {error}");
                    return;
                }
            };
            encoder.bitrate_bps = 64_000;
            encoder.complexity = 5;
            encoder.use_inband_fec = true;
            encoder.packet_loss_perc = 10;
            encoder.use_dtx = true;

            let mut buffer = vec![0i16; FRAME_SAMPLES * CHANNELS];
            let mut pcm_f32 = vec![0.0f32; FRAME_SAMPLES * CHANNELS];
            let mut packet = vec![0u8; 1_500];
            let mut silence = 0u32;

            while !closed.load(Ordering::Relaxed) {
                let read = match pcm.read_i16(&mut buffer) {
                    Ok(frames) if frames > 0 => frames,
                    Ok(_) => continue,
                    Err(error) => {
                        warn!("capture audio : {error}");
                        thread::sleep(std::time::Duration::from_millis(20));
                        continue;
                    }
                };

                let frames = read.min(FRAME_SAMPLES);
                let samples = frames * CHANNELS;
                let is_muted = muted.load(Ordering::Relaxed);
                for index in 0..samples {
                    pcm_f32[index] = if is_muted {
                        0.0
                    } else {
                        buffer[index] as f32 / 32768.0
                    };
                }
                // Complète une lecture partielle avec du silence.
                for sample in pcm_f32
                    .iter_mut()
                    .take(FRAME_SAMPLES * CHANNELS)
                    .skip(samples)
                {
                    *sample = 0.0;
                }

                if is_muted {
                    silence = silence.saturating_add(1);
                } else {
                    silence = 0;
                }
                let level = peak(&pcm_f32[..samples]);
                if is_muted && level < 0.0001 {
                    // Avec DTX, les longues plages muettes coûtent un octet.
                    encoder.use_dtx = true;
                }

                match encoder.encode(&pcm_f32, FRAME_SAMPLES, &mut packet) {
                    Ok(len) if len > 0 => {
                        if mic_tx.send(packet[..len].to_vec()).is_err() {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(error) => warn!("encodage Opus : {error}"),
                }
            }
            info!("capture audio arrêtée");
        })
        .expect("thread de capture audio")
}

fn spawn_playback(
    pcm: Pcm,
    mixer: Arc<Mutex<Mixer>>,
    deafened: Arc<AtomicBool>,
    closed: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("fluxer-voice-playback".into())
        .spawn(move || {
            let samples = FRAME_SAMPLES * CHANNELS;
            let mut buffer = vec![0i16; samples];
            while !closed.load(Ordering::Relaxed) {
                let mixed = match mixer.lock() {
                    Ok(mut mixer) => mixer.take(samples),
                    Err(_) => break,
                };
                let silent = deafened.load(Ordering::Relaxed);
                for (slot, sample) in buffer.iter_mut().zip(mixed) {
                    *slot = if silent {
                        0
                    } else {
                        (sample.clamp(-1.0, 1.0) * 32767.0) as i16
                    };
                }
                if let Err(error) = pcm.write_i16(&buffer) {
                    warn!("lecture audio : {error}");
                    thread::sleep(std::time::Duration::from_millis(20));
                }
            }
            info!("lecture audio arrêtée");
        })
        .expect("thread de lecture audio")
}

fn peak(samples: &[f32]) -> f32 {
    samples
        .iter()
        .fold(0.0f32, |max, sample| max.max(sample.abs()))
}

/// Crée un décodeur Opus stéréo prêt à l'emploi.
pub fn decoder() -> Option<OpusDecoder> {
    match OpusDecoder::new(SAMPLE_RATE, CHANNELS) {
        Ok(decoder) => Some(decoder),
        Err(error) => {
            warn!("décodeur Opus indisponible : {error}");
            None
        }
    }
}
