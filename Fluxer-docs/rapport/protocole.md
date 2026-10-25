# Protocole Fluxer observé

Ce document résume le protocole réellement utilisé par Fluxer-rs. Il a été
vérifié sur l'instance publique `https://api.fluxer.app` en octobre 2026.

## 1. Découverte

```http
GET https://<origine>/.well-known/fluxer
```

Réponse (extrait) :

```json
{
  "endpoints": {
    "api": "https://api.fluxer.app",
    "gateway": "wss://gateway.fluxer.app",
    "media": "https://cdn.fluxer.app"
  }
}
```

Le client ne code jamais un CDN en dur : toutes les URLs dérivent de
`media_base()`.

## 2. Authentification REST

L'en-tête est le jeton **brut**, sans `Bearer` :

```http
Authorization: <jeton>
```

Points d'entrée utilisés :

| Méthode | Chemin | Usage |
|---|---|---|
| GET | `/v1/users/@me` | compte courant |
| GET | `/v1/users/@me/guilds` | serveurs |
| GET | `/v1/guilds/{id}` | détail, rôles, bannière |
| GET | `/v1/guilds/{id}/channels` | salons |
| GET | `/v1/guilds/{id}/members?limit=200` | membres |
| GET | `/v1/channels/{id}/messages?limit=50` | page d'historique |
| GET | `/v1/channels/{id}/messages?limit=50&before={id}` | page précédente |
| POST | `/v1/channels/{id}/messages` | envoi |
| GET | `/v1/users/@me/channels` | MP et groupes privés |

Les messages sont renvoyés **du plus récent au plus ancien**. L'engine inverse
chaque page avant de l'envoyer à l'interface, puis pagine avec `before`.

Les types de salons observés :

| Valeur | Type |
|---|---|
| 0 | textuel |
| 2 | vocal |
| 4 | catégorie |
| 5 | annonces |
| 1 / 3 | MP / groupe privé |

## 3. Passerelle

URL :

```text
wss://gateway.fluxer.app/?v=1&encoding=json&stream=1
```

Le flux est du **JSON simple** (pas d'ETF), éventuellement compressé côté
transport selon les options ; le client demande `stream=1`.

### Opcodes

| Op | Sens | Rôle |
|---|---|---|
| 0 | réception | dispatch |
| 1 / 11 | aller-retour | heartbeat / ACK |
| 2 | envoi | `IDENTIFY` |
| 4 | envoi | rejoindre/quitter un vocal |
| 6 | envoi | `RESUME` |
| 10 | réception | `HELLO` (intervalle de heartbeat) |
| 14 | envoi | souscription membres + présences |

### Identifications

```json
{
  "op": 2,
  "d": {
    "token": "<jeton>",
    "capabilities": 0,
    "properties": { "os": "linux", "browser": "Fluxer-rs" }
  }
}
```

Après `READY`, le client conserve la séquence et l'identifiant de session pour
pouvoir reprendre :

```json
{ "op": 6, "d": { "token": "<jeton>", "session_id": "…", "seq": 42 } }
```

### Heartbeat

`HELLO` fournit `heartbeat_interval`. Le client envoie ensuite :

```json
{ "op": 1, "d": 42 }
```

et vérifie les ACK (`op 11`). En cas de silence prolongé, la connexion est
considérée comme morte et relancée avec un backoff exponentiel.

## 4. Présences

La passerelle ne pousse par défaut que les présences des amis et des
destinataires de MP. Pour un serveur, il faut souscrire explicitement :

```json
{
  "op": 14,
  "d": {
    "subscriptions": {
      "<guild_id>": { "members": ["<user_id>", "…"] }
    }
  }
}
```

Contraintes observées :

- **100 identifiants maximum** par serveur ;
- une souscription remplace la précédente : l'engine libère l'ancien serveur
  avant d'en souscrire un nouveau ;
- `READY` contient aussi un tableau `presences` global (amis / MP), traité
  séparément.

Sans op 14, tous les membres d'un serveur apparaissent hors ligne.

## 5. Vocal

### Rejoindre

```json
{
  "op": 4,
  "d": {
    "guild_id": "<guild>",
    "channel_id": "<salon>",
    "self_mute": false,
    "self_deaf": false
  }
}
```

### Quitter

Même paquet avec `channel_id: null`.

### Réponses

1. `VOICE_STATE_UPDATE` : participants, mute, sourdine, identifiant de session.
2. `VOICE_SERVER_UPDATE` : endpoint du serveur vocal et jeton ICE.

Le client officiel utilise un **maillage WebRTC** : chaque pair ouvre une
connexion avec les autres, la signalisation passant par l'opcode 13
(SDP/ICE). Les serveurs ICE sont fournis par `VOICE_SERVER_UPDATE`.

Fluxer-rs implémente l'adhésion, les participants et l'interface. Le transport
audio (WebRTC, Opus, CPAL) reste à faire ; aucune promesse d'audio ne doit être
faite avant des tests réels.

## 6. Messages et rendu

Un message contient au minimum :

```json
{
  "id": "…",
  "channel_id": "…",
  "author": { "id": "…", "username": "…", "global_name": "…" },
  "content": "…",
  "timestamp": "2026-10-23T21:00:00.000Z",
  "attachments": []
}
```

L'interface :

- déduplique par `id` (`MESSAGE_CREATE` peut précéder la fin du chargement) ;
- regroupe les messages consécutifs du même auteur ;
- insère un séparateur de jour ;
- rend le Markdown avec `markdown.rs`.

## 7. Erreurs et reconnexion

Les erreurs REST sont non fatales : elles sont journalisées et affichées dans
la barre d'état. La passerelle se reconnecte indéfiniment ; `RESUME` est tenté
en premier, sinon une nouvelle identification est effectuée.

## 8. Pièges

- Jeton brut, sans préfixe `Bearer`.
- Snowflakes en chaînes, jamais en entiers.
- Dates RFC 3339, jamais l'heure locale brute.
- Messages du plus récent au plus ancien.
- CDN fourni par la découverte, jamais codé en dur.
- Limite de 100 identifiants pour op 14.
