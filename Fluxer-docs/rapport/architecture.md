# Architecture de Fluxer-rs

## Vue d'ensemble

Fluxer-rs est un espace de travail Cargo à deux crates :

```text
fluxer-core   protocole Fluxer, aucune dépendance graphique
fluxer-gui    application native eframe/egui
```

Le client officiel est une application Electron : le processus principal
Node.js, un rendu Chromium et des modules natifs N-API. Fluxer-rs remplace
l'ensemble par un binaire unique :

- HTTP et WebSocket via `reqwest` et `tokio-tungstenite` ;
- temps réel via `tokio` ;
- interface via `egui` ;
- rendu via `wgpu` sur **Vulkan** ;
- fenêtrage via `winit` sur **Wayland**.

Aucun WebView, aucun moteur JavaScript, aucun script Python.

## fluxer-core

`fluxer-core` ne connaît pas egui. Il expose :

| Module | Rôle |
|---|---|
| `discovery` | lecture de `/.well-known/fluxer` : API REST, passerelle, CDN |
| `api` | client REST `reqwest` avec jeton brut |
| `gateway` | WebSocket : `HELLO`, heartbeat, `IDENTIFY`, `RESUME`, reconnexion |
| `models` | `User`, `Guild`, `Channel`, `Message`, `Member`, `Role` |
| `media` | construction des URLs CDN (avatars, icônes, bannières) |
| `session` | `Config` persistant + import du client officiel |
| `error` | erreurs typées avec messages français |

Les identifiants sont des snowflakes **conservés en chaînes**. Les dates sont
des chaînes RFC 3339 converties avec `chrono`.

### Boucle de la passerelle

```text
connexion ──▶ HELLO (op 10) ──▶ heartbeat périodique
      │
      ├── IDENTIFY (op 2) ──▶ READY + dispatches
      │
      └── coupure ──▶ RESUME (op 6) ──▶ sinon reconnexion avec backoff
```

`run_gateway` reçoit deux canaux :

- `events` : événements normalisés (`Ready`, `Dispatch`, fermeture) ;
- `outgoing` : paquets JSON à écrire une fois la session identifiée.

Les souscriptions de présence (op 14) sont envoyées par l'engine après le
chargement des membres, avec la limite officielle de 100 identifiants par
serveur.

## fluxer-gui

### Communication

```text
thread UI (egui)                    thread moteur (tokio)
     │ Command ─────────────────────────────▶ │
     │                                        │ REST + passerelle
     │ ◀───────────────────────────── Update │
```

- `engine.rs` possède le client REST, la passerelle et l'état réseau.
- `app.rs` conserve l'état d'affichage, applique les `Update`, persiste les
  préférences et expose les actions.
- `ui.rs` dessine et émet des `Command` ; il ne bloque jamais sur le réseau.

### Rendu Vulkan

La chaîne graphique est volontairement restreinte :

```toml
eframe = { features = ["wgpu_no_default_features", "wayland", ...] }
wgpu = { default-features = false, features = ["std", "vulkan", "wgsl"] }
winit = { default-features = false, features = ["wayland", "wayland-dlopen", "rwh_06"] }
```

- pas de feature `glow` : aucun contexte OpenGL n'est créé ;
- `wgpu` n'active ni `gles`, ni `metal`, ni `dx12`, ni `webgpu` ;
- `NativeOptions::renderer` vaut `Renderer::Wgpu` ;
- `WgpuConfiguration` force `Backends::VULKAN` et
  `PowerPreference::HighPerformance`.

La sélection de l'adaptateur est visible avec `RUST_LOG=info` :

```text
There are 2 available wgpu adapters: {backend: Vulkan, name: "AMD Radeon 680M (RADV REMBRANDT)" ...}
```

### Plateforme Wayland

- `winit` est compilé **sans** la feature `x11` ;
- `WINIT_UNIX_BACKEND=wayland` est positionné avant toute création de thread ;
- l'absence de `WAYLAND_DISPLAY` provoque un message explicite et un arrêt ;
- `wayland-dlopen` charge `libwayland` à l'exécution, ce qui évite
  `pkg-config` et les en-têtes de développement à la compilation.

### État et préférences

`State` contient les données réseau et d'affichage ; `Config` ne contient que
ce qui doit survivre à un redémarrage :

- jeton et instance ;
- mode compact, liste des membres, favoris ;
- notifications et sons ;
- langue, échelle d'interface, animations réduites ;
- état mute/sourdine du vocal.

Les sauvegardes passent par `session::save`, qui conserve les permissions
`0600` du fichier.

## Interface

```text
┌────┬──────────────┬──────────────────────────────┬──────────┐
│rail│ liste salons │ en-tête (salon + recherche)  │ membres  │
│72px│    320px     ├──────────────────────────────┤  264px   │
│    │              │ timeline                     │          │
│    │              ├──────────────────────────────┤          │
│    │ panneau user │ composeur                    │          │
└────┴──────────────┴──────────────────────────────┴──────────┘
```

Trois vues principales : serveurs (`View::Guilds`), messages privés
(`View::Dms`) et favoris (`View::Favorites`). La fenêtre de réglages
superpose une navigation groupée et un panneau de contenu défilant.

### Polices

Noto Sans Regular/Bold/Italic/BoldItalic, Noto Sans Mono et une police emoji.
Les familles nommées `fluxer-bold`, `fluxer-italic` et `fluxer-bold-italic`
évitent les changements de taille. L'emoji est ajouté **après** les polices
texte pour ne pas étirer l'interlignage.

### Markdown

`markdown.rs` produit un `LayoutJob` egui : gras, italique, souligné, barré,
code inline, blocs de code, citations, titres, listes, liens et mentions
Fluxer (`<@id>`, `<#id>`, `<:nom:id>`). Le parseur est tolérant : un marqueur
non fermé reste littéral.

## Persistance et sécurité

- `~/.config/fluxer-rs/config.json`, permissions `0600`.
- Le jeton n'est jamais journalisé ni affiché.
- L'import officiel lit une **copie** de la base SQLite du client Electron ;
  la base vive n'est jamais ouverte.

## Débogage

- `FLUXER_SCREENSHOT=/chemin.png` : capture interne après 8 s.
- `FLUXER_VIEW=dms|favorites|voice` : vue forcée.
- `FLUXER_GUILD=<id>` : serveur forcé.
- `FLUXER_SETTINGS=<onglet>` : ouvre les réglages sur un onglet.
- `RUST_LOG=debug` : journaux détaillés.

La capture interne est indépendante du compositeur Wayland, mais elle peut
montrer des images fantômes sur les textes dynamiques.
