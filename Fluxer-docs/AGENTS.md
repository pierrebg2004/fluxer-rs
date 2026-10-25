# AGENTS.md — guide de reprise pour une IA

Ce fichier décrit le projet **Fluxer-rs**, son architecture, ses conventions et
l'état exact du travail. Lis-le intégralement avant de modifier du code.

---

## 1. Mission

Construire un **client de bureau natif pour Fluxer**, écrit **100 % en Rust**,
dans `~/Documents/Fluxer`.

- Remplace le client officiel Electron situé dans `/opt/Fluxer`.
- Objectif fonctionnel : **tout Fluxer en Rust** — serveurs, salons, messages,
  messages privés, historique, favoris, vocal, paramètres complets.
- L'interface doit ressembler au client officiel (thème sombre, mêmes
  proportions, mêmes couleurs).
- Toute l'interface **et toute la documentation** sont en **français**.
- Le livrable final ne contient que des fichiers `.rs`, `.toml`, `.md`, `.txt`
  (et éventuellement des polices). Pas de JavaScript, TypeScript, Python,
  Electron ni WebView, même pour l'outillage.

### Règles absolues

1. **Aucun fichier `.js`, `.ts`, `.tsx`, `.cjs`, `.mjs` ou `.py` dans le dépôt.**
   Les scripts d'analyse doivent être des binaires/exemples Rust.
2. Une seule instance de l'application à la fois (pas de fenêtres multiples).
3. Ne jamais afficher, journaliser ou committer le jeton de session.
4. Les commentaires et libellés d'interface sont en français ; les identifiants
   de code restent en anglais.
5. Ne pas réintroduire `_extracted/` (ancien dépaquetage du client Electron, supprimé).
6. Tout le code est sous **AGPL-3.0-or-later** : conserver l'en-tête SPDX de
   chaque fichier `.rs`, `LICENSE.txt` et l'avis affiché dans les réglages.

---

## 2. Démarrage rapide

```sh
cd ~/Documents/Fluxer
export PATH="$HOME/.cargo/bin:$PATH"

# Compilation
cargo build

# Lancement — 100 % Wayland, 100 % Vulkan (aucun repli X11/OpenGL)
WAYLAND_DISPLAY=wayland-0 ./target/debug/fluxer

# Tests et vérification statique
cargo test
cargo clippy --all-targets
cargo fmt --check
```

Prérequis : session Wayland, pilote Vulkan (Mesa RADV/ANV, NVIDIA, etc.) et
`libwayland` chargeable dynamiquement. Aucun paquet `pkg-config` n'est requis
à la compilation. L'application refuse de démarrer si `WAYLAND_DISPLAY` est
absent plutôt que de basculer sur X11.

Le jeton est lu depuis `~/.config/fluxer-rs/config.json` (permissions `0600`).
Il peut être importé automatiquement depuis le client officiel via le bouton
« Importer la session Fluxer installée » ou `Command::ImportInstalled`.

### Capture d'écran de contrôle

L'application sait capturer son propre framebuffer (pur Rust, indépendant du
compositeur) :

```sh
FLUXER_SCREENSHOT=/tmp/opencode/capture.png ./target/debug/fluxer
```

La capture a lieu après ~8 s. Elle peut accumuler des images fantômes sur les
textes dynamiques ; l'interface statique reste fiable. Pour tuer uniquement
**notre** instance :

```sh
for p in $(pgrep -x fluxer); do
  case "$(readlink /proc/$p/exe)" in
    *Documents/Fluxer/target/debug/fluxer) kill "$p";;
  esac
done
```

Ne jamais utiliser `pkill -f fluxer` : cela tue aussi le shell appelant et le
client officiel.

---

## 3. Arborescence

```text
Fluxer/
├── AGENTS.md                  ← ce fichier
├── README.md                  ← présentation utilisateur (à compléter)
├── Cargo.toml                 ← workspace + dépendances partagées
├── crates/
│   ├── fluxer-core/           ← bibliothèque protocole, sans interface
│   │   ├── src/
│   │   │   ├── api.rs         ← client REST reqwest
│   │   │   ├── discovery.rs   ← /.well-known/fluxer, endpoints
│   │   │   ├── error.rs       ← erreurs + messages français
│   │   │   ├── gateway.rs     ← WebSocket temps réel, heartbeat, reprise
│   │   │   ├── media.rs       ← URLs CDN (avatars, icônes, bannières)
│   │   │   ├── models.rs      ← User, Guild, Channel, Message, Member…
│   │   │   └── session.rs     ← Config persistée + import client officiel
│   │   └── examples/          ← sondes et outils d'analyse (Rust uniquement)
│   └── fluxer-gui/            ← application eframe/egui
│       └── src/
│           ├── main.rs        ← point d'entrée, polices, TLS, viewport
│           ├── app.rs         ← état applicatif + pompe de mises à jour
│           ├── engine.rs      ← moteur async (REST + passerelle)
│           ├── ui.rs          ← tout le dessin egui
│           ├── theme.rs       ← palette et dimensions officielles
│           ├── fonts.rs       ← Noto Sans / Noto Sans Mono / emoji
│           ├── icons.rs       ← icônes vectorielles dessinées à la main
│           └── markdown.rs    ← Markdown → LayoutJob egui
├── rapport/                   ← documentation technique
├── assets/fluxer-rs.png       ← icône rouge générée en Rust
└── target/                    ← sortie de compilation (non livrée)
```

---

## 4. Architecture

### 4.1 `fluxer-core`

Aucune dépendance à egui. Tout est asynchrone (tokio).

- `discovery::discover(http, origine)` interroge
  `https://<origine>/.well-known/fluxer` et renvoie :
  - `api_base` (REST)
  - `gateway` (WebSocket)
  - `media_base()` (CDN)
- `ApiClient::new(&endpoints, token)` : `Authorization: <token>` **brut**,
  sans préfixe `Bearer`.
- `session::Config` : préférences + jeton, sérialisées en JSON.
- `gateway::run_gateway(url, token, events, outgoing, cancel)` :
  - se reconnecte indéfiniment avec backoff ;
  - gère `HELLO`, heartbeat, `IDENTIFY`, `RESUME`, séquence ;
  - `outgoing` est un canal d'écriture : tout `serde_json::Value` envoyé y est
    transmis au socket une fois la session identifiée.
  - événements exposés via `GatewayEvent` (`Ready`, `Dispatch`, …).

### 4.2 `fluxer-gui`

Deux mondes communiquent par messages :

```text
UI (egui, thread principal)
   │ Command  ────────────────▶  Engine (thread tokio dédié)
   │                                  │ REST (reqwest)
   │                                  │ Passerelle (thread tokio)
   ◀────────────── Update ────────────┘
```

- `engine.rs` possède `ApiClient`, l'état réseau et le canal `outgoing`.
- `app.rs` conserve l'état d'affichage, applique les `Update`, persiste les
  préférences et expose les actions (`toggle_favorite`, `open_favorite`…).
- `ui.rs` ne fait que dessiner et émettre des `Command`.

**Ne jamais bloquer le thread UI** : tout appel réseau passe par l'engine.

### 4.3 Rendu 100 % Vulkan, plateforme 100 % Wayland

Le backend graphique est **wgpu sur Vulkan uniquement** :

- `eframe` est compilé avec `wgpu_no_default_features` (pas de `glow`) ;
- `wgpu` est déclaré avec `default-features = false` et seulement
  `std`, `vulkan`, `wgsl` — ni GLES/OpenGL, ni Metal, ni DX12, ni WebGPU ;
- `NativeOptions::renderer = Renderer::Wgpu` ;
- `wgpu_options.wgpu_setup` force `Backends::VULKAN` et
  `PowerPreference::HighPerformance`.

Côté plateforme, `winit` est compilé avec `wayland` + `wayland-dlopen` et
**sans `x11`**. `main.rs` positionne `WINIT_UNIX_BACKEND=wayland` et refuse de
démarrer sans `WAYLAND_DISPLAY`. `wayland-dlopen` évite toute dépendance
`pkg-config`/`libwayland-dev` à la compilation.

---

## 5. Protocole Fluxer (vérifié en direct)

### 5.1 Authentification

En-tête `Authorization` contenant **le jeton brut**. Instances :
`https://api.fluxer.app` par défaut, découverte sur `/.well-known/fluxer`.

### 5.2 REST utile

| Méthode | Chemin | Usage |
|---|---|---|
| GET | `/v1/users/@me` | utilisateur courant |
| GET | `/v1/users/@me/guilds` | serveurs |
| GET | `/v1/guilds/{id}` | détail serveur (rôles, compteurs, bannière) |
| GET | `/v1/guilds/{id}/channels` | salons |
| GET | `/v1/guilds/{id}/members?limit=200` | membres |
| GET | `/v1/channels/{id}/messages?limit=50[&before=id]` | historique |
| POST | `/v1/channels/{id}/messages` | envoi (JSON ou multipart) |
| PATCH | `/v1/channels/{id}/messages/{mid}` | édition |
| DELETE | `/v1/channels/{id}/messages/{mid}` | suppression |
| PUT | `/v1/channels/{id}/messages/{mid}/reactions/{emoji}/@me` | ajouter une réaction |
| DELETE | `/v1/channels/{id}/messages/{mid}/reactions/{emoji}/@me` | retirer sa réaction |
| PUT/DELETE | `/v1/channels/{id}/pins/{mid}` | épingler / désépingler |
| PATCH | `/v1/users/@me` | nom affiché, pronoms, bio |
| GET | `/v1/users/@me/channels` | MP et groupes privés |

Les messages arrivent **du plus récent au plus ancien** : le moteur inverse la
page avant de l'envoyer à l'UI.

### 5.3 Passerelle

URL : `wss://…?v=1&encoding=json&stream=1` (JSON simple, pas ETF).

Opcodes utilisés :

| Op | Sens | Rôle |
|---|---|---|
| 0 | réception | dispatch (`READY`, `MESSAGE_CREATE`, `PRESENCE_UPDATE`, `VOICE_STATE_UPDATE`, …) |
| 1 / 11 | aller-retour | heartbeat / ACK |
| 2 | envoi | `IDENTIFY` |
| 4 | envoi | rejoindre/quitter un salon vocal |
| 6 | envoi | `RESUME` |
| 10 | réception | `HELLO` (intervalle de heartbeat) |
| 14 | envoi | souscription paresseuse (membres + présences) |

### 5.4 Présences

Par défaut la passerelle ne pousse que les présences des amis et des
destinataires de MP. Pour une guilde, il faut souscrire :

```json
{"op":14,"d":{"subscriptions":{"<guild_id>":{"members":["<user_id>", "…"]}}}}
```

Limite officielle : **100 identifiants par guilde**. L'engine souscrit après
le chargement des membres (`Engine::subscribe_presences`) et libère l'ancienne
guilde. Sans cela, tous les membres apparaissent hors ligne.

`READY` contient aussi un tableau `presences` global (amis / MP), traité par
`send_presences`.

### 5.5 Vocal

- Rejoindre : op 4

```json
{"op":4,"d":{"guild_id":"…","channel_id":"…","self_mute":false,"self_deaf":false}}
```

- Quitter : même paquet avec `channel_id: null`.
- La passerelle répond `VOICE_STATE_UPDATE` (participants) puis
  `VOICE_SERVER_UPDATE` (endpoint + jeton ICE).
- Le client officiel utilise un **maillage WebRTC** (`VoiceMeshPeer.ts`),
  signalisation via op 13, ICE serveurs dans `VOICE_SERVER_UPDATE`.

**État actuel** : l'adhésion, les participants et l'UI vocale sont câblés ;
le transport audio natif (WebRTC/Opus/CPAL) reste à implémenter. Voir §10.

---

## 6. Interface et egui 0.37

### 6.1 Structure visuelle

```text
┌────┬──────────────┬──────────────────────────────┬──────────┐
│rail│ liste salons │ en-tête (salon + recherche)  │ membres  │
│72px│    320px     ├──────────────────────────────┤  264px   │
│    │              │ timeline                     │          │
│    │              ├──────────────────────────────┤          │
│    │ panneau user │ composeur                    │          │
└────┴──────────────┴──────────────────────────────┴──────────┘
```

Vues prévues : `View::Guilds`, `View::Dms`, `View::Favorites`.

### 6.2 Pièges egui 0.37

- Panneaux : `Panel::{left,right,top,bottom}(id).show(ui, |ui| …)`.
- `Ui::id()` n'existe plus → utiliser `ui.scope_id()`.
- `Color32` est **prémultiplié** : un blanc translucide d'alpha `a` s'écrit
  `Color32::from_rgba_premultiplied(a, a, a, a)`.
- `from_rgba_unmultiplied` **n'est pas `const`** : impossible dans une
  constante. Pour les surcouches constantes, utiliser la forme prémultipliée.
- `LayoutSection.byte_range` utilise `ByteIndex` (`.start.0..end.0`).
- `Visuals` n'a pas `clone_text_style` dans cette version.
- `ScrollArea::show` renvoie un `ScrollAreaOutput` avec `state.offset` :
  c'est ce qui permet la pagination automatique en haut de timeline.
- Pour un libellé tronqué : `egui::Label::new(...).truncate()`.

### 6.3 Polices

- Noto Sans Regular / Bold / Italic / BoldItalic + Noto Sans Mono.
- Familles nommées : `fluxer-bold`, `fluxer-italic`, `fluxer-bold-italic`.
- **L'emoji doit être ajouté après les polices texte**, sinon son glyphe
  d'espace large étire l'interlignage des mots.

### 6.4 Couleurs

Source : feuille de style du client officiel (voir en-tête de `theme.rs`).
Le fond principal est `#0b0b0c`, la barre latérale `#18181a`, l'accent
`theme::ACCENT`. Ne pas inventer de couleurs : réutiliser les constantes.

---

## 7. Persistance

Fichier : `~/.config/fluxer-rs/config.json` (créé en `0600`).

Champs de `session::Config` :

| Champ | Type | Rôle |
|---|---|---|
| `instance` | string | origine de découverte |
| `token` | string | jeton (secret) |
| `user_id` | string? | compte connecté |
| `compact` | bool | mode compact |
| `show_members` | bool | colonne des membres |
| `enable_favorites` | bool | afficher les favoris |
| `favorites` | `SavedMessage[]` | messages enregistrés |
| `notifications` | bool | notifications activées |
| `notification_sounds` | bool | sons |
| `language` | string | `fr`, `en`, … |
| `ui_scale` | f32 | échelle d'interface |
| `reduce_motion` | bool | animations réduites |
| `voice_muted` / `voice_deafened` | bool | état vocal par défaut |

`SavedMessage` : `message_id`, `channel_id`, `channel_label`, `author`,
`content`, `timestamp`.

Toujours sauvegarder via `session::save(&config)` (préserve les permissions).
L'import officiel lit une **copie** de
`~/.config/fluxer/desktop-app-store.sqlite3` : ne jamais ouvrir la base vive.

---

## 8. Débogage

```sh
# Journalisation détaillée
RUST_LOG=debug ./target/debug/fluxer

# Sonde de découverte / REST / READY
cargo run -p fluxer-core --example probe

# Observation de la passerelle
cargo run -p fluxer-core --example gwpeek

# Inventaire des modules natifs du client officiel
cargo run -p fluxer-core --example native_exports
```

Les exemples `probe`, `gwpeek`, `gwstatus`, `presencecheck` sont des outils de
rétro-ingénierie temporaires mais **doivent rester en Rust**.

Le client officiel tourne sous systemd utilisateur et reprend le focus :
ne pas le lancer/arrêter, ne pas compter sur `spectacle` (il capture souvent le
terminal actif). Utiliser `FLUXER_SCREENSHOT`.

---

## 9. Source de référence

Le dépôt officiel est cloné en lecture seule :

```sh
/tmp/opencode/fluxer-src
```

Clone sparce : `fluxer_gateway`, `fluxer_api/src`, `fluxer_app/src`.

Fichiers particulièrement utiles :

| Sujet | Fichier |
|---|---|
| souscriptions op 14 | `fluxer_gateway/src/guild/guild_unified_subscriptions.erl` |
| présences de masse | `fluxer_app/src/features/presence/state/MemberPresenceSubscription.ts` |
| liste des membres | `fluxer_app/src/features/member/state/MemberSidebar.ts` |
| vocal (maillage) | `fluxer_app/src/features/voice/engine/v2/…`, `VoiceMeshPeer.ts` |
| paramètres | `fluxer_app/src/features/user/components/modals/tabs/` |
| dispatch passerelle | `fluxer_app/src/features/gateway/events/EventRouter.ts` |

Licence du projet : **AGPL-3.0-or-later**, cohérente avec l'amont.

---

## 10. État d'avancement

### Terminé et vérifié en direct

- Découverte d'instance, REST, passerelle, `READY`.
- Connexion par jeton + import depuis le client officiel.
- Rail des serveurs, liste des salons avec catégories repliables.
- En-tête, bannière, sujet, liste des membres avec rôles/couleurs.
- Timeline : regroupement des messages, séparateurs de jour, Markdown,
  pièces jointes, mode compact.
- Composeur (Entrée / Maj+Entrée), envoi REST + écho temps réel.
- Présences via op 14 (vérifié : 16 membres en ligne).
- Thème, polices, icônes, écran de connexion, fenêtre de paramètres de base.
- Modèles MP (`ChannelKind::{DirectMessage, GroupDm}`, `recipients`,
  `direct_label()`), endpoint `/v1/users/@me/channels`.
- Vue `View::Dms` (avatars, présence, non-lus, sélection automatique).
- Vue `View::Favorites` (liste, recherche, détail, ouverture du salon,
  suppression, persistance, entrée de menu contextuel).
- Historique infini avec chargement automatique près du haut (`load_older_armed`).
- Réglages complets (~11 onglets, recherche, groupes, aperçu de profil,
  persistance des bascules).
- Vocal : adhésion op 4, `VOICE_STATE_UPDATE`, panneau compact, page d'appel,
  chat textuel du salon vocal, participants sous le salon dans la barre latérale.
- Rendu **100 % Vulkan** (wgpu) et plateforme **100 % Wayland** (aucun X11,
  aucun OpenGL compilé).
- Icône de fenêtre + entrée de menu `.desktop` (logo rouge Rust), installées
  par `cargo run -p fluxer-gui --example install_desktop`.
- Markdown : correctif des marqueurs `**gras**`, `*italique*`, `~~barré~~`
  (le contenu entre marqueurs n'est plus avalé).
- Édition et suppression de messages (menu contextuel + modale de confirmation).
- Réactions : affichage des compteurs, bascule au clic, menu de réactions
  rapides, mise à jour temps réel via `MESSAGE_REACTION_ADD/REMOVE`.
- Réponses citées (`message_reference`) avec bandeau dans le composeur et
  aperçu dans la timeline.
- Pièces jointes : glisser-déposer (25 Mio max) et envoi `multipart`
  (`payload_json` + `files[0]`).
- Messages épinglés (épingler/désépingler + indicateur).
- Carte de profil au clic sur un membre (avatar, identifiant, pronoms, bio,
  date d'arrivée, rôles).
- `PATCH /v1/users/@me` pour le nom affiché, les pronoms et la bio.
- Chargement automatique de l'historique au défilement (sans bouton), comme
  sur Fluxer.
- `cargo fmt`, `cargo clippy --all-targets` et `cargo test` sans avertissement.

### Limites connues

- **Audio vocal non implémenté** : la signalisation WebRTC (op 13, SDP/ICE),
  l'encodage Opus et CPAL restent à faire. L'UI et l'adhésion fonctionnent.
- Gestion des rôles, salons de forum et fils de discussion non branchés.
- Les avatars et bannières ne peuvent pas encore être téléversés.

### À faire ensuite

- Audio vocal natif : WebRTC (SDP/ICE via op 13), Opus, CPAL.
- Téléversement d'avatar et de bannière.
- Gestion des rôles et permissions, salons de forum, fils.
- Tests d'intégration supplémentaires (favoris, historique complet, MP).

---

## 11. Conventions de contribution (IA)

1. **Lire avant d'écrire** : `app.rs`, `engine.rs`, `ui.rs` sont interdépendants.
2. Toute nouvelle fonctionnalité suit la chaîne :
   `Command` → traitement engine → `Update` → `FluxerApp::apply` → `ui.rs`.
3. Ne pas dupliquer l'état : les préférences vivent dans `State` + `Config`,
   l'état réseau dans `Engine`.
4. Les erreurs réseau sont non fatales : `tracing::warn!` + message UI.
5. Sauvegarder la configuration après chaque bascule persistante.
6. Ajouter des commentaires **en français**, expliquant le *pourquoi*.
7. Compiler et lancer **une seule** instance avant de conclure.
8. Documenter toute limite réelle (notamment l'audio) plutôt que de la masquer.
9. Aucun secret dans le code, les logs, les captures ou les commits.
10. En fin de tâche : `cargo fmt`, `cargo test`, `cargo clippy --all-targets`.

---

## 12. Pièges connus

- `reqwest` et `tokio-tungstenite` embarquent chacun rustls : installer
  explicitement `rustls::crypto::ring::default_provider()` au démarrage.
- Le CDN média est renvoyé par la découverte : toujours préfixer les URLs via
  `media_base`, jamais coder `cdn.fluxer.app` en dur.
- Les identifiants sont des snowflakes **chaînes** : ne pas les convertir en
  entiers.
- Les timestamps sont RFC 3339 : utiliser `chrono`, jamais l'heure locale brute.
- La base SQLite officielle est verrouillée : copier avant lecture.
- Wayland + egui : la capture interne est plus fiable que l'outil système.
- `wgpu` sans le feature `gles` n'active plus `wayland-backend/dlopen` via
  GLES : il faut déclarer explicitement `winit` avec `wayland-dlopen`, sinon
  `wayland-sys` réclame `pkg-config` et `libwayland-client`.
- Forcer `WAYLAND_DISPLAY` vide fait sortir winit immédiatement : garder la
  variable du compositeur, même si `DISPLAY` est aussi présent.
- Une capture trop précoce (< 8 s) ne contient pas encore les données réseau.
- Ne pas définir `FLUXER_SETTINGS` vide : `std::env::var` renvoie `Ok("")` et
  la fenêtre de réglages s'ouvre sur l'onglet Compte.
- `MESSAGE_CREATE` peut arriver avant la fin du chargement d'un salon :
  l'UI déduplique par identifiant.
