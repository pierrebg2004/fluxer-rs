// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! Fluxer-rs — native Rust desktop client for Fluxer.
//!
//! No Electron, no JavaScript, no WebView: the HTTP API, the realtime gateway
//! and the whole UI are implemented in Rust (eframe/egui + tokio).

mod app;
mod engine;
mod fonts;
mod icons;
mod logo;
mod markdown;
mod rich;
mod theme;
mod ui;

use eframe::egui;

fn main() -> eframe::Result<()> {
    // 100 % Wayland : on refuse explicitement X11. L'interface n'est compilée
    // qu'avec la plateforme Wayland de winit, et cette variable empêche toute
    // tentative de repli si les deux backends venaient à être présents.
    // SAFETY: appelé avant la création de tout thread.
    unsafe {
        std::env::set_var("WINIT_UNIX_BACKEND", "wayland");
    }
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        eprintln!(
            "Fluxer-rs requiert une session Wayland (WAYLAND_DISPLAY absent). \
             Aucun repli X11 n'est prévu."
        );
        std::process::exit(1);
    }

    // Plusieurs piles TLS coexistent dans le graphe de dépendances (reqwest et
    // tokio-tungstenite), rustls ne peut donc pas choisir de fournisseur seul.
    let _ = rustls::crypto::ring::default_provider().install_default();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    // 100 % Vulkan : on force wgpu sur le backend Vulkan, sans repli OpenGL.
    let mut wgpu_options = eframe::egui_wgpu::WgpuConfiguration::default();
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut wgpu_options.wgpu_setup {
        setup.instance_descriptor.backends = eframe::wgpu::Backends::VULKAN;
        setup.power_preference = eframe::wgpu::PowerPreference::HighPerformance;
    }

    let window_size = std::env::var("FLUXER_WINDOW_SIZE")
        .ok()
        .and_then(|value| {
            let (width, height) = value.split_once('x')?;
            Some([width.trim().parse::<f32>().ok()?, height.trim().parse::<f32>().ok()?])
        })
        .unwrap_or([1280.0, 832.0]);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(window_size)
            .with_min_inner_size([960.0, 600.0])
            .with_title("Fluxer-rs")
            .with_icon(window_icon()),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options,
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        "Fluxer-rs",
        options,
        Box::new(|cc| {
            // Decode remote avatars, guild icons and banners (webp/gif/png/jpeg).
            egui_extras::install_image_loaders(&cc.egui_ctx);
            fonts::install(&cc.egui_ctx);
            Ok(Box::new(app::FluxerApp::new(cc)))
        }),
    )
}

/// Icône de fenêtre, dessinée en Rust (aucun fichier binaire dans le dépôt)
/// pour que la barre des tâches affiche la vague rouge même hors installation
/// dans le thème d'icônes du bureau.
fn window_icon() -> egui::IconData {
    egui::IconData {
        rgba: logo::icon_rgba(),
        width: logo::SIZE,
        height: logo::SIZE,
    }
}
