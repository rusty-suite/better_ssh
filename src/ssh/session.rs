/// Gestion d'une session SSH asynchrone via russh.
/// L'architecture est basée sur deux canaux crossbeam :
///   - `cmd_tx` : l'UI envoie des commandes vers la task SSH (saisie, resize, disconnect)
///   - `event_rx` : la task SSH envoie des événements vers l'UI (données, statut)
/// Un canal mpsc dédié (`sftp_tx`) achemine les commandes SFTP vers la task SFTP.
/// Les tasks tokio tournent en arrière-plan et ne bloquent jamais le thread egui.
use anyhow::{Context, Result};
use async_trait::async_trait;
use crossbeam_channel::{Receiver, Sender};
use russh::client::{self, Handle};
use russh::keys::key::PublicKey;
use russh::{ChannelMsg, Disconnect};
use russh_sftp::client::SftpSession;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};

use crate::config::{AuthMethod, ConnectionProfile};
use crate::ssh::sftp::{RemoteEntry, SftpClient};

// ─── Commandes SFTP ───────────────────────────────────────────────────────────

/// Opérations SFTP demandées par l'UI à la task SFTP.
#[derive(Debug)]
pub enum SftpCommand {
    ListDir(String),
    Rename { from: String, to: String },
    Delete(String),
    DeleteDir(String),
    Mkdir(String),
    CreateFile(String),
    MovePaths { paths: Vec<String>, dest: String },
    Download { remote: String, local: std::path::PathBuf },
}

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
    /// Upload d'un fichier via SFTP sur le serveur (contenu en mémoire).
    SftpUpload { content: Vec<u8>, remote_path: String },
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
    /// Résultat d'un listage de répertoire SFTP.
    SftpListing { path: String, entries: Vec<RemoteEntry> },
    /// Résultat d'une opération SFTP (rename, delete, mkdir, etc.).
    SftpOpResult { op: String, ok: bool, msg: String },
    /// UID numérique de l'utilisateur connecté (déterminé au démarrage SFTP).
    SftpUid(u32),
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
        Ok(true)
    }
}

// ─── Session publique ─────────────────────────────────────────────────────────

/// Représente une session SSH active depuis le point de vue de l'UI.
/// Wrapping thread-safe des canaux de communication vers les tasks async.
pub struct SshSession {
    /// Canal pour envoyer des commandes PTY à la task SSH.
    pub cmd_tx: Sender<SessionCommand>,
    /// Canal pour recevoir les événements de la task SSH.
    pub event_rx: Receiver<SessionEvent>,
    /// Canal pour envoyer des commandes à la task SFTP.
    sftp_tx: mpsc::UnboundedSender<SftpCommand>,
}

impl SshSession {
    /// Démarre une session SSH en arrière-plan et retourne les handles de communication.
    pub fn connect(profile: ConnectionProfile, password: Option<String>) -> Self {
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded::<SessionCommand>();
        let (event_tx, event_rx) = crossbeam_channel::unbounded::<SessionEvent>();
        let (sftp_tx, sftp_rx) = mpsc::unbounded_channel::<SftpCommand>();

        let etx = event_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = run_session(profile, password, cmd_rx, etx.clone(), sftp_rx).await {
                let _ = etx.send(SessionEvent::Error(e.to_string()));
            }
        });

        Self { cmd_tx, event_rx, sftp_tx }
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

    /// Envoie une commande SFTP à la task SFTP (non-bloquant).
    pub fn send_sftp(&self, cmd: SftpCommand) {
        let _ = self.sftp_tx.send(cmd);
    }
}

// ─── Boucle de session async ──────────────────────────────────────────────────

/// Boucle principale d'une session SSH. Tourne dans une tokio task.
/// 1. Établit la connexion TCP et négocie SSH
/// 2. Authentifie l'utilisateur
/// 3. Ouvre un canal PTY + shell
/// 4. Ouvre un canal SFTP (non-fatal si non supporté)
/// 5. Relaie les données bidirectionnellement jusqu'à la déconnexion
async fn run_session(
    profile: ConnectionProfile,
    password: Option<String>,
    cmd_rx: Receiver<SessionCommand>,
    event_tx: Sender<SessionEvent>,
    sftp_rx: mpsc::UnboundedReceiver<SftpCommand>,
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
    log::info!("Connexion SSH → {} (utilisateur : {})", addr, profile.username);

    // Connexion TCP avec timeout global.
    let mut session = timeout(
        Duration::from_secs(profile.connection_timeout_secs),
        client::connect(config, addr.clone(), handler),
    )
    .await
    .context("timeout de connexion")?
    .context("connexion TCP échouée")?;

    log::info!("TCP établi → {}", addr);

    // Authentification selon la méthode choisie dans le profil.
    let auth_method_name = match &profile.auth_method {
        AuthMethod::Password => "password",
        AuthMethod::PublicKey { .. } => "publickey",
        AuthMethod::Agent => "agent",
    };
    log::info!("Authentification SSH : méthode={} utilisateur={}", auth_method_name, profile.username);

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
        log::warn!("Authentification refusée : utilisateur={} hôte={}", profile.username, addr);
        anyhow::bail!("Authentification refusée pour l'utilisateur '{}'", profile.username);
    }
    log::info!("Authentifié avec succès → {}", addr);

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

    // Ouvre un second canal pour le sous-système SFTP (non-fatal).
    // Les commandes SFTP envoyées avant que le canal soit prêt sont mises en attente
    // dans le canal mpsc et traitées dès que run_sftp_handler démarre.
    match session.channel_open_session().await {
        Ok(sftp_ch) => {
            if sftp_ch.request_subsystem(true, "sftp").await.is_ok() {
                match russh_sftp::client::SftpSession::new(sftp_ch.into_stream()).await {
                    Ok(sftp_session) => {
                        let sftp_client = SftpClient::new(sftp_session);
                        let etx = event_tx.clone();
                        tokio::spawn(async move {
                            run_sftp_handler(sftp_client, sftp_rx, etx).await;
                        });
                    }
                    Err(e) => log::warn!("Échec d'initialisation SFTP : {e}"),
                }
            } else {
                log::warn!("Le serveur ne supporte pas le sous-système SFTP");
            }
        }
        Err(e) => log::warn!("Impossible d'ouvrir le canal SFTP : {e}"),
    }

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
                SessionCommand::SftpUpload { content, remote_path } => {
                    // Ouvre un canal SFTP séparé sur la même session SSH (multiplexage).
                    let upload_result: Result<usize> = async {
                        let mut sftp_ch = session.channel_open_session().await?;
                        sftp_ch.request_subsystem(true, "sftp").await?;
                        let mut sftp = SftpSession::new(sftp_ch.into_stream()).await?;
                        sftp.write(&remote_path, &content).await?;
                        Ok(content.len())
                    }
                    .await;
                    let _ = event_tx.send(match upload_result {
                        Ok(n) => SessionEvent::SftpOpResult {
                            ok: true,
                            message: format!("OK Transfert réussi — {} → {} octets", remote_path, n),
                        },
                        Err(e) => SessionEvent::SftpOpResult {
                            ok: false,
                            message: format!("ERREUR SFTP : {}", e),
                        },
                    });
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
                _ => {}
            },
        }
    }
}

// ─── Gestionnaire SFTP async ──────────────────────────────────────────────────

/// Traite les commandes SFTP reçues via le canal mpsc et renvoie les résultats
/// via `event_tx`. S'arrête naturellement quand `sftp_tx` est abandonné
/// (session fermée → SshSession droppée).
async fn run_sftp_handler(
    client: SftpClient,
    mut sftp_rx: mpsc::UnboundedReceiver<SftpCommand>,
    event_tx: Sender<SessionEvent>,
) {
    // Détermine l'UID de l'utilisateur courant en statant "." (son répertoire home).
    if let Some(uid) = client.get_current_uid().await {
        let _ = event_tx.send(SessionEvent::SftpUid(uid));
    }

    while let Some(cmd) = sftp_rx.recv().await {
        match cmd {
            SftpCommand::ListDir(path) => {
                log::debug!("SFTP list : {}", path);
                match client.list_dir(&path).await {
                    Ok(entries) => {
                        log::debug!("SFTP list OK : {} entrées dans {}", entries.len(), path);
                        let _ = event_tx.send(SessionEvent::SftpListing { path, entries });
                    }
                    Err(e) => {
                        log::warn!("SFTP list échoué : {} — {}", path, e);
                        let _ = event_tx.send(SessionEvent::SftpOpResult {
                            op: format!("list {path}"),
                            ok: false,
                            msg: e.to_string(),
                        });
                    }
                }
            }
            SftpCommand::Rename { from, to } => {
                log::debug!("SFTP rename : {} → {}", from, to);
                let result = client.rename(&from, &to).await;
                if let Err(ref e) = result { log::warn!("SFTP rename échoué : {}", e); }
                let _ = event_tx.send(SessionEvent::SftpOpResult {
                    op: "rename".into(),
                    ok: result.is_ok(),
                    msg: result.err().map(|e| e.to_string()).unwrap_or_default(),
                });
            }
            SftpCommand::Delete(path) => {
                log::debug!("SFTP delete : {}", path);
                let result = client.remove_file(&path).await;
                if let Err(ref e) = result { log::warn!("SFTP delete échoué : {}", e); }
                let _ = event_tx.send(SessionEvent::SftpOpResult {
                    op: "delete".into(),
                    ok: result.is_ok(),
                    msg: result.err().map(|e| e.to_string()).unwrap_or_default(),
                });
            }
            SftpCommand::DeleteDir(path) => {
                log::debug!("SFTP rmdir : {}", path);
                let result = client.remove_dir(&path).await;
                if let Err(ref e) = result { log::warn!("SFTP rmdir échoué : {}", e); }
                let _ = event_tx.send(SessionEvent::SftpOpResult {
                    op: "rmdir".into(),
                    ok: result.is_ok(),
                    msg: result.err().map(|e| e.to_string()).unwrap_or_default(),
                });
            }
            SftpCommand::Mkdir(path) => {
                log::debug!("SFTP mkdir : {}", path);
                let result = client.mkdir(&path).await;
                if let Err(ref e) = result { log::warn!("SFTP mkdir échoué : {}", e); }
                let _ = event_tx.send(SessionEvent::SftpOpResult {
                    op: "mkdir".into(),
                    ok: result.is_ok(),
                    msg: result.err().map(|e| e.to_string()).unwrap_or_default(),
                });
            }
            SftpCommand::CreateFile(path) => {
                log::debug!("SFTP create : {}", path);
                let result = client.create_empty_file(&path).await;
                if let Err(ref e) = result { log::warn!("SFTP create échoué : {}", e); }
                let _ = event_tx.send(SessionEvent::SftpOpResult {
                    op: "create".into(),
                    ok: result.is_ok(),
                    msg: result.err().map(|e| e.to_string()).unwrap_or_default(),
                });
            }
            SftpCommand::MovePaths { paths, dest } => {
                log::debug!("SFTP move : {} élément(s) → {}", paths.len(), dest);
                let mut errors = Vec::new();
                for path in &paths {
                    let filename = path.rsplit('/').next().unwrap_or(path.as_str());
                    let target = format!("{}/{}", dest.trim_end_matches('/'), filename);
                    if let Err(e) = client.rename(path, &target).await {
                        log::warn!("SFTP move échoué : {} → {} : {}", path, target, e);
                        errors.push(e.to_string());
                    }
                }
                let ok = errors.is_empty();
                let _ = event_tx.send(SessionEvent::SftpOpResult {
                    op: format!("move {} élément(s)", paths.len()),
                    ok,
                    msg: errors.join("; "),
                });
            }
            SftpCommand::Download { remote, local } => {
                log::debug!("SFTP download : {} → {}", remote, local.display());
                let result = client.download_file(&remote, &local).await;
                if let Err(ref e) = result { log::warn!("SFTP download échoué : {}", e); }
                let _ = event_tx.send(SessionEvent::SftpOpResult {
                    op: "download".into(),
                    ok: result.is_ok(),
                    msg: result.err().map(|e| e.to_string()).unwrap_or_default(),
                });
            }
        }
    }
}
