// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! Moteur vocal natif : maillage WebRTC, Opus pur Rust et ALSA.
//!
//! Aucune dépendance C n'est compilée : ALSA est chargé à l'exécution et
//! l'encodeur/décodeur Opus est écrit en Rust.

pub mod alsa;
pub mod audio;
pub mod mesh;
pub mod sfu;

pub use audio::AudioIo;
pub use mesh::{IceServer, Mesh, MeshConfig, MeshEvent};
pub use sfu::{Sfu, SfuConfig, SfuEvent};
