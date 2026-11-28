/// Structure principale de l'application et boucle de mise à jour egui.
/// `BetterSshApp` contient tout l'état global : onglets ouverts, sidebar,
/// panneau de scan réseau, préférences, etc.
use crate::config::{AppConfig, AuthMethod, ConnectionProfile, Vault};
use crate::i18n::Lang;
use crate::network::scanner::ScanResult;
use crate::network::TelnetSession;
use crate::ssh::session::{SessionCommand, SessionEvent, SftpCommand, SftpOp, SshSession};
use crate::ui::{
    file_explorer::FileExplorerState,
    network_scan::NetworkScanState,
    sidebar::SidebarState,
    snippets::SnippetsState,
    system_monitor::SystemMonitorState,
    terminal::TerminalState,
};
use egui::{Context, FontId, TextStyle};
use rusty_core::settings::CommonSettings;
use rusty_core::update::{self, Release};
use rusty_ui::rusty_logo::{Logo, LogoAnim};
use rusty_ui::Tone;
use std::sync::mpsc::{self, Receiver};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Identifiant de l'application (dossier portable, journal, mises à jour, dépôt).
pub const APP_ID: &str = "better_ssh";
/// Nom des fichiers publiés (`betterssh.exe`, `betterssh-linux-x86_64`).
pub const ASSET_BASE: &str = "betterssh";

/// Agent HTTP générique (jamais d'information personnelle).
pub fn user_agent() -> String {
    format!("{APP_ID}/{VERSION}")
}

/// Réglages persistants communs (langue, mises à jour).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub common: CommonSettings,
}

// ─── Session unifiée (SSH ou Telnet) ─────────────────────────────────────────

/// Enveloppe une session SSH ou Telnet derrière une interface commune.
pub enum AnySession {
    Ssh(SshSession),
    Telnet(TelnetSession),
}

impl AnySession {
    pub fn send_input(&self, data: Vec<u8>) {
        match self {
            Self::Ssh(s)    => s.send_input(data),
            Self::Telnet(t) => t.send_input(data),
        }
    }
    pub fn try_recv(&self) -> Option<SessionEvent> {
        match self {
            Self::Ssh(s)    => s.try_recv(),
            Self::Telnet(t) => t.try_recv(),
        }
    }
    pub fn disconnect(&self) {
        match self {
            Self::Ssh(s)    => s.disconnect(),
            Self::Telnet(t) => t.disconnect(),
        }
    }
    /// Commandes SFTP : SSH uniquement, ignorées pour Telnet.
    pub fn send_sftp(&self, cmd: SftpCommand) {
        if let Self::Ssh(s) = self { s.send_sftp(cmd); }
    }
    /// Upload de fichier via glisser-déposer : SSH uniquement.
    pub fn upload_file(&self, content: Vec<u8>, remote_path: String) {
        if let Self::Ssh(s) = self {
            let _ = s.cmd_tx.send(SessionCommand::SftpUpload { content, remote_path });
        }
    }
}

// ─── Dialogue Telnet ──────────────────────────────────────────────────────────

pub struct TelnetDialog {
    pub host: String,
    pub port: String,
}

// ─── Dialogue de connexion depuis le scan réseau ──────────────────────────────

/// État du dialogue affiché quand l'utilisateur clique "Connecter" dans le scan.
/// Permet de saisir / vérifier les identifiants avant d'ouvrir réellement la session.
pub struct ScanConnectDialog {
    /// Résultat de scan à l'origine du dialogue (IP, hostname, bannière).
    pub scan_result: ScanResult,
    /// Nom d'utilisateur (pré-rempli depuis profil existant ou "root" par défaut).
    pub username: String,
    /// Mot de passe saisi (jamais persisté en clair).
    pub password: String,
    /// Méthode d'authentification sélectionnée.
    pub auth_method: AuthMethod,
    /// Chemin du fichier de clé privée (si auth par clé).
    pub identity_file: String,
    /// Champ de saisie de la clé maître du vault (pour déverrouillage ou création).
    pub vault_key_input: String,
    /// true si le mot de passe a été chargé automatiquement depuis le vault.
    pub vault_password_loaded: bool,
    /// true si aucun profil existant n'a été trouvé pour cet hôte (sera créé).
    pub is_new: bool,
    /// ID du profil existant (pour mise à jour et accès au vault).
    pub existing_profile_id: Option<String>,
    /// Message d'erreur de déverrouillage vault (mauvais mot de passe).
    pub vault_error: Option<String>,
}

// ─── Onglet de session ────────────────────────────────────────────────────────

/// Représente une session SSH ouverte dans un onglet.
pub struct Tab {
    /// Identifiant unique (séquentiel) pour distinguer les onglets.
    pub id: usize,
    /// Profil de connexion associé à cet onglet.
    pub profile: ConnectionProfile,
    /// Session active — SSH ou Telnet (None si pas encore connecté).
    pub session: Option<AnySession>,
    /// État du widget terminal (scrollback, parseur ANSI, saisie).
    pub terminal: TerminalState,
    /// État du panneau d'exploration SFTP (arborescence, chemin courant).
    pub file_explorer: FileExplorerState,
    /// État du panneau de monitoring système (CPU, RAM, disques).
    pub system_monitor: SystemMonitorState,
    /// true si le panneau SFTP est affiché (toggle F2).
    pub show_file_explorer: bool,
    /// true si le panneau monitoring est affiché (toggle F3).
    pub show_system_monitor: bool,
    /// true si la session est connectée et opérationnelle.
    pub connected: bool,
}

impl Tab {
    pub fn new(id: usize, profile: ConnectionProfile) -> Self {
        Self {
            id,
            profile,
            session: None,
            terminal: TerminalState::new(),
            file_explorer: FileExplorerState::new(),
            system_monitor: SystemMonitorState::new(),
            show_file_explorer: false,
            show_system_monitor: false,
            connected: false,
        }
    }
}

// ─── Application principale ───────────────────────────────────────────────────

/// État global de BetterSSH. Implémente `eframe::App` pour être appelé à chaque frame.
pub struct BetterSshApp {
    /// Configuration persistée (thème, terminal, réseau).
    pub config: AppConfig,
    /// État de la barre latérale (liste des profils, dialogue de création).
    pub sidebar: SidebarState,
    /// Onglets ouverts (un par session SSH).
    pub tabs: Vec<Tab>,
    /// Index de l'onglet actif dans `tabs`.
    pub active_tab: usize,
    /// Compteur monotone pour générer des IDs d'onglet uniques.
    pub next_tab_id: usize,
    /// État du gestionnaire de snippets / macros.
    pub snippets: SnippetsState,
    /// État du panneau de scan réseau.
    pub network_scan: NetworkScanState,
    /// true = fenêtre snippets visible.
    pub show_snippets: bool,
    /// true = fenêtre scan réseau visible.
    pub show_network_scan: bool,
    /// true = fenêtre préférences visible.
    pub show_preferences: bool,
    /// Handle du runtime tokio partagé pour lancer des tâches async.
    pub tokio_rt: tokio::runtime::Handle,
    /// Vault de mots de passe déverrouillé pour cette session (None = verrouillé).
    pub vault: Option<Vault>,
    /// Dialogue de connexion en attente depuis le scan réseau (None = pas de dialogue).
    pub pending_scan_connect: Option<ScanConnectDialog>,
    /// Dialogue de connexion Telnet (None = fermé).
    pub telnet_dialog: Option<TelnetDialog>,
    /// Langue active de l'interface.
    pub lang: Lang,
    /// Répertoire de travail (config, vault, lang…).
    pub work_dir: std::path::PathBuf,
    /// Langue complète du kit (textes communs `[kit]`, stem, code du bouton Langue).
    pub kit_lang: rusty_core::Lang,
    pub settings: Settings,
    pub logo: Logo,
    pub toasts: rusty_ui::Toasts,
    pub zones: rusty_ui::Zones,
    pub show_language: bool,
    pub show_about: bool,
    pub show_update: bool,
    pub online: Option<bool>,
    pub net_rx: Option<Receiver<bool>>,
    pub lang_rx: Option<Receiver<Result<usize, String>>>,
    pub update: Option<Release>,
    pub update_rx: Option<Receiver<Result<Option<Release>, String>>>,
    pub install_rx: Option<Receiver<Result<(), String>>>,
    pub args_rx: Receiver<Vec<String>>,
}

impl BetterSshApp {
    /// Initialise l'application : détection du thème OS, chargement config, polices.
    pub fn new(cc: &eframe::CreationContext<'_>, settings: Settings, args_rx: Receiver<Vec<String>>, rolled_back: Option<String>) -> Self {
        let config = AppConfig::load().unwrap_or_default();

        // Thème et polices de la charte, taille de police du terminal sauvegardée.
        setup_fonts(&cc.egui_ctx, config.terminal.font_size);

        let rt = tokio::runtime::Handle::current();

        let work_dir = AppConfig::config_dir();
        let (lang, kit_lang) = crate::i18n::load_lang(&work_dir, &settings.common.language);
        rusty_ui::set_texts(&cc.egui_ctx, rusty_ui::KitTexts::from_lookup(|k| kit_lang.t(k)));
        let mut logo = Logo::new(LogoAnim::Scan);
        logo.play_launch(0.0);

        let mut app = Self {
            sidebar: SidebarState::new(config.profiles.clone()),
            config,
            tabs: Vec::new(),
            active_tab: 0,
            next_tab_id: 1,
            snippets: SnippetsState::new(&lang),
            network_scan: NetworkScanState::new(),
            show_snippets: false,
            show_network_scan: false,
            show_preferences: false,
            tokio_rt: rt,
            vault: None,
            pending_scan_connect: None,
            telnet_dialog: None,
            lang,
            work_dir,
            kit_lang,
            settings,
            logo,
            toasts: rusty_ui::Toasts::default(),
            zones: rusty_ui::Zones::default(),
            show_language: false,
            show_about: false,
            show_update: false,
            online: None,
            net_rx: Some(rusty_core::net::check_in_background(user_agent())),
            lang_rx: None,
            update: None,
            update_rx: None,
            install_rx: None,
            args_rx,
        };
        if let Some(v) = rolled_back {
            let text = app.kit_lang.tf("kit.update_restored_text", &[("version", &v)]);
            app.toasts.push(Tone::Warning, app.kit_lang.t("kit.update_restored"), text);
        }
        app
    }

    /// Ouvre un nouvel onglet pour le profil donné, démarre la session SSH et le rend actif.
    /// `password` est passé uniquement pour l'auth par mot de passe ; `None` sinon.
    pub fn open_profile(&mut self, profile: ConnectionProfile, password: Option<String>) {
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        let mut tab = Tab::new(id, profile.clone());

        // Répertoire de départ de l'explorateur = home de l'utilisateur.
        tab.file_explorer.current_path = if profile.username == "root" {
            "/root".into()
        } else if !profile.username.is_empty() {
            format!("/home/{}", profile.username)
        } else {
            "/".into()
        };

        // Charge l'historique de commandes depuis le vault chiffré si disponible.
        if let Some(vault) = &self.vault {
            if let Ok(history) = vault.get_history(&profile.id) {
                // session_history : index 0 = plus récente.
                // On insère du plus ancien vers le plus récent via push_front.
                for cmd in history.into_iter().rev() {
                    tab.terminal.session_history.push_front(cmd);
                    if tab.terminal.session_history.len() >= 50 { break; }
                }
            }
        }

        // Lance la session SSH en arrière-plan immédiatement.
        tab.session = Some(AnySession::Ssh(SshSession::connect(profile, password)));

        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
    }

    /// Ferme l'onglet à l'index donné et ajuste `active_tab`.
    pub fn close_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.tabs.remove(index);
            if self.active_tab >= self.tabs.len() && !self.tabs.is_empty() {
                self.active_tab = self.tabs.len() - 1;
            }
        }
    }

    /// Ouvre un onglet Telnet vers `host:port`.
    pub fn open_telnet(&mut self, host: String, port: u16) {
        let profile = ConnectionProfile::new(
            format!("Telnet {}:{}", host, port),
            host.clone(),
            "telnet",
        );
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        let mut tab = Tab::new(id, profile);
        tab.session = Some(AnySession::Telnet(TelnetSession::connect(host, port)));
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
    }

    /// Persiste la configuration courante sur le disque.
    pub fn save_config(&mut self) {
        self.config.profiles = self.sidebar.profiles.clone();
        if let Err(e) = self.config.save() {
            log::error!("Erreur lors de la sauvegarde de la configuration : {e}");
        }
    }

    /// Déchiffre et injecte hôte + utilisateur depuis le vault dans tous les profils
    /// en mémoire. À appeler dès que le vault vient d'être déverrouillé.
    pub fn hydrate_profiles_from_vault(&mut self) {
        if let Some(vault) = &self.vault {
            for profile in &mut self.sidebar.profiles {
                if profile.host.is_empty() {
                    profile.host = vault.get_address(&profile.id)
                        .ok().flatten().unwrap_or_default();
                }
                if profile.username.is_empty() {
                    profile.username = vault.get_username(&profile.id)
                        .ok().flatten().unwrap_or_default();
                }
            }
        }
    }

    /// Applique la taille de police dans tous les onglets ouverts et dans egui.
    pub fn apply_font_size(&mut self, ctx: &Context, size: f32) {
        self.config.terminal.font_size = size;
        // Met à jour la police dans chaque onglet déjà ouvert.
        for tab in &mut self.tabs {
            tab.terminal.font_size = size;
        }
        setup_fonts(ctx, size);
    }
}

// ─── Boucle principale egui ───────────────────────────────────────────────────

impl eframe::App for BetterSshApp {
    /// Appelée à chaque frame par eframe (~60 fps). C'est ici que tout le rendu se passe.
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // Collecte d'abord les événements SSH reçus en async depuis la dernière frame.
        self.zones.begin_frame(ctx);
        poll_session_events(self);
        self.poll_kit(ctx);
        handle_keyboard_shortcuts(self, ctx);
        crate::ui::render(self, ctx);
        // Repaint régulier pour rafraîchir les données SSH qui arrivent en async.
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }
}

// ─── Raccourcis clavier ───────────────────────────────────────────────────────

/// Traite les raccourcis clavier globaux (indépendants du widget en focus).
fn handle_keyboard_shortcuts(app: &mut BetterSshApp, ctx: &Context) {
    use egui::{Key, Modifiers};
    ctx.input_mut(|i| {
        // Ctrl+T → nouveau profil / nouvelle connexion
        if i.consume_key(Modifiers::CTRL, Key::T) {
            app.sidebar.show_new_profile = true;
        }
        // Ctrl+W → fermer l'onglet actif
        if i.consume_key(Modifiers::CTRL, Key::W) {
            let idx = app.active_tab;
            app.close_tab(idx);
        }
        // Ctrl+Tab → onglet suivant (cyclique)
        if i.consume_key(Modifiers::CTRL, Key::Tab) {
            if !app.tabs.is_empty() {
                app.active_tab = (app.active_tab + 1) % app.tabs.len();
            }
        }
        // Ctrl+Shift+Tab → onglet précédent
        if i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::Tab) {
            if !app.tabs.is_empty() {
                app.active_tab = app.active_tab.saturating_sub(1);
            }
        }
        // F2 → toggle explorateur SFTP
        if i.consume_key(Modifiers::NONE, Key::F2) {
            let idx = app.active_tab;
            if idx < app.tabs.len() {
                let was_shown = app.tabs[idx].show_file_explorer;
                app.tabs[idx].show_file_explorer = !was_shown;
                // Déclenche le premier chargement si on ouvre l'explorateur
                // sur une session déjà connectée sans listage précédent.
                if app.tabs[idx].show_file_explorer
                    && app.tabs[idx].connected
                    && !app.tabs[idx].file_explorer.loaded
                    && !app.tabs[idx].file_explorer.loading
                {
                    let path = app.tabs[idx].file_explorer.current_path.clone();
                    app.tabs[idx].file_explorer.loading = true;
                    if let Some(session) = &app.tabs[idx].session {
                        session.send_sftp(SftpCommand::ListDir(path));
                    }
                }
            }
        }
        // F3 → toggle moniteur système
        if i.consume_key(Modifiers::NONE, Key::F3) {
            if let Some(tab) = app.tabs.get_mut(app.active_tab) {
                tab.show_system_monitor = !tab.show_system_monitor;
            }
        }
        // F4 → toggle snippets
        if i.consume_key(Modifiers::NONE, Key::F4) {
            app.show_snippets = !app.show_snippets;
        }
        // F5 → toggle scan réseau
        if i.consume_key(Modifiers::NONE, Key::F5) {
            app.show_network_scan = !app.show_network_scan;
        }
        // Ctrl+, → préférences
        if i.consume_key(Modifiers::CTRL, Key::Comma) {
            app.show_preferences = !app.show_preferences;
        }
    });
}

// ─── Collecte des événements SSH ─────────────────────────────────────────────

/// Vide les canaux d'événements de toutes les sessions SSH actives
/// et met à jour le terminal / l'état de connexion de chaque onglet.
fn poll_session_events(app: &mut BetterSshApp) {
    // Sauvegarde asynchrone de l'historique quand une commande a été ajoutée.
    // L'encryption age est lente (~1 s) → spawn_blocking pour ne pas bloquer l'UI.
    for tab in &mut app.tabs {
        if tab.terminal.pending_history_save {
            tab.terminal.pending_history_save = false;
            if let Some(vault) = app.vault.clone() {
                let commands: Vec<String> = tab.terminal.session_history
                    .iter().take(20).cloned().collect();
                let profile_id = tab.profile.id.clone();
                tokio::task::spawn_blocking(move || {
                    if let Err(e) = vault.store_history(&profile_id, &commands) {
                        log::warn!("Historique vault : échec sauvegarde : {e}");
                    }
                });
            }
        }
    }

    for tab in &mut app.tabs {
        // Collecte les événements sans garder de référence sur `tab.session`
        // (évite le conflit borrow immuable / mutable sur `tab`).
        let events: Vec<SessionEvent> = if let Some(session) = &tab.session {
            let mut evs = Vec::new();
            loop {
                match session.try_recv() {
                    Some(ev) => {
                        let terminal = matches!(ev, SessionEvent::Disconnected(_) | SessionEvent::Error(_));
                        evs.push(ev);
                        if terminal { break; } // plus d'événements utiles après ça
                    }
                    None => break,
                }
            }
            evs
        } else {
            Vec::new()
        };

        for event in events {
            match event {
                SessionEvent::Connected => {
                    tab.connected = true;
                    tab.terminal.feed(format!("\x1b[32m{}\x1b[0m\r\n", app.lang.app_ssh_connected).as_bytes());
                    // Déclenche le premier listage SFTP si l'explorateur est ouvert.
                    // La commande est mise en attente dans le canal mpsc et traitée dès
                    // que run_sftp_handler démarre (après l'init du sous-système SFTP).
                    if tab.show_file_explorer {
                        let path = tab.file_explorer.current_path.clone();
                        tab.file_explorer.loading = true;
                        if let Some(session) = &tab.session {
                            session.send_sftp(SftpCommand::ListDir(path));
                        }
                    }
                }
                SessionEvent::Data(data) => {
                    tab.terminal.feed(&data);
                }
                SessionEvent::Disconnected(reason) => {
                    tab.connected = false;
                    let msg = Lang::fmt(&app.lang.app_ssh_disconnected, &[("msg", &reason.text(&app.lang))]);
                    let text = format!("\r\n\x1b[33m[{msg}]\x1b[0m\r\n");
                    tab.terminal.feed(text.as_bytes());
                    tab.session = None;
                }
                SessionEvent::Error(err) => {
                    tab.connected = false;
                    let msg = Lang::fmt(&app.lang.app_ssh_error, &[("err", &err.text(&app.lang))]);
                    let text = format!("\r\n\x1b[31m{msg}\x1b[0m\r\n");
                    tab.terminal.feed(text.as_bytes());
                    tab.session = None;
                }
                SessionEvent::Signal(sig) => {
                    let msg = Lang::fmt(&app.lang.app_ssh_signal, &[("sig", &sig)]);
                    tab.terminal.feed(format!("\r\n\x1b[31m{msg}\x1b[0m\r\n").as_bytes());
                }
                SessionEvent::FingerprintAlert { host, fingerprint } => {
                    log::warn!("Alerte fingerprint MITM : {host} — {fingerprint}");
                }
                SessionEvent::SftpListing { path, entries } => {
                    // N'applique le listage que s'il correspond au chemin courant
                    // (évite d'écraser une navigation effectuée entre-temps).
                    if tab.file_explorer.current_path == path {
                        tab.file_explorer.entries = entries;
                        tab.file_explorer.loading = false;
                        tab.file_explorer.loaded  = true;
                        tab.file_explorer.dir_error = None;
                    }
                }
                SessionEvent::SftpUid(uid) => {
                    tab.file_explorer.current_uid = Some(uid);
                }
                SessionEvent::SftpOpResult { op, ok, msg } => {
                    // Détecte les erreurs de listage de répertoire.
                    let listed_path = match &op {
                        SftpOp::List(path) => Some(path.clone()),
                        _ => None,
                    };
                    let op_text = op.text(&app.lang);

                    if ok {
                        tab.file_explorer.add_toast(op_text.to_string());
                        // Rafraîchit le répertoire courant après toute opération réussie.
                        let path = tab.file_explorer.current_path.clone();
                        tab.file_explorer.loading = true;
                        if let Some(session) = &tab.session {
                            session.send_sftp(SftpCommand::ListDir(path));
                        }
                    } else if listed_path.as_deref() == Some(tab.file_explorer.current_path.as_str()) {
                        // Erreur de listage du répertoire courant → accès refusé ou indisponible.
                        let label = if msg.to_lowercase().contains("permission")
                            || msg.to_lowercase().contains("denied")
                            || msg.contains("3")  // code SFTP 3 = permission denied
                        {
                            app.lang.sftp_access_denied.clone()
                        } else {
                            Lang::fmt(&app.lang.sftp_read_error, &[("msg", &msg)])
                        };
                        tab.file_explorer.loading   = false;
                        tab.file_explorer.dir_error = Some(label);
                    } else {
                        tab.file_explorer.add_toast(format!("{op_text} : {msg}"));
                    }
                }
            }
        }
    }
}

// ─── Téléchargement de langue en arrière-plan ────────────────────────────────

// ─── Charte Rusty Suite : réseau, langues, mise à jour ───────────────────────

impl BetterSshApp {
    /// Enregistre les réglages communs (langue, mises à jour).
    pub fn save_settings(&self) {
        if let Err(e) = rusty_core::settings::save(&self.work_dir, &self.settings) {
            log::warn!("Réglages non enregistrés : {e}");
        }
    }

    /// Recharge la langue choisie (textes de l'application et textes communs du kit).
    pub fn reload_language(&mut self, ctx: &Context) {
        let (lang, kit_lang) = crate::i18n::load_lang(&self.work_dir, &self.settings.common.language);
        self.lang = lang;
        self.kit_lang = kit_lang;
        let kit = self.kit_lang.clone();
        rusty_ui::set_texts(ctx, rusty_ui::KitTexts::from_lookup(|k| kit.t(k)));
    }

    /// Réseau, langues distantes, mise à jour, second lancement.
    fn poll_kit(&mut self, ctx: &Context) {
        if let Some(online) = self.net_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.net_rx = None;
            self.online = Some(online);
            if online {
                let (dir, ua) = (self.work_dir.clone(), user_agent());
                let (tx, rx) = mpsc::channel();
                std::thread::spawn(move || {
                    let _ = tx.send(rusty_core::lang::refresh_remote(APP_ID, crate::i18n::PACKS, &dir, &ua));
                });
                self.lang_rx = Some(rx);
                if self.settings.common.check_updates {
                    let (tx, rx) = mpsc::channel();
                    std::thread::spawn(move || {
                        let _ = tx.send(update::check(APP_ID, ASSET_BASE, VERSION, &user_agent()).map_err(|e| e.to_string()));
                    });
                    self.update_rx = Some(rx);
                }
            }
        }
        if let Some(Ok(n)) = self.lang_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.lang_rx = None;
            if n > 0 {
                self.reload_language(ctx);
            }
        }
        if let Some(result) = self.update_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.update_rx = None;
            match result {
                Ok(release) => self.update = release,
                Err(e) => log::warn!("Vérification des mises à jour impossible : {e}"),
            }
        }
        if let Some(result) = self.install_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.install_rx = None;
            match result {
                Ok(()) => {
                    if let Ok(exe) = std::env::current_exe() {
                        let _ = std::process::Command::new(exe).spawn();
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                Err(e) => {
                    self.show_update = false;
                    self.toasts.push(Tone::Error, self.kit_lang.t("kit.update_failed"), e);
                }
            }
        }
        // Second lancement : ramène la fenêtre ouverte au premier plan.
        if self.args_rx.try_iter().count() > 0 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    }

    /// Télécharge et installe la mise à jour (fil séparé).
    pub fn start_install(&mut self) {
        let Some(release) = self.update.clone() else { return };
        let (dir, ua) = (self.work_dir.clone(), user_agent());
        let checksum_text = self.kit_lang.t("kit.update_checksum");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let r = update::install(&release, VERSION, &dir, &ua).map_err(|e| match e {
                update::UpdateError::Checksum => checksum_text,
                other => other.to_string(),
            });
            let _ = tx.send(r);
        });
        self.install_rx = Some(rx);
    }
}

// ─── Configuration des polices ────────────────────────────────────────────────

/// Initialise les polices egui et définit les styles de texte de l'application.
/// `terminal_size` est la taille en points utilisée pour la police monospace.
pub fn setup_fonts(ctx: &Context, terminal_size: f32) {
    // Thème, polices (Inter, JetBrains Mono, icônes) et couleurs de la charte.
    rusty_ui::install(ctx);
    add_system_fallbacks(ctx);
    // Le terminal garde sa taille de police réglable dans les préférences.
    ctx.all_styles_mut(|style| {
        style.text_styles.insert(TextStyle::Monospace, FontId::monospace(terminal_size));
    });
}

/// Polices système ajoutées en dernier recours : un terminal distant peut afficher des
/// symboles, emoji ou caractères CJK absents d'Inter et de JetBrains Mono.
fn add_system_fallbacks(ctx: &Context) {
    use egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
    #[cfg(target_os = "windows")]
    let candidates: &[(&str, &str)] = &[
        ("C:/Windows/Fonts/seguisym.ttf", "SegoeSym"),
        ("C:/Windows/Fonts/seguiemj.ttf", "SegoeEmoji"),
        ("C:/Windows/Fonts/meiryo.ttc", "Meiryo"),
        ("C:/Windows/Fonts/simsun.ttc", "SimSun"),
    ];
    #[cfg(not(target_os = "windows"))]
    let candidates: &[(&str, &str)] = &[
        ("/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf", "NotoSans"),
        ("/usr/share/fonts/truetype/noto/NotoSansSymbols-Regular.ttf", "NotoSymbols"),
        ("/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf", "NotoEmoji"),
    ];
    for (path, name) in candidates {
        let Ok(bytes) = std::fs::read(path) else { continue };
        let families = [egui::FontFamily::Proportional, egui::FontFamily::Monospace]
            .into_iter()
            .map(|family| InsertFontFamily { family, priority: FontPriority::Lowest })
            .collect();
        ctx.add_font(FontInsert::new(name, egui::FontData::from_owned(bytes), families));
    }
}
