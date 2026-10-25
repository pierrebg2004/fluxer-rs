// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! Installe l'icône et l'entrée de menu de Fluxer-rs pour le bureau courant.
//!
//! ```sh
//! cargo run -p fluxer-gui --example install_desktop
//! cargo run -p fluxer-gui --example install_desktop -- /chemin/vers/fluxer
//! ```
//!
//! Écrit :
//! - `~/.local/share/icons/hicolor/<taille>/apps/fluxer-rs.png`
//! - `~/.local/share/pixmaps/fluxer-rs.png`
//! - `~/.local/share/applications/fluxer-rs.desktop`
//!
//! Tout est fait en Rust ; aucune dépendance à un script externe et aucun
//! fichier binaire dans le dépôt : le logo est dessiné par `src/logo.rs`.

use std::path::{Path, PathBuf};

use image::imageops::FilterType;

// Le générateur d'icône est partagé avec le binaire : un seul dessin en Rust
// alimente la fenêtre et l'entrée de bureau.
#[path = "../src/logo.rs"]
mod logo;

const SIZES: [u32; 8] = [16, 24, 32, 48, 64, 128, 256, 512];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let home = std::env::var("HOME").map(PathBuf::from)?;
    let data = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home.join(".local").join("share"));

    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("racine du projet introuvable")?;

    // Logo rastérisé en Rust, puis réduit pour chaque variante du thème.
    let logo = image::RgbaImage::from_raw(logo::SIZE, logo::SIZE, logo::icon_rgba())
        .ok_or("dimensions d'icône invalides")?;

    for size in SIZES {
        let dir = data
            .join("icons")
            .join("hicolor")
            .join(format!("{size}x{size}"))
            .join("apps");
        std::fs::create_dir_all(&dir)?;
        let resized = image::imageops::resize(&logo, size, size, FilterType::Lanczos3);
        resized.save(dir.join("fluxer-rs.png"))?;
    }

    let pixmaps = data.join("pixmaps");
    std::fs::create_dir_all(&pixmaps)?;
    logo.save(pixmaps.join("fluxer-rs.png"))?;

    // Binaire à lancer : argument explicite, sinon release puis debug.
    let binary = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| {
            ["target/release/fluxer", "target/debug/fluxer"]
                .iter()
                .map(|relative| root.join(relative))
                .find(|path| path.exists())
        })
        .ok_or("binaire fluxer introuvable : compilez d'abord avec `cargo build`")?;
    let binary = binary.canonicalize()?;

    let applications = data.join("applications");
    std::fs::create_dir_all(&applications)?;
    let desktop = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Version=1.0\n\
         Name=Fluxer-rs\n\
         GenericName=Client de messagerie\n\
         Comment=Client Fluxer natif écrit en Rust\n\
         Exec={} %U\n\
         TryExec={}\n\
         Icon=fluxer-rs\n\
         Terminal=false\n\
         Categories=Network;InstantMessaging;Chat;\n\
         Keywords=fluxer;chat;messagerie;rust;\n\
         StartupNotify=true\n\
         StartupWMClass=Fluxer-rs\n",
        binary.display(),
        binary.display()
    );
    std::fs::write(applications.join("fluxer-rs.desktop"), desktop)?;

    println!("icônes installées dans {}", data.join("icons").display());
    println!(
        "entrée de menu: {}",
        applications.join("fluxer-rs.desktop").display()
    );
    println!("binaire utilisé: {}", binary.display());
    Ok(())
}
