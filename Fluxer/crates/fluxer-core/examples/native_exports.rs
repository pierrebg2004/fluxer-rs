// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! Inventaire des modules natifs (`.node`) livrés avec le client officiel.
//!
//! Remplace l'ancien script `scripts/dump_native_exports.cjs` : le projet ne
//! contient plus une seule ligne de JavaScript. Ce binaire Rust parcourt
//! `app.asar.unpacked/node_modules/@fluxer`, lit chaque module ELF et affiche
//! en JSON son paquet, sa taille, son point d'entrée N-API et les symboles
//! exportés.
//!
//! ```sh
//! cargo run -p fluxer-core --example native_exports
//! ```

use std::path::{Path, PathBuf};

use object::{Object, ObjectSymbol};

const ROOT: &str = "/opt/Fluxer/resources/app.asar.unpacked/node_modules/@fluxer";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(ROOT));

    let mut modules = Vec::new();
    for entry in std::fs::read_dir(&root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let package = entry.file_name().to_string_lossy().to_string();
        for file in std::fs::read_dir(entry.path())? {
            let file = file?;
            let name = file.file_name().to_string_lossy().to_string();
            if !name.ends_with(".node") {
                continue;
            }
            modules.push(describe(&package, &file.path()));
        }
    }

    modules.sort_by(|a, b| (&a.package, &a.file).cmp(&(&b.package, &b.file)));
    println!("{}", serde_json::to_string_pretty(&modules)?);
    eprintln!("{} module(s) natif(s) inspecté(s)", modules.len());
    Ok(())
}

/// Description d'un module natif, sérialisée telle quelle en JSON.
#[derive(serde::Serialize)]
struct NativeModule {
    package: String,
    file: String,
    size: u64,
    /// `true` lorsque le binaire expose le point d'entrée N-API attendu par
    /// Node.js (équivalent du `require()` réussi de l'ancien script).
    napi: bool,
    /// Symboles dynamiques exportés, dans l'ordre du fichier ELF.
    exports: Vec<String>,
}

fn describe(package: &str, path: &Path) -> NativeModule {
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let mut exports = Vec::new();
    let mut napi = false;

    if let Ok(bytes) = std::fs::read(path)
        && let Ok(object) = object::File::parse(&*bytes)
    {
        // Only dynamic symbols: they are the ones a loader can resolve.
        // Native addons written in Rust keep thousands of internal
        // mangled symbols, which would drown the inventory.
        for symbol in object.dynamic_symbols() {
            if !symbol.is_definition() {
                continue;
            }
            let Ok(name) = symbol.name() else { continue };
            if name.is_empty() {
                continue;
            }
            if name == "napi_register_module_v1" {
                napi = true;
            }
            if !exports.iter().any(|existing| existing == name) {
                exports.push(name.to_string());
            }
        }
    }

    NativeModule {
        package: package.to_string(),
        file: path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default(),
        size,
        napi,
        exports,
    }
}
