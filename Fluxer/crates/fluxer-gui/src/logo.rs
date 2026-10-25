// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! Icône de l'application, entièrement dessinée en Rust.
//!
//! Le dépôt ne contient aucun fichier binaire : le logo est calculé à la
//! compilation (constantes) puis rastérisé à l'exécution, et l'installateur
//! d'entrée de bureau encode lui-même les PNG dans le thème d'icônes.

/// Taille du logo généré. 256 px suffit pour la barre des tâches et reste
/// léger en mémoire ; l'installateur redimensionne ensuite chaque variante.
pub const SIZE: u32 = 256;

/// Rastérise le logo (carré arrondi rouge, vague blanche) en RGBA non
/// prémultiplié, prêt pour `egui::IconData` et pour l'encodeur PNG.
pub fn icon_rgba() -> Vec<u8> {
    let size = SIZE as usize;
    let mut out = vec![0u8; size * size * 4];
    let radius = 52.0f32;
    let wave = Wave::default();

    for y in 0..size {
        for x in 0..size {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;

            // Distance signée au carré arrondi : négative à l'intérieur.
            let d = rounded_rect_distance(px, py, size as f32, radius);
            let coverage = smoothstep(1.0, -1.0, d);
            if coverage <= 0.0 {
                continue;
            }

            // Dégradé diagonal du rouge clair vers le rouge sombre.
            let t = (px / size as f32 + py / size as f32) * 0.5;
            let bg = [
                lerp(255.0, 143.0, t),
                lerp(107.0, 28.0, t),
                lerp(74.0, 18.0, t),
            ];

            // La vague blanche est une courbe sinusoïdale épaisse et arrondie.
            let wave_distance = wave.distance(px, py, size as f32);
            let wave_coverage = smoothstep(1.2, -1.2, wave_distance - wave.thickness);

            let color = [
                lerp(bg[0], 255.0, wave_coverage),
                lerp(bg[1], 255.0, wave_coverage),
                lerp(bg[2], 255.0, wave_coverage),
            ];

            // Léger éclat en haut du logo, comme le rendu officiel.
            let shine = (1.0 - (py / size as f32)).powi(3) * 0.10;
            let index = (y * size + x) * 4;
            out[index] = (color[0] + 255.0 * shine).min(255.0) as u8;
            out[index + 1] = (color[1] + 255.0 * shine).min(255.0) as u8;
            out[index + 2] = (color[2] + 255.0 * shine).min(255.0) as u8;
            out[index + 3] = (coverage * 255.0).round() as u8;
        }
    }
    out
}

/// Paramètres de la vague : deux arches descendantes puis ascendantes.
struct Wave {
    thickness: f32,
    amplitude: f32,
}

impl Default for Wave {
    fn default() -> Self {
        Self {
            thickness: 11.5,
            amplitude: 34.0,
        }
    }
}

impl Wave {
    /// Distance verticale approchée à la courbe, corrigée de sa pente pour
    /// obtenir une épaisseur constante sur les flancs.
    fn distance(&self, x: f32, y: f32, size: f32) -> f32 {
        let phase = (x / size) * std::f32::consts::TAU * 1.15 - 0.35;
        let center = size * 0.5 + phase.sin() * self.amplitude;
        let slope = phase.cos() * self.amplitude * std::f32::consts::TAU * 1.15 / size;
        (y - center).abs() / (1.0 + slope * slope).sqrt()
    }
}

fn rounded_rect_distance(x: f32, y: f32, size: f32, radius: f32) -> f32 {
    let half = size * 0.5;
    let qx = (x - half).abs() - (half - radius);
    let qy = (y - half).abs() - (half - radius);
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) - radius
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    if edge0 == edge1 {
        return if value < edge0 { 0.0 } else { 1.0 };
    }
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}
