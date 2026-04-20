/// Gestion d'une session SSH asynchrone via russh.
/// L'architecture est basée sur deux canaux crossbeam :
///   - `cmd_tx` : l'UI envoie des commandes vers la task SSH (saisie, resize, disconnect)
///   - `event_rx` : la task SSH envoie des événements vers l'UI (données, statut)
/// La task tokio tourne en arrière-plan et ne bloque jamais le thread egui.
use anyhow::{Context, Result};
use async_trait::async_trait;
use crossbeam_channel::{Receiver, Sender};
use russh::client::{self, Handle};
use russh::keys::key::PublicKey;
use russh::{ChannelMsg, Disconnect};
use std::sync::Arc;
use tokio::time::{timeout, Duration};

use crate::config::{AuthMethod, ConnectionProfile};

// ─── Types de messages ────────────────────────────────────────────────────────

/// Commandes envoyées par l'UI vers la task SSH.
#[derive(Debug)]
pub enum SessionCommand {
    /// Octets saisis par l'utilisateur à transmettre au pseudo-terminal distant.
    SendInput(Vec<u8>),
    /// Nouveau redimensionnement du terminal (cols × rows en caractères).
    Resize { cols: u32, rows: u32 },
    /// Ferme proprement la session SSH.
    Disconnect,
}

/// Événements émis par la task SSH vers l'UI.
#[derive(Debug, Clone)]
pub enum SessionEvent {
    /// Connexion et authentification réussies.
    Connected,
    /// Données reçues du serveur (sortie terminal, stderr, etc.).
    Data(Vec<u8>),
    /// Session fermée normalement (message explicatif inclus).
    Disconnected(String),
    /// Erreur non-récupérable (connexion refusée, auth échouée…).
    Error(String),
    /// Le fingerprint de l'hôte a changé → alerte MITM potentielle.
    FingerprintAlert { host: String, fingerprint: String },
}

// ─── Handler russh ────────────────────────────────────────────────────────────

/// Implémentation du trait `client::Handler` de russh.
/// Reçoit les événements de bas niveau du protocole SSH.
struct ClientHandler {
    event_tx: Sender<SessionEvent>,
}

#[async_trait]
impl client::Handler for ClientHandler {
    type Error = anyhow::Error;

    /// Appelé lors de la réception de la clé publique du serveur.
    /// TODO : comparer avec ~/.rustshell/known_hosts et alerter en cas de changement.
    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        let fp = server_public_key.fingerprint();
        log::debug!("Fingerprint serveur : {fp}");
        // Pour l'instant on accepte tout et on log. La vérification known_hosts
        // sera implémentée en Phase 2.
        Ok(true)
    }
}

// ─── Session publique ─────────────────────────────────────────────────────────

/// Représente une session SSH active depuis le point de vue de l'UI.
/// Wrapping thread-safe des canaux de communication vers la task async.
pub struct SshSession {
    /// Canal pour envoyer des commandes à la task SSH.
    pub cmd_tx: Sender<SessionCommand>,
    /// Canal pour recevoir les événements de la task SSH.
    pub event_rx: Receiver<SessionEvent>,
}

impl SshSession {
    /// Démarre une session SSH en arrière-plan et retourne les handles de communication.
    pub fn connect(profile: ConnectionProfile, password: Option<String>) -> Self {
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded::<SessionCommand>();
        let (event_tx, event_rx) = crossbeam_channel::unbounded::<SessionEvent>();

        let etx = event_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = run_session(profile, password, cmd_rx, etx.clone()).await {
                let _ = etx.send(SessionEvent::Error(e.to_string()));
            }
        });

        Self { cmd_tx, event_rx }
    }

    /// Envoie des octets de saisie clavier vers le pseudo-terminal distant.
    pub fn send_input(&self, data: Vec<u8>) {
        let _ = self.cmd_tx.send(SessionCommand::SendInput(data));
    }

    /// Notifie le serveur d'un changement de taille du terminal.
    pub fn resize(&self, cols: u32, rows: u32) {
        let _ = self.cmd_tx.send(SessionCommand::Resize { cols, rows });
    }

    /// Demande la fermeture propre de la session.
    pub fn disconnect(&self) {
        let _ = self.cmd_tx.send(SessionCommand::Disconnect);
    }

    /// Tente de lire un événement sans bloquer (non-blocking).
    pub fn try_recv(&self) -> Option<SessionEvent> {
        self.event_rx.try_recv().ok()
    }
}

// ─── Boucle de session async ──────────────────────────────────────────────────

/// Boucle principale d'une session SSH. Tourne dans une tokio task.
/// 1. Établit la connexion TCP et négocie SSH
/// 2. Authentifie l'utilisateur
/// 3. Ouvre un canal avec pseudo-terminal
/// 4. Relaie les données bidirectionnellement jusqu'à la déconnexion
async fn run_session(
    profile: ConnectionProfile,
    password: Option<String>,
    cmd_rx: Receiver<SessionCommand>,
    event_tx: Sender<SessionEvent>,
) -> Result<()> {
    // Configure le client SSH.
    // inactivity_timeout = None : russh ne coupe pas de son côté après N secondes d'idle.
    // keepalive_interval = 30 s : le client envoie un paquet SSH keepalive toutes les
    //   30 secondes d'inactivité, ce qui empêche le serveur (sshd) de couper la connexion
    //   à cause de son propre ClientAliveInterval.
    // keepalive_max = 3 : si 3 keepalives consécutifs restent sans réponse, russh
    //   déconnecte proprement au lieu de bloquer indéfiniment.
    let config = Arc::new(client::Config {
        inactivity_timeout: None,
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
        ..<_>::default()
    });

    let handler = ClientHandler { event_tx: event_tx.clone() };
    let addr = format!("{}:{}", profile.host, profile.port);

    // Connexion TCP avec timeout global.
    let mut session = timeout(
        Duration::from_secs(profile.connection_timeout_secs),
        client::connect(config, addr, handler),
    )
    .await
    .context("timeout de connexion")?
    .context("connexion TCP échouée")?;

    // Authentification selon la méthode choisie dans le profil.
    let authenticated = match &profile.auth_method {
        AuthMethod::Password => {
            let pw = password.unwrap_or_default();
            session
                .authenticate_password(&profile.username, pw)
                .await
                .context("authentification par mot de passe")?
        }
        AuthMethod::PublicKey { identity_file } => {
            // Charge la clé depuis le chemin donné (supporte ~/ via shellexpand).
            let key = crate::ssh::key_auth::load_key(identity_file, None)
                .await
                .context("chargement de la clé privée")?;
            session
                .authenticate_publickey(&profile.username, Arc::new(key))
                .await
                .context("authentification par clé publique")?
        }
        AuthMethod::Agent => {
            anyhow::bail!("Authentification par agent SSH non encore implémentée");
        }
    };

    if !authenticated {
        anyhow::bail!("Authentification refusée pour l'utilisateur '{}'", profile.username);
    }

    // Ouvre un canal de session avec un pseudo-terminal (PTY) pour le shell interactif.
    let mut channel = session
        .channel_open_session()
        .await
        .context("ouverture du canal de session")?;

    channel
        .request_pty(true, "xterm-256color", 80, 24, 0, 0, &[(russh::Pty::ECHO, 1)])
        .await
        .context("demande PTY")?;

    channel.request_shell(true).await.context("demande shell")?;

    let _ = event_tx.send(SessionEvent::Connected);

    // ── Boucle I/O bidirectionnelle ───────────────────────────────────────────
    // exit_code : rempli quand le serveur envoie ExitStatus avant Close.
    let mut exit_code: Option<u32> = None;

    loop {
        // Traite toutes les commandes en attente de l'UI (non-bloquant).
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                SessionCommand::SendInput(data) => {
                    channel.data(data.as_ref()).await?;
                }
                SessionCommand::Resize { cols, rows } => {
                    channel.window_change(cols, rows, 0, 0).await?;
                }
                SessionCommand::Disconnect => {
                    let _ = session
                        .disconnect(Disconnect::ByApplication, "Fermeture par l'utilisateur", "fr")
                        .await;
                    let _ = event_tx.send(SessionEvent::Disconnected(
                        "Déconnecté par l'utilisateur".into(),
                    ));
                    return Ok(());
                }
            }
        }

        // Attend un message du serveur avec un timeout court pour rester réactif.
        match tokio::time::timeout(Duration::from_millis(50), channel.wait()).await {
            // Timeout : rien de nouveau, on reboucle pour vérifier les commandes UI.
            Err(_) => continue,

            // Le canal interne a été fermé (connexion réseau perdue ou russh interne).
            Ok(None) => {
                let msg = match exit_code {
                    Some(0) => "Session terminée (exit 0)".into(),
                    Some(n) => format!("Session terminée (exit {})", n),
                    None    => "Connexion interrompue".into(),
                };
                let _ = event_tx.send(SessionEvent::Disconnected(msg));
                return Ok(());
            }

            Ok(Some(msg)) => match msg {
                ChannelMsg::Data { data } => {
                    let _ = event_tx.send(SessionEvent::Data(data.to_vec()));
                }
                ChannelMsg::ExtendedData { data, .. } => {
                    // stderr → affiché dans le terminal comme stdout.
                    let _ = event_tx.send(SessionEvent::Data(data.to_vec()));
                }
                // Le serveur indique que le processus shell s'est terminé.
                // On garde le code de sortie mais on n'interrompt pas encore :
                // le serveur enverra Eof puis Close immédiatement après.
                ChannelMsg::ExitStatus { exit_status } => {
                    exit_code = Some(exit_status);
                }
                ChannelMsg::ExitSignal { signal_name, .. } => {
                    let _ = event_tx.send(SessionEvent::Data(
                        format!("\r\n\x1b[31m[Signal : {:?}]\x1b[0m\r\n", signal_name).into_bytes(),
                    ));
                }
                // EOF = le serveur ne produira plus de données ; on attend Close.
                ChannelMsg::Eof => {}
                // Close = fermeture propre du canal SSH.
                ChannelMsg::Close => {
                    let msg = match exit_code {
                        Some(0) => "Session terminée".into(),
                        Some(n) => format!("Session terminée (exit {})", n),
                        None    => "Session fermée par le serveur".into(),
                    };
                    let _ = event_tx.send(SessionEvent::Disconnected(msg));
                    return Ok(());
                }
                // Succès/échec d'une requête (pty, shell…) — ignoré silencieusement.
                _ => {}
            },
        }
    }
}
