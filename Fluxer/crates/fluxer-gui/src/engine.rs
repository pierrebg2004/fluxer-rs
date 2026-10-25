// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Fluxer-rs contributors
//
// Ce fichier fait partie de Fluxer-rs, distribué sous AGPL-3.0-or-later.
// Voir LICENSE.txt pour le texte intégral.

//! Background engine: owns the HTTP client and the gateway task, talks to the
//! UI through plain message passing. Everything here is async Rust.

use std::collections::HashMap;
use std::sync::mpsc::Sender;

use eframe::egui;
use fluxer_core::gateway::{GatewayEvent, run_gateway};
use fluxer_core::{
    ApiClient, Channel, Config, Emoji, Guild, GuildDetail, Member, Message, User, discovery,
    session,
};
use fluxer_voice::{IceServer, Mesh, MeshConfig, MeshEvent, Sfu, SfuConfig, SfuEvent};
use serde_json::{Value, json};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio_util::sync::CancellationToken;

const PAGE_SIZE: u16 = 50;

#[derive(Debug)]
pub enum Command {
    Login {
        instance: String,
        token: String,
    },
    ImportInstalled,
    SelectGuild(String),
    SelectChannel(String),
    SelectDm(String),
    SendMessage {
        content: String,
        reply_to: Option<String>,
    },
    EditMessage {
        channel_id: String,
        message_id: String,
        content: String,
    },
    DeleteMessage {
        channel_id: String,
        message_id: String,
    },
    ToggleReaction {
        channel_id: String,
        message_id: String,
        emoji: String,
        active: bool,
    },
    UploadAttachment {
        channel_id: String,
        filename: String,
        content_type: Option<String>,
        bytes: Vec<u8>,
        content: String,
    },
    SetPinned {
        channel_id: String,
        message_id: String,
        pinned: bool,
    },
    IndicateTyping {
        channel_id: String,
    },
    StartThread {
        channel_id: String,
        message_id: String,
        name: String,
    },
    StartForumPost {
        channel_id: String,
        name: String,
        content: String,
    },
    JoinInvite {
        code: String,
    },
    /// Récupère l'aperçu d'une invitation citée dans un message.
    LoadInvite {
        code: String,
    },
    LoadDiscovery {
        offset: u16,
    },
    JoinDiscoveryGuild {
        guild_id: String,
    },
    UpdateProfile {
        display_name: Option<String>,
        pronouns: Option<String>,
        bio: Option<String>,
        avatar: Option<Option<String>>,
        banner: Option<Option<String>>,
    },
    LoadOlder,
    Refresh,
    Logout,
    JoinVoice {
        guild_id: String,
        channel_id: String,
    },
    LeaveVoice {
        guild_id: String,
    },
    SetVoiceState {
        muted: bool,
        deafened: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayStatus {
    Offline,
    Connecting,
    Online,
    Reconnecting,
}

#[derive(Debug)]
pub enum Update {
    Status(String),
    Error(String),
    LoggedIn(User),
    MediaBase(String),
    Guilds(Vec<Guild>),
    GuildDetail(Box<GuildDetail>),
    Members(Vec<Member>),
    Channels(Vec<Channel>),
    Messages {
        channel_id: String,
        messages: Vec<Message>,
        prepend: bool,
    },
    HasMoreBefore(bool),
    MessageCreated(Message),
    MessageUpdated(Message),
    MessageDeleted {
        channel_id: String,
        message_id: String,
    },
    /// Réaction ajoutée ou retirée. `delta` vaut +1 ou -1 ; `me` indique si
    /// l'auteur de l'action est l'utilisateur courant.
    ReactionChanged {
        channel_id: String,
        message_id: String,
        emoji: Emoji,
        delta: i64,
        me: Option<bool>,
    },
    ReactionRemovedAll {
        channel_id: String,
        message_id: String,
    },
    MessagePinned {
        channel_id: String,
        message_id: String,
        pinned: bool,
    },
    ProfileUpdated(User),
    /// Un membre est en train d'écrire dans un salon.
    TypingStarted {
        channel_id: String,
        user_id: String,
    },
    Presence {
        user_id: String,
        status: String,
    },
    PrivateChannels(Vec<Channel>),
    /// Invitation acceptée : guilde rejointe ou groupe privé ouvert.
    InviteAccepted {
        guild_id: Option<String>,
        dm_channel_id: Option<String>,
    },
    /// Aperçu d'invitation chargé (`None` si le code est invalide/expiré).
    InvitePreview {
        code: String,
        invite: Option<fluxer_core::InvitePreview>,
    },
    Discovery {
        guilds: Vec<fluxer_core::DiscoveryGuild>,
        total: i64,
        append: bool,
    },
    DiscoveryLoading(bool),
    /// Salon vocal actuellement rejoint (`None` après déconnexion vocale).
    Voice {
        guild_id: Option<String>,
        channel_id: Option<String>,
    },
    /// Participants du salon vocal courant.
    VoiceMembers(Vec<String>),
    /// Transport audio réellement connecté (SFU ou maillage).
    VoiceConnected(bool),
    /// État de mute/déafen du client local.
    VoiceSelf {
        muted: bool,
        deafened: bool,
    },
    Gateway(GatewayStatus),
    LoadingChannels(bool),
    LoadingMessages(bool),
    LoadingMembers(bool),
}

pub struct Handle {
    pub commands: UnboundedSender<Command>,
    pub updates: std::sync::mpsc::Receiver<Update>,
}

/// Spawns the engine on a dedicated multi-threaded Tokio runtime.
pub fn spawn(ctx: egui::Context) -> Handle {
    let (cmd_tx, cmd_rx) = unbounded_channel();
    let (update_tx, update_rx) = std::sync::mpsc::channel();

    std::thread::Builder::new()
        .name("fluxer-engine".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("tokio runtime");

            runtime.block_on(async move {
                let mut engine = Engine {
                    http: reqwest::Client::new(),
                    config: session::load(),
                    api: None,
                    me: None,
                    guild: None,
                    channel: None,
                    oldest: None,
                    has_more_before: false,
                    guilds: Vec::new(),
                    channels: Vec::new(),
                    gateway_rx: None,
                    gateway_tx: None,
                    subscribed_guild: None,
                    voice_channel: None,
                    voice_guild: None,
                    voice_members: HashMap::new(),
                    voice_muted: false,
                    voice_deafened: false,
                    voice: None,
                    voice_sfu: None,
                    voice_connection_id: None,
                    voice_peers: HashMap::new(),
                    gateway_cancel: CancellationToken::new(),
                    update_tx,
                    ctx,
                };

                engine.start().await;
                engine.run(cmd_rx).await;
                engine.gateway_cancel.cancel();
            });
        })
        .expect("engine thread");

    Handle {
        commands: cmd_tx,
        updates: update_rx,
    }
}

struct Engine {
    http: reqwest::Client,
    config: Config,
    api: Option<ApiClient>,
    me: Option<User>,
    guild: Option<String>,
    channel: Option<String>,
    oldest: Option<String>,
    has_more_before: bool,
    guilds: Vec<Guild>,
    channels: Vec<Channel>,
    gateway_rx: Option<UnboundedReceiver<GatewayEvent>>,
    /// Canal d'écriture vers la passerelle (op 14, présence…).
    gateway_tx: Option<UnboundedSender<Value>>,
    /// Guilde dont les présences de membres sont actuellement souscrites.
    subscribed_guild: Option<String>,
    voice_channel: Option<String>,
    voice_guild: Option<String>,
    voice_members: HashMap<String, Vec<String>>,
    voice_muted: bool,
    voice_deafened: bool,
    voice: Option<Mesh>,
    /// Session LiveKit (SFU) utilisée pour la voix de guilde.
    voice_sfu: Option<Sfu>,
    /// Identifiant de connexion vocal du client local.
    voice_connection_id: Option<String>,
    /// Participants distants : identifiant de connexion → identifiant utilisateur.
    voice_peers: HashMap<String, String>,
    gateway_cancel: CancellationToken,
    update_tx: Sender<Update>,
    ctx: egui::Context,
}

impl Engine {
    fn send(&self, update: Update) {
        let _ = self.update_tx.send(update);
        self.ctx.request_repaint();
    }

    async fn start(&mut self) {
        if self.config.is_authenticated() {
            let instance = self.config.instance.clone();
            let token = self.config.token.clone();
            self.login(instance, token).await;
        }
    }

    async fn run(&mut self, mut commands: UnboundedReceiver<Command>) {
        let mut gateway_events = self.gateway_rx.take();

        loop {
            tokio::select! {
                command = commands.recv() => {
                    let Some(command) = command else { break };
                    if let Err(err) = self.handle(command, &mut gateway_events).await {
                        self.send(Update::Error(err));
                    }
                }
                Some(event) = recv_gateway(gateway_events.as_mut()) => {
                    self.handle_gateway(event);
                }
            }
        }
    }

    async fn handle(
        &mut self,
        command: Command,
        gateway_events: &mut Option<UnboundedReceiver<GatewayEvent>>,
    ) -> Result<(), String> {
        match command {
            Command::Login { instance, token } => {
                self.login(instance, token).await;
                *gateway_events = self.gateway_rx.take();
            }
            Command::ImportInstalled => {
                let config = session::import_from_installed_fluxer().map_err(|e| e.user_hint())?;
                self.config = config.clone();
                session::save(&config).map_err(|e| e.user_hint())?;
                self.send(Update::Status("session importée".into()));
                self.login(config.instance, config.token).await;
                *gateway_events = self.gateway_rx.take();
            }
            Command::SelectGuild(guild_id) => {
                self.load_guild(&guild_id).await?;
            }
            Command::SelectChannel(channel_id) => {
                self.load_channel(&channel_id).await?;
            }
            Command::SelectDm(channel_id) => {
                self.guild = None;
                self.channels.clear();
                self.send(Update::Members(Vec::new()));
                self.subscribed_guild = None;
                self.load_channel(&channel_id).await?;
            }
            Command::SendMessage { content, reply_to } => {
                let Some(api) = self.api.clone() else {
                    return Err("non connecté".into());
                };
                let Some(channel) = self.channel.clone() else {
                    return Ok(());
                };
                let message = api
                    .send_message(&channel, &content, reply_to.as_deref())
                    .await
                    .map_err(|e| e.user_hint())?;
                self.send(Update::MessageCreated(message));
            }
            Command::EditMessage {
                channel_id,
                message_id,
                content,
            } => {
                let Some(api) = self.api.clone() else {
                    return Err("non connecté".into());
                };
                let message = api
                    .edit_message(&channel_id, &message_id, &content)
                    .await
                    .map_err(|e| e.user_hint())?;
                self.send(Update::MessageUpdated(message));
            }
            Command::DeleteMessage {
                channel_id,
                message_id,
            } => {
                let Some(api) = self.api.clone() else {
                    return Err("non connecté".into());
                };
                api.delete_message(&channel_id, &message_id)
                    .await
                    .map_err(|e| e.user_hint())?;
                self.send(Update::MessageDeleted {
                    channel_id,
                    message_id,
                });
            }
            Command::ToggleReaction {
                channel_id,
                message_id,
                emoji,
                active,
            } => {
                let Some(api) = self.api.clone() else {
                    return Err("non connecté".into());
                };
                if active {
                    api.add_reaction(&channel_id, &message_id, &emoji)
                        .await
                        .map_err(|e| e.user_hint())?;
                } else {
                    api.remove_reaction(&channel_id, &message_id, &emoji)
                        .await
                        .map_err(|e| e.user_hint())?;
                }
                // L'état est appliqué par l'écho `MESSAGE_REACTION_ADD` /
                // `MESSAGE_REACTION_REMOVE` de la passerelle, ce qui évite tout
                // double comptage avec la réponse REST (204 sans corps).
            }
            Command::IndicateTyping { channel_id } => {
                let Some(api) = self.api.clone() else {
                    return Ok(());
                };
                let _ = api.indicate_typing(&channel_id).await;
            }
            Command::StartThread {
                channel_id,
                message_id,
                name,
            } => {
                let Some(api) = self.api.clone() else {
                    return Err("non connecté".into());
                };
                let thread = api
                    .start_thread_from_message(&channel_id, &message_id, &name)
                    .await
                    .map_err(|e| e.user_hint())?;
                self.load_channel(&thread.id).await?;
            }
            Command::StartForumPost {
                channel_id,
                name,
                content,
            } => {
                let Some(api) = self.api.clone() else {
                    return Err("non connecté".into());
                };
                let thread = api
                    .start_forum_post(&channel_id, &name, &content)
                    .await
                    .map_err(|e| e.user_hint())?;
                self.load_channel(&thread.id).await?;
            }
            Command::LoadInvite { code } => {
                let Some(api) = self.api.clone() else {
                    return Err("non connecté".into());
                };
                let invite = api.invite(&code).await.ok();
                self.send(Update::InvitePreview { code, invite });
            }
            Command::JoinInvite { code } => {
                let Some(api) = self.api.clone() else {
                    return Err("non connecté".into());
                };
                let invite = api.accept_invite(&code).await.map_err(|e| e.user_hint())?;
                let guild_id = invite
                    .get("guild")
                    .and_then(|guild| guild.get("id"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let channel = invite.get("channel");
                let dm_channel_id = channel
                    .filter(|channel| {
                        matches!(
                            channel.get("type").and_then(Value::as_i64),
                            Some(1) | Some(3)
                        )
                    })
                    .and_then(|channel| channel.get("id"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                if guild_id.is_some() {
                    match api.guilds().await {
                        Ok(guilds) => {
                            self.guilds = guilds.clone();
                            self.send(Update::Guilds(guilds));
                        }
                        Err(error) => {
                            tracing::warn!("liste des serveurs après invitation : {error}");
                        }
                    }
                } else if dm_channel_id.is_some() {
                    match api.private_channels().await {
                        Ok(channels) => self.send(Update::PrivateChannels(channels)),
                        Err(error) => {
                            tracing::warn!("conversations après invitation : {error}");
                        }
                    }
                }
                self.send(Update::InviteAccepted {
                    guild_id,
                    dm_channel_id,
                });
            }
            Command::LoadDiscovery { offset } => {
                let Some(api) = self.api.clone() else {
                    return Err("non connecté".into());
                };
                self.send(Update::DiscoveryLoading(true));
                let result = api.discovery_guilds(offset).await;
                self.send(Update::DiscoveryLoading(false));
                match result {
                    Ok((guilds, total)) => self.send(Update::Discovery {
                        guilds,
                        total,
                        append: offset > 0,
                    }),
                    Err(error) => return Err(error.user_hint()),
                }
            }
            Command::JoinDiscoveryGuild { guild_id } => {
                let Some(api) = self.api.clone() else {
                    return Err("non connecté".into());
                };
                api.join_discovery_guild(&guild_id)
                    .await
                    .map_err(|e| e.user_hint())?;
                match api.guilds().await {
                    Ok(guilds) => {
                        self.guilds = guilds.clone();
                        self.send(Update::Guilds(guilds));
                    }
                    Err(error) => tracing::warn!("liste des serveurs découverte : {error}"),
                }
                self.send(Update::InviteAccepted {
                    guild_id: Some(guild_id),
                    dm_channel_id: None,
                });
            }
            Command::SetPinned {
                channel_id,
                message_id,
                pinned,
            } => {
                let Some(api) = self.api.clone() else {
                    return Err("non connecté".into());
                };
                api.set_pinned(&channel_id, &message_id, pinned)
                    .await
                    .map_err(|e| e.user_hint())?;
                self.send(Update::MessagePinned {
                    channel_id,
                    message_id,
                    pinned,
                });
            }
            Command::UploadAttachment {
                channel_id,
                filename,
                content_type,
                bytes,
                content,
            } => {
                let Some(api) = self.api.clone() else {
                    return Err("non connecté".into());
                };
                let message = api
                    .upload_attachment(
                        &channel_id,
                        &filename,
                        content_type.as_deref(),
                        bytes,
                        &content,
                    )
                    .await
                    .map_err(|e| e.user_hint())?;
                self.send(Update::MessageCreated(message));
            }
            Command::UpdateProfile {
                display_name,
                pronouns,
                bio,
                avatar,
                banner,
            } => {
                let Some(api) = self.api.clone() else {
                    return Err("non connecté".into());
                };
                let mut body = json!({});
                if let Some(name) = display_name {
                    body["global_name"] = Value::String(name);
                }
                if let Some(pronouns) = pronouns {
                    body["pronouns"] = Value::String(pronouns);
                }
                if let Some(bio) = bio {
                    body["bio"] = Value::String(bio);
                }
                if let Some(avatar) = avatar {
                    body["avatar"] = match avatar {
                        Some(data) => Value::String(data),
                        None => Value::Null,
                    };
                }
                if let Some(banner) = banner {
                    body["banner"] = match banner {
                        Some(data) => Value::String(data),
                        None => Value::Null,
                    };
                }
                let user = api.update_profile(body).await.map_err(|e| e.user_hint())?;
                self.send(Update::ProfileUpdated(user));
            }
            Command::LoadOlder => {
                self.load_older().await?;
            }
            Command::Refresh => {
                if let Some(guild) = self.guild.clone() {
                    self.load_guild(&guild).await?;
                }
            }
            Command::JoinVoice {
                guild_id,
                channel_id,
            } => {
                self.stop_voice();
                self.voice_guild = Some(guild_id.clone());
                self.voice_channel = Some(channel_id.clone());
                self.voice_members.clear();
                self.send(Update::Voice {
                    guild_id: Some(guild_id.clone()),
                    channel_id: Some(channel_id.clone()),
                });
                self.send(Update::VoiceMembers(Vec::new()));
                self.send_voice_state();
                // A voice channel also has a text chat; open it so the call
                // view can display the conversation underneath the stage.
                self.load_channel(&channel_id).await?;
            }
            Command::LeaveVoice { guild_id } => {
                if let Some(tx) = self.gateway_tx.as_ref() {
                    let _ = tx.send(json!({
                        "op": 4,
                        "d": {
                            "guild_id": guild_id,
                            "channel_id": serde_json::Value::Null,
                            "self_mute": true,
                            "self_deaf": true,
                        }
                    }));
                }
                self.stop_voice();
                self.voice_channel = None;
                self.voice_guild = None;
                self.voice_members.clear();
                self.send(Update::Voice {
                    guild_id: None,
                    channel_id: None,
                });
                self.send(Update::VoiceMembers(Vec::new()));
            }
            Command::SetVoiceState { muted, deafened } => {
                self.voice_muted = muted;
                self.voice_deafened = deafened;
                if let Some(mesh) = self.voice.as_ref() {
                    mesh.set_muted(muted);
                    mesh.set_deafened(deafened);
                }
                if let Some(sfu) = self.voice_sfu.as_ref() {
                    sfu.set_muted(muted);
                    sfu.set_deafened(deafened);
                }
                self.send(Update::VoiceSelf { muted, deafened });
                if self.voice_channel.is_some() {
                    self.send_voice_state();
                }
            }
            Command::Logout => {
                self.stop_voice();
                self.gateway_cancel.cancel();
                self.gateway_cancel = CancellationToken::new();
                self.api = None;
                self.me = None;
                self.guild = None;
                self.channel = None;
                self.oldest = None;
                self.guilds.clear();
                self.channels.clear();
                self.gateway_rx = None;
                *gateway_events = None;
                self.config.token.clear();
                let _ = session::save(&self.config);
                self.send(Update::Gateway(GatewayStatus::Offline));
            }
        }
        Ok(())
    }

    async fn login(&mut self, instance: String, token: String) {
        self.send(Update::Status("découverte de l'instance…".into()));
        self.gateway_cancel.cancel();
        self.gateway_cancel = CancellationToken::new();
        self.gateway_rx = None;

        let endpoints = match discovery::discover(&self.http, &instance).await {
            Ok(doc) => doc.endpoints,
            Err(err) => {
                self.send(Update::Error(err.user_hint()));
                return;
            }
        };
        self.send(Update::MediaBase(endpoints.media_base().to_string()));

        let api = match ApiClient::new(&endpoints, token.clone()) {
            Ok(api) => api,
            Err(err) => {
                self.send(Update::Error(err.user_hint()));
                return;
            }
        };

        self.send(Update::Status("connexion…".into()));
        match api.me().await {
            Ok(me) => {
                self.config = Config {
                    instance,
                    token: token.clone(),
                    user_id: Some(me.id.clone()),
                    ..self.config.clone()
                };
                let _ = session::save(&self.config);
                self.me = Some(me.clone());
                self.send(Update::LoggedIn(me));
            }
            Err(err) => {
                self.send(Update::Error(err.user_hint()));
                return;
            }
        }

        self.api = Some(api.clone());

        self.send(Update::Status("chargement des serveurs…".into()));
        match api.guilds().await {
            Ok(guilds) => {
                self.guilds = guilds.clone();
                self.send(Update::Guilds(guilds.clone()));
                if let Some(first) = guilds.first() {
                    let id = first.id.clone();
                    let _ = self.load_guild(&id).await;
                }
            }
            Err(err) => {
                self.send(Update::Error(err.user_hint()));
                return;
            }
        }

        match api.private_channels().await {
            Ok(channels) => self.send(Update::PrivateChannels(channels)),
            Err(err) => tracing::warn!("private channels: {}", err.user_hint()),
        }

        let (tx, rx) = unbounded_channel();
        let (out_tx, out_rx) = unbounded_channel();
        self.gateway_rx = Some(rx);
        self.gateway_tx = Some(out_tx);
        let gateway_url = endpoints.gateway.clone();
        let cancel = self.gateway_cancel.clone();
        self.send(Update::Gateway(GatewayStatus::Connecting));
        tokio::spawn(async move {
            run_gateway(gateway_url, token, tx, out_rx, cancel).await;
        });
        self.send(Update::Status("en ligne".into()));
    }

    async fn load_guild(&mut self, guild_id: &str) -> Result<(), String> {
        let Some(api) = self.api.clone() else {
            return Err("non connecté".into());
        };
        self.guild = Some(guild_id.to_string());
        self.send(Update::LoadingChannels(true));
        self.send(Update::Members(Vec::new()));
        let channels = api.channels(guild_id).await.map_err(|e| e.user_hint())?;
        self.channels = channels.clone();
        self.send(Update::Channels(channels.clone()));
        self.send(Update::LoadingChannels(false));

        if let Some(text) = channels.iter().find(|c| c.kind().is_readable()) {
            let id = text.id.clone();
            self.load_channel(&id).await?;
        } else {
            self.channel = None;
            self.oldest = None;
        }

        self.load_guild_meta(&api, guild_id).await;
        Ok(())
    }

    /// Fetches the rich guild object (roles, counts, banner) and its member
    /// list. Failures are non-fatal: the channel list is already usable.
    async fn load_guild_meta(&mut self, api: &ApiClient, guild_id: &str) {
        self.send(Update::LoadingMembers(true));
        let (detail, members) =
            tokio::join!(api.guild_detail(guild_id), api.members(guild_id, 200));

        match detail {
            Ok(detail) => {
                if let Some(entry) = self.guilds.iter_mut().find(|g| g.id == detail.id) {
                    entry.banner = detail.banner.clone();
                    entry.banner_width = detail.banner_width;
                    entry.banner_height = detail.banner_height;
                    entry.description = detail.description.clone();
                    entry.member_count = detail.member_count;
                    entry.online_count = detail.online_count;
                }
                self.send(Update::GuildDetail(Box::new(detail)));
            }
            Err(err) => tracing::warn!("guild detail: {}", err.user_hint()),
        }

        match members {
            Ok(members) => {
                self.send(Update::Members(members.clone()));
                self.subscribe_presences(guild_id, &members);
            }
            Err(err) => tracing::warn!("members: {}", err.user_hint()),
        }
        self.send(Update::LoadingMembers(false));
    }

    /// Souscrit aux présences des membres visibles (op 14).
    ///
    /// Le client officiel plafonne la souscription à 100 identifiants par
    /// guilde ; les changements arrivent ensuite en `PRESENCE_UPDATE`. Sans
    /// cette requête, la passerelle ne diffuse que les présences des amis et
    /// des destinataires de messages privés, d'où une liste de membres
    /// entièrement « hors ligne ».
    fn subscribe_presences(&mut self, guild_id: &str, members: &[Member]) {
        let Some(tx) = self.gateway_tx.as_ref() else {
            return;
        };
        if self.subscribed_guild.as_deref() == Some(guild_id) {
            return;
        }
        // Libère l'ancienne guilde pour ne pas laisser fuiter des abonnements.
        if let Some(previous) = self.subscribed_guild.take() {
            let _ = tx.send(json!({
                "op": 14,
                "d": { "subscriptions": { previous: { "members": [] } } }
            }));
        }
        let ids: Vec<&str> = members
            .iter()
            .take(100)
            .map(|member| member.user.id.as_str())
            .collect();
        let _ = tx.send(json!({
            "op": 14,
            "d": { "subscriptions": { guild_id: { "members": ids } } }
        }));
        self.subscribed_guild = Some(guild_id.to_string());
    }

    /// Notifie la passerelle de notre présence vocale (op 4).
    fn send_voice_state(&self) {
        let (Some(tx), Some(guild_id), Some(channel_id)) = (
            self.gateway_tx.as_ref(),
            self.voice_guild.as_ref(),
            self.voice_channel.as_ref(),
        ) else {
            return;
        };
        let _ = tx.send(json!({
            "op": 4,
            "d": {
                "guild_id": guild_id,
                "channel_id": channel_id,
                "self_mute": self.voice_muted,
                "self_deaf": self.voice_deafened,
                "self_video": false,
                "self_stream": false,
                "connection_id": self.voice_connection_id,
                "viewer_stream_keys": [],
            }
        }));
    }

    async fn load_channel(&mut self, channel_id: &str) -> Result<(), String> {
        let Some(api) = self.api.clone() else {
            return Err("non connecté".into());
        };
        self.channel = Some(channel_id.to_string());
        self.oldest = None;
        self.has_more_before = true;
        self.send(Update::LoadingMessages(true));
        self.send(Update::Messages {
            channel_id: channel_id.to_string(),
            messages: Vec::new(),
            prepend: false,
        });
        let mut messages = api
            .messages(channel_id, PAGE_SIZE, None)
            .await
            .map_err(|e| e.user_hint())?;
        let count = messages.len() as u16;
        let has_more = count == PAGE_SIZE;
        self.has_more_before = has_more;
        messages.reverse();
        self.oldest = messages.first().map(|m| m.id.clone());
        if let Some(last) = messages.last() {
            let _ = api.ack_message(channel_id, &last.id, 0, false).await;
        }
        self.send(Update::HasMoreBefore(has_more));
        self.send(Update::Messages {
            channel_id: channel_id.to_string(),
            messages,
            prepend: false,
        });
        self.send(Update::LoadingMessages(false));
        Ok(())
    }

    /// Charge la page précédente de l'historique (défilement vers le haut).
    async fn load_older(&mut self) -> Result<(), String> {
        if !self.has_more_before {
            return Ok(());
        }
        let Some(api) = self.api.clone() else {
            return Ok(());
        };
        let (Some(channel), Some(oldest)) = (self.channel.clone(), self.oldest.clone()) else {
            return Ok(());
        };
        self.send(Update::LoadingMessages(true));
        let mut page = api
            .messages(&channel, PAGE_SIZE, Some(&oldest))
            .await
            .map_err(|e| e.user_hint())?;
        if page.is_empty() {
            self.has_more_before = false;
            self.send(Update::HasMoreBefore(false));
            self.send(Update::LoadingMessages(false));
            return Ok(());
        }
        let has_more = (page.len() as u16) == PAGE_SIZE;
        self.has_more_before = has_more;
        self.send(Update::HasMoreBefore(has_more));
        page.reverse();
        self.oldest = page.first().map(|message| message.id.clone());
        self.send(Update::Messages {
            channel_id: channel,
            messages: page,
            prepend: true,
        });
        self.send(Update::LoadingMessages(false));
        Ok(())
    }

    /// Démarre la session vocale annoncée par la passerelle : SFU LiveKit par
    /// défaut, maillage pair-à-pair uniquement si le serveur l'exige.
    fn handle_voice_server(&mut self, data: &Value) {
        let p2p = data.get("p2p") == Some(&Value::Bool(true));
        let Some(connection_id) = data
            .get("connection_id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        self.voice_connection_id = Some(connection_id.clone());

        if !p2p {
            if self.voice_sfu.is_some() {
                return;
            }
            let Some(endpoint) = data
                .get("endpoint")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
            else {
                self.send(Update::Error("endpoint vocal absent".into()));
                return;
            };
            let Some(token) = data
                .get("token")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
            else {
                self.send(Update::Error("jeton vocal absent".into()));
                return;
            };
            let (Some(guild_id), Some(channel_id)) =
                (self.voice_guild.clone(), self.voice_channel.clone())
            else {
                return;
            };
            tracing::info!(
                "session vocale SFU annoncée (guilde {guild_id}, salon {channel_id}, endpoint {})",
                endpoint.split('?').next().unwrap_or("")
            );
            let config = SfuConfig {
                endpoint,
                token,
                guild_id,
                channel_id,
                muted: self.voice_muted,
                deafened: self.voice_deafened,
            };
            match Sfu::start(config) {
                Ok((sfu, mut events)) => {
                    self.voice_sfu = Some(sfu);
                    let updates = self.update_tx.clone();
                    let ctx = self.ctx.clone();
                    tokio::spawn(async move {
                        while let Some(event) = events.recv().await {
                            match event {
                                SfuEvent::Connected => {
                                    tracing::info!("voix SFU connectée");
                                    let _ = updates.send(Update::VoiceConnected(true));
                                    ctx.request_repaint();
                                }
                                SfuEvent::Disconnected => {
                                    tracing::info!("voix SFU déconnectée");
                                    let _ = updates.send(Update::VoiceConnected(false));
                                    ctx.request_repaint();
                                }
                                SfuEvent::AudioUnavailable(message) | SfuEvent::Error(message) => {
                                    let _ = updates.send(Update::VoiceConnected(false));
                                    let _ = updates.send(Update::Error(message));
                                    ctx.request_repaint();
                                }
                            }
                        }
                    });
                }
                Err(error) => self.send(Update::Error(error)),
            }
            return;
        }

        let ice_servers: Vec<IceServer> = data
            .get("ice_servers")
            .and_then(Value::as_array)
            .map(|servers| {
                servers
                    .iter()
                    .filter_map(|server| {
                        let urls = server
                            .get("urls")
                            .and_then(Value::as_array)?
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect();
                        Some(IceServer {
                            urls,
                            username: server
                                .get("username")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            credential: server
                                .get("credential")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        if self.voice.is_some() {
            return;
        }
        let (Some(gateway), Some(guild_id), Some(channel_id)) = (
            self.gateway_tx.clone(),
            self.voice_guild.clone(),
            self.voice_channel.clone(),
        ) else {
            return;
        };
        let config = MeshConfig {
            guild_id,
            channel_id,
            local_connection_id: connection_id,
            ice_servers,
            muted: self.voice_muted,
            deafened: self.voice_deafened,
            gateway,
        };
        match Mesh::start(config) {
            Ok((mesh, mut events)) => {
                for (connection, user) in &self.voice_peers {
                    mesh.peer_joined(connection.clone(), user.clone());
                }
                self.voice = Some(mesh);
                let updates = self.update_tx.clone();
                let ctx = self.ctx.clone();
                tokio::spawn(async move {
                    while let Some(event) = events.recv().await {
                        match event {
                            MeshEvent::AudioUnavailable(message) | MeshEvent::Error(message) => {
                                let _ = updates.send(Update::Error(message));
                                ctx.request_repaint();
                            }
                            MeshEvent::Connected(peer) => {
                                tracing::info!("voix connectée avec {peer}");
                                let _ = updates.send(Update::VoiceConnected(true));
                                ctx.request_repaint();
                            }
                            MeshEvent::Disconnected(peer) => {
                                tracing::info!("voix déconnectée de {peer}");
                                let _ = updates.send(Update::VoiceConnected(false));
                                ctx.request_repaint();
                            }
                        }
                    }
                });
            }
            Err(error) => self.send(Update::Error(error)),
        }
    }

    fn stop_voice(&mut self) {
        if let Some(mesh) = self.voice.take() {
            mesh.stop();
        }
        if let Some(sfu) = self.voice_sfu.take() {
            sfu.stop();
        }
        self.voice_peers.clear();
        self.voice_connection_id = None;
        self.send(Update::VoiceConnected(false));
    }

    fn handle_gateway(&mut self, event: GatewayEvent) {
        match event {
            GatewayEvent::Connected => {
                self.send(Update::Gateway(GatewayStatus::Connecting));
            }
            GatewayEvent::Ready { .. } => {
                self.send(Update::Gateway(GatewayStatus::Online));
            }
            GatewayEvent::HeartbeatAck => {}
            GatewayEvent::Reconnecting { reason } => {
                tracing::info!("gateway reconnect: {reason}");
                self.send(Update::Gateway(GatewayStatus::Reconnecting));
            }
            GatewayEvent::Closed { code, reason } => {
                tracing::info!("gateway closed: {code:?} {reason}");
                self.send(Update::Gateway(GatewayStatus::Offline));
            }
            GatewayEvent::Fatal(reason) => {
                self.send(Update::Gateway(GatewayStatus::Offline));
                self.send(Update::Error(reason));
            }
            GatewayEvent::Hello { .. } => {}
            GatewayEvent::Dispatch { event, data } => match event.as_str() {
                "READY" => {
                    // The READY payload carries the initial presence snapshot,
                    // both at the top level and per guild.
                    if let Some(list) = data.get("presences").and_then(|v| v.as_array()) {
                        self.send_presences(list);
                    }
                    if let Some(list) = data.get("private_channels").and_then(|v| v.as_array()) {
                        let channels: Vec<Channel> = list
                            .iter()
                            .filter_map(|value| serde_json::from_value(value.clone()).ok())
                            .collect();
                        if !channels.is_empty() {
                            self.send(Update::PrivateChannels(channels));
                        }
                    }
                    if let Some(guilds) = data.get("guilds").and_then(|v| v.as_array()) {
                        for guild in guilds {
                            if let Some(list) = guild.get("presences").and_then(|v| v.as_array()) {
                                self.send_presences(list);
                            }
                        }
                    }
                }
                "MESSAGE_CREATE" => {
                    if let Ok(message) = serde_json::from_value::<Message>(data) {
                        self.send(Update::MessageCreated(message));
                    }
                }
                "MESSAGE_UPDATE" => {
                    if let Ok(message) = serde_json::from_value::<Message>(data) {
                        self.send(Update::MessageUpdated(message));
                    }
                }
                "MESSAGE_DELETE" => {
                    let channel_id = data
                        .get("channel_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let message_id = data
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    if !channel_id.is_empty() && !message_id.is_empty() {
                        self.send(Update::MessageDeleted {
                            channel_id,
                            message_id,
                        });
                    }
                }
                "MESSAGE_REACTION_ADD" | "MESSAGE_REACTION_REMOVE" => {
                    let added = event == "MESSAGE_REACTION_ADD";
                    let channel_id = data
                        .get("channel_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let message_id = data
                        .get("message_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let user_id = data
                        .get("user_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    let emoji = data
                        .get("emoji")
                        .and_then(|v| serde_json::from_value::<Emoji>(v.clone()).ok());
                    if let (false, Some(emoji)) =
                        (channel_id.is_empty() || message_id.is_empty(), emoji)
                    {
                        let me = self
                            .me
                            .as_ref()
                            .map(|user| user.id == user_id)
                            .unwrap_or(false);
                        self.send(Update::ReactionChanged {
                            channel_id,
                            message_id,
                            emoji,
                            delta: if added { 1 } else { -1 },
                            me: if me { Some(added) } else { None },
                        });
                    }
                }
                "MESSAGE_REACTION_REMOVE_ALL" => {
                    if let (Some(channel_id), Some(message_id)) = (
                        data.get("channel_id").and_then(|v| v.as_str()),
                        data.get("message_id").and_then(|v| v.as_str()),
                    ) {
                        // Toutes les réactions sont retirées localement.
                        self.send(Update::ReactionRemovedAll {
                            channel_id: channel_id.to_string(),
                            message_id: message_id.to_string(),
                        });
                    }
                }
                "TYPING_START" => {
                    let channel_id = data
                        .get("channel_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let user_id = data
                        .get("user_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    if !channel_id.is_empty() && !user_id.is_empty() {
                        self.send(Update::TypingStarted {
                            channel_id,
                            user_id,
                        });
                    }
                }
                "VOICE_STATE_UPDATE" => {
                    let user_id = data
                        .get("user_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let channel_id = data
                        .get("channel_id")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    let connection_id = data
                        .get("connection_id")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    let me = self.me.as_ref().map(|user| user.id.clone());
                    if !user_id.is_empty() {
                        // Remove the user from any channel first.
                        for members in self.voice_members.values_mut() {
                            members.retain(|id| id != &user_id);
                        }

                        if me.as_deref() == Some(user_id.as_str()) {
                            self.voice_connection_id = connection_id.clone();
                        }

                        let in_current = channel_id.is_some()
                            && channel_id.as_deref() == self.voice_channel.as_deref();
                        if in_current
                            && me.as_deref() != Some(user_id.as_str())
                            && let Some(connection_id) = connection_id.clone()
                        {
                            self.voice_peers
                                .insert(connection_id.clone(), user_id.clone());
                            if let Some(mesh) = self.voice.as_ref() {
                                mesh.peer_joined(connection_id, user_id.clone());
                            }
                        }

                        // Départ (ou changement de salon) : retirer les
                        // connexions vocales de cet utilisateur du maillage.
                        if !in_current {
                            let stale: Vec<String> = self
                                .voice_peers
                                .iter()
                                .filter(|(_, id)| *id == &user_id)
                                .map(|(connection, _)| connection.clone())
                                .collect();
                            for connection in stale {
                                self.voice_peers.remove(&connection);
                                if let Some(mesh) = self.voice.as_ref() {
                                    mesh.peer_left(connection);
                                }
                            }
                        }

                        if let Some(channel_id) = channel_id {
                            self.voice_members
                                .entry(channel_id)
                                .or_default()
                                .push(user_id);
                        }
                        if let Some(current) = self.voice_channel.clone() {
                            let members = self
                                .voice_members
                                .get(&current)
                                .cloned()
                                .unwrap_or_default();
                            self.send(Update::VoiceMembers(members));
                        }
                    }
                }
                "VOICE_SERVER_UPDATE" => {
                    self.handle_voice_server(&data);
                }
                "VOICE_SIGNAL" => {
                    let from = data
                        .get("from")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    if let (Some(mesh), Some(signal)) =
                        (self.voice.as_ref(), data.get("data").cloned())
                        && !from.is_empty()
                    {
                        mesh.signal(from, signal);
                    }
                }
                "PRESENCE_UPDATE" => {
                    if let Some(user_id) = data
                        .get("user")
                        .and_then(|user| user.get("id"))
                        .and_then(|id| id.as_str())
                    {
                        let status = data
                            .get("status")
                            .and_then(|status| status.as_str())
                            .unwrap_or("offline");
                        self.send(Update::Presence {
                            user_id: user_id.to_string(),
                            status: status.to_string(),
                        });
                    }
                }
                _ => {}
            },
        }
    }

    /// Emits one update per entry of a presences array. Accepts both
    /// `{user: {id}}` and `{user_id}` shapes.
    fn send_presences(&self, list: &[serde_json::Value]) {
        for presence in list {
            let user_id = presence
                .get("user")
                .and_then(|user| user.get("id"))
                .and_then(|id| id.as_str())
                .or_else(|| presence.get("user_id").and_then(|id| id.as_str()));
            let Some(user_id) = user_id else {
                continue;
            };
            let status = presence
                .get("status")
                .and_then(|status| status.as_str())
                .unwrap_or("offline");
            self.send(Update::Presence {
                user_id: user_id.to_string(),
                status: status.to_string(),
            });
        }
    }
}

/// `Option<&mut Receiver>` cannot be awaited directly in `select!`, so this
/// future stays pending while no gateway connection exists.
async fn recv_gateway(
    receiver: Option<&mut UnboundedReceiver<GatewayEvent>>,
) -> Option<GatewayEvent> {
    match receiver {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}
