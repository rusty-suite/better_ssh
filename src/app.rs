/// Structure principale de l'application et boucle de mise à jour egui.
/// `BetterSshApp` contient tout l'état global : onglets ouverts, sidebar,
/// panneau de scan réseau, préférences, etc.
use crate::config::{AppConfig, AuthMethod, ConnectionProfile, Vault};
use crate::network::scanner::ScanResult;
use crate::ssh::session::{SessionCommand, SessionEvent, SshSession};
use crate::ui::{
    file_explorer::FileExplorerState,
    network_scan::NetworkScanState,
    sidebar::SidebarState,
    snippets::SnippetsState,
    system_monitor::SystemMonitorState,
    terminal::TerminalState,
};
use egui::{Context, FontId, TextStyle};

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
}

// ─── Onglet de session ────────────────────────────────────────────────────────

/// Représente une session SSH ouverte dans un onglet.
pub struct Tab {
    /// Identifiant unique (séquentiel) pour distinguer les onglets.
    pub id: usize,
    /// Profil de connexion associé à cet onglet.
    pub profile: ConnectionProfile,
    /// Session SSH active (None si pas encore connecté).
    pub session: Option<SshSession>,
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
    /// true = thème sombre actif.
    pub dark_mode: bool,
    /// Handle du runtime tokio partagé pour lancer des tâches async.
    pub tokio_rt: tokio::runtime::Handle,
    /// Vault de mots de passe déverrouillé pour cette session (None = verrouillé).
    pub vault: Option<Vault>,
    /// Dialogue de connexion en attente depuis le scan réseau (None = pas de dialogue).
    pub pending_scan_connect: Option<ScanConnectDialog>,
}

impl BetterSshApp {
    /// Initialise l'application : détection du thème OS, chargement config, polices.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let dark_mode = dark_light::detect() == dark_light::Mode::Dark;
        let config = AppConfig::load().unwrap_or_default();

        // Applique la police et la taille sauvegardées.
        setup_fonts(&cc.egui_ctx, config.terminal.font_size);
        apply_theme(&cc.egui_ctx, dark_mode);

        let rt = tokio::runtime::Handle::current();

        Self {
            sidebar: SidebarState::new(config.profiles.clone()),
            config,
            tabs: Vec::new(),
            active_tab: 0,
            next_tab_id: 1,
            snippets: SnippetsState::new(),
            network_scan: NetworkScanState::new(),
            show_snippets: false,
            show_network_scan: false,
            show_preferences: false,
            dark_mode,
            tokio_rt: rt,
            vault: None,
            pending_scan_connect: None,
        }
    }

    /// Ouvre un nouvel onglet pour le profil donné, démarre la session SSH et le rend actif.
    /// `password` est passé uniquement pour l'auth par mot de passe ; `None` sinon.
    pub fn open_profile(&mut self, profile: ConnectionProfile, password: Option<String>) {
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        let mut tab = Tab::new(id, profile.clone());

        // Lance la session SSH en arrière-plan immédiatement.
        tab.session = Some(SshSession::connect(profile, password));

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

    /// Persiste la configuration courante sur le disque.
    pub fn save_config(&mut self) {
        self.config.profiles = self.sidebar.profiles.clone();
        if let Err(e) = self.config.save() {
            log::error!("Erreur lors de la sauvegarde de la configuration : {e}");
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
        poll_session_events(self);
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
            if let Some(tab) = app.tabs.get_mut(app.active_tab) {
                tab.show_file_explorer = !tab.show_file_explorer;
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
                    tab.terminal.feed(b"\x1b[32mConnexion SSH etablie.\x1b[0m\r\n");
                }
                SessionEvent::Data(data) => {
                    tab.terminal.feed(&data);
                }
                SessionEvent::Disconnected(msg) => {
                    tab.connected = false;
                    let text = format!("\r\n\x1b[33m[Session terminée : {}]\x1b[0m\r\n", msg);
                    tab.terminal.feed(text.as_bytes());
                    tab.session = None;
                }
                SessionEvent::Error(err) => {
                    tab.connected = false;
                    let text = format!("\r\n\x1b[31mErreur SSH : {}\x1b[0m\r\n", err);
                    tab.terminal.feed(text.as_bytes());
                    tab.session = None;
                }
                SessionEvent::FingerprintAlert { host, fingerprint } => {
                    log::warn!("Alerte fingerprint MITM : {host} — {fingerprint}");
                }
            }
        }
    }
}

// ─── Configuration des polices ────────────────────────────────────────────────

/// Initialise les polices egui et définit les styles de texte de l'application.
/// `terminal_size` est la taille en points utilisée pour la police monospace.
pub fn setup_fonts(ctx: &Context, terminal_size: f32) {
    // Garde les polices par défaut d'egui (Ubuntu pour le texte, Hack pour le code).
    let fonts = egui::FontDefinitions::default();
    ctx.set_fonts(fonts);

    // Ajuste les tailles de style tout en respectant le ratio terminal/UI.
    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (TextStyle::Heading,   FontId::proportional(18.0)),
        (TextStyle::Body,      FontId::proportional(14.0)),
        // La police monospace sert au terminal et aux zones de code.
        (TextStyle::Monospace, FontId::monospace(terminal_size)),
        (TextStyle::Button,    FontId::proportional(14.0)),
        (TextStyle::Small,     FontId::proportional(11.0)),
    ]
    .into();
    ctx.set_style(style);
}

/// Applique le thème clair ou sombre à l'interface.
pub fn apply_theme(ctx: &Context, dark: bool) {
    if dark {
        ctx.set_visuals(egui::Visuals::dark());
    } else {
        ctx.set_visuals(egui::Visuals::light());
    }
}
