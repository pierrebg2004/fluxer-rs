# Fluxer-rs

Client de bureau **natif pour Fluxer**, écrit intégralement en **Rust**, sans
Electron, sans JavaScript et sans WebView. L'interface reprend celle du client
officiel (thème sombre, rail des serveurs, salons, messages, MP, favoris,
vocal et réglages complets).

- **100 % Rust** : protocole REST, passerelle temps réel, rendu et outillage.
- **100 % Vulkan** : rendu via `wgpu`, backend Vulkan exclusif (pas d'OpenGL).
- **100 % Wayland** : `winit` compilé sans X11 ; l'application refuse de
  démarrer hors session Wayland.
- **Interface et documentation en français.**

## Prérequis

- Rust stable récent (`rustup`, édition 2024).
- Une session **Wayland** (`WAYLAND_DISPLAY` défini).
- Un pilote **Vulkan** fonctionnel :
  - AMD : Mesa RADV ;
  - Intel : Mesa ANV ;
  - NVIDIA : pilote propriétaire récent.
- `libwayland` disponible à l'exécution (chargement dynamique, pas de
  `pkg-config` ni de `libwayland-dev` requis à la compilation).

Vérification rapide de Vulkan :

```sh
vulkaninfo --summary
```

## Compilation et lancement

```sh
cd ~/Documents/Fluxer
export PATH="$HOME/.cargo/bin:$PATH"

cargo build --release
./target/release/fluxer
```

En développement :

```sh
cargo run -p fluxer-gui
```

La session est lue depuis `~/.config/fluxer-rs/config.json` (permissions
`0600`). Le bouton « Importer la session Fluxer installée » récupère le jeton
du client officiel sans jamais l'afficher dans les journaux.

## Fonctionnalités

- Connexion par jeton et import depuis le client Electron officiel.
- Découverte d'instance (`.well-known/fluxer`), REST et passerelle WebSocket.
- Rail des serveurs, catégories repliables, salons textuels et vocaux.
- Fil de messages : regroupement, séparateurs de jour, Markdown, pièces
  jointes, défilement et chargement automatique de l'historique.
- Envoi de messages (Entrée pour envoyer, Maj+Entrée pour un saut de ligne),
  réponses citées, édition et suppression avec confirmation.
- Réactions : compteurs, bascule au clic, réactions rapides et temps réel.
- Pièces jointes par glisser-déposer (multipart, 25 Mio maximum).
- Messages épinglés et indicateur dans la timeline.
- Liste des membres, rôles colorés, présences en temps réel (op 14) et carte
  de profil détaillée au clic.
- Messages privés et groupes privés, avatars, présence et non-lus.
- Favoris : enregistrement d'un message, recherche, détail, ouverture du
  salon, suppression, persistance.
- Vocal : adhésion, participants, panneau compact, page d'appel et chat
  textuel du salon vocal.
- Réglages : compte, profil éditable (nom affiché, pronoms, bio via PATCH),
  apparence, discussion, notifications, audio/vidéo, raccourcis, langue,
  intégration Linux, avancé, à propos, avec recherche interne.
- Thème sombre conforme au client officiel, polices Noto Sans et emoji.
- Icône rouge « Rust » pour la fenêtre et le menu des applications.

## Icône et menu des applications

L'icône est générée en Rust à partir du logo officiel, puis teintée en rouge :

```sh
cargo run -p fluxer-gui --example make_icon
cargo run -p fluxer-gui --example install_desktop
```

La seconde commande installe :

- `~/.local/share/icons/hicolor/*/apps/fluxer-rs.png` ;
- `~/.local/share/pixmaps/fluxer-rs.png` ;
- `~/.local/share/applications/fluxer-rs.desktop`.

## Vérifications

```sh
cargo fmt --check
cargo clippy --all-targets
cargo test
```

L'application peut capturer son propre framebuffer (utile sans outil de
capture Wayland) :

```sh
WAYLAND_DISPLAY=wayland-0 \
FLUXER_SCREENSHOT=/tmp/fluxer.png \
./target/debug/fluxer
```

Variables de débogage : `FLUXER_VIEW=dms|favorites|voice`,
`FLUXER_GUILD=<id>`, `FLUXER_SETTINGS=<onglet>`.

## Architecture

```text
fluxer-core   protocole : découverte, REST, passerelle, modèles, session
fluxer-gui    interface : eframe/egui + wgpu (Vulkan) + winit (Wayland)
```

- `crates/fluxer-core/src/` : `api.rs`, `discovery.rs`, `gateway.rs`,
  `models.rs`, `media.rs`, `session.rs`.
- `crates/fluxer-gui/src/` : `main.rs`, `app.rs`, `engine.rs`, `ui.rs`,
  `theme.rs`, `fonts.rs`, `icons.rs`, `markdown.rs`.

La documentation technique détaillée se trouve dans
[`AGENTS.md`](AGENTS.md) et dans le dossier [`rapport/`](rapport/).

## Limites connues

- **L'audio vocal n'est pas encore transporté** : la signalisation WebRTC
  (op 13, SDP/ICE), Opus et CPAL restent à implémenter. L'adhésion, les
  participants et l'interface fonctionnent.
- Le téléversement de pièces jointes et l'édition/suppression de messages ne
  sont pas branchés.
- Les modifications de profil sont un aperçu local ; l'envoi via
  `PATCH /v1/users/@me` reste à faire.

## Licence

Fluxer-rs est distribué sous **GNU Affero General Public License v3.0 ou
ultérieure** (`AGPL-3.0-or-later`). Le texte intégral se trouve dans
[`LICENSE.txt`](LICENSE.txt) ; chaque fichier source porte l'identifiant SPDX
correspondant. L'onglet « À propos » des réglages affiche également l'avis de
licence et l'absence de garantie.

Copyright (C) 2026 Fluxer-rs contributors.

Cette licence est cohérente avec le projet Fluxer amont.
