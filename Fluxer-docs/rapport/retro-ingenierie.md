# Rétro-ingénierie du client Fluxer

## Objectif

Comprendre le comportement du client officiel Electron (`/opt/Fluxer`) pour
le reproduire fidèlement en Rust, sans réutiliser son code JavaScript ni son
moteur Chromium.

## Méthode

1. **Observation réseau** : le client officiel tourne sous systemd utilisateur.
   Ses échanges REST et WebSocket ont été observés sans intercepter de secret :
   seuls les noms d'événements, les chemins et les schémas utiles ont été
   conservés.
2. **Lecture du dépôt amont** : clone sparce en lecture seule dans
   `/tmp/opencode/fluxer-src` (`fluxer_gateway`, `fluxer_api/src`,
   `fluxer_app/src`). Il sert de référence pour les noms de champs et les
   opcodes, pas de code source réutilisé.
3. **Sondes Rust** : des exemples du crate `fluxer-core` reproduisent chaque
   étape (découverte, REST, passerelle, présences) et vérifient les hypothèses
   en direct.
4. **Analyse statique des modules natifs** : les modules N-API du client
   Electron ont été inventoriés avec `native_exports` (analyse de symboles
   dynamiques via la crate `object`). Pour les binaires plus complexes,
   l'assistant **ReVa** (Reverse Engineering Assistant de Ghidra) peut être
   utilisé hors du dépôt : il aide à retrouver les structures et les appels
   d'initialisation sans introduire d'outil non-Rust dans le projet.
5. **Validation visuelle** : captures internes de Fluxer-rs comparées aux
   captures du client officiel (thème, proportions, libellés).

## Sources particulièrement utiles

| Sujet | Fichier amont |
|---|---|
| souscriptions op 14 | `fluxer_gateway/src/guild/guild_unified_subscriptions.erl` |
| présences de masse | `fluxer_app/src/features/presence/state/MemberPresenceSubscription.ts` |
| liste des membres | `fluxer_app/src/features/member/state/MemberSidebar.ts` |
| vocal (maillage) | `fluxer_app/src/features/voice/engine/v2/…`, `VoiceMeshPeer.ts` |
| paramètres | `fluxer_app/src/features/user/components/modals/tabs/` |
| routage des événements | `fluxer_app/src/features/gateway/events/EventRouter.ts` |

## Enseignements clés

- L'authentification REST utilise le jeton **brut**, sans `Bearer`.
- Les présences ne sont pas poussées pour tous les membres d'un serveur : il
  faut souscrire avec l'opcode 14, par lots de 100 identifiants.
- Les messages sont paginés du plus récent au plus ancien (`before`).
- Le vocal n'est pas un serveur média central : c'est un **maillage WebRTC**
  signalé par l'opcode 13, avec des serveurs ICE fournis par
  `VOICE_SERVER_UPDATE`.
- Le client officiel stocke sa session dans une base SQLite
  (`desktop-app-store.sqlite3`) verrouillée : toute lecture doit se faire sur
  une copie.
- Les identifiants sont des snowflakes sous forme de chaînes.

## Outillage interne

Tous les outils du dépôt sont des binaires ou exemples **Rust** :

```sh
cargo run -p fluxer-core --example probe
cargo run -p fluxer-core --example gwpeek
cargo run -p fluxer-core --example gwstatus
cargo run -p fluxer-core --example presencecheck
cargo run -p fluxer-core --example native_exports
cargo run -p fluxer-gui --example make_icon
cargo run -p fluxer-gui --example install_desktop
```

Aucun script Python, JavaScript ou shell n'est requis pour analyser, générer
ou installer le client.

## Limites et éthique

- Aucun secret n'est journalisé : les sondes n'affichent que des compteurs,
  des longueurs et des noms d'événements.
- Le client officiel n'est ni modifié ni arrêté par l'outillage.
- Le code produit est original ; le dépôt amont sert uniquement de
  documentation de protocole.
- Le projet est distribué sous AGPL-3.0-or-later, en cohérence avec l'amont.
