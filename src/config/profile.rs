/// Profils de connexion SSH et configuration globale de l'application.
/// Sauvegarde dans ~/.rustshell/config.toml et ~/.rustshell/profiles.toml
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use dirs::home_dir;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ─── Méthode d'authentification ───────────────────────────────────────────────

/// Méthode utilisée pour s'authentifier auprès du serveur SSH.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AuthMethod {
    /// Mot de passe chiffré dans le vault local (age).
    Password,
    /// Clé privée ed25519 ou RSA lue depuis le disque.
    PublicKey { identity_file: String },
    /// Délégation à l'agent SSH système (SSH_AUTH_SOCK / Pageant).
    Agent,
}

impl Default for AuthMethod {
    fn default() -> Self {
        Self::Password
    }
}

impl std::fmt::Display for AuthMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Password => write!(f, "Mot de passe"),
            Self::PublicKey { identity_file } => write!(f, "Clé : {identity_file}"),
            Self::Agent => write!(f, "Agent SSH"),
        }
    }
}

// ─── Profil de connexion ──────────────────────────────────────────────────────

/// Représente une connexion SSH sauvegardée.
/// Tous les champs sont persistés dans profiles.toml (sauf le mot de passe
/// qui va dans vault.toml chiffré avec age).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionProfile {
    /// Identifiant unique généré à la création du profil.
    pub id: String,
    /// Nom affiché dans la barre latérale (ex: "Serveur Web Prod").
    pub name: String,
    /// Adresse IP ou nom d'hôte DNS.
    pub host: String,
    /// Port SSH (22 par défaut).
    pub port: u16,
    /// Nom d'utilisateur pour la connexion.
    pub username: String,
    /// Méthode d'authentification choisie.
    pub auth_method: AuthMethod,
    /// Étiquettes pour grouper / filtrer les profils.
    pub tags: Vec<String>,
    /// Couleur d'accentuation RGB affichée dans la barre latérale.
    pub color_tag: Option<[u8; 3]>,
    /// Horodatage de la dernière connexion réussie.
    pub last_connected: Option<DateTime<Utc>>,
    /// Hôte intermédiaire (bastion / ProxyJump).
    pub jump_host: Option<String>,
    /// Délai max avant d'abandonner la connexion.
    pub connection_timeout_secs: u64,
}

impl Default for ConnectionProfile {
    fn default() -> Self {
        Self {
            id: uuid(),
            name: String::new(),
            host: String::new(),
            port: 22,
            username: String::new(),
            auth_method: AuthMethod::default(),
            tags: Vec::new(),
            color_tag: None,
            last_connected: None,
            jump_host: None,
            connection_timeout_secs: 30,
        }
    }
}

impl ConnectionProfile {
    /// Crée un profil minimal avec nom, hôte et utilisateur.
    pub fn new(name: impl Into<String>, host: impl Into<String>, username: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            host: host.into(),
            username: username.into(),
            ..Default::default()
        }
    }

    /// Retourne le nom à afficher : le nom personnalisé si défini, sinon l'hôte.
    pub fn display_name(&self) -> &str {
        if self.name.is_empty() { &self.host } else { &self.name }
    }
}

// ─── Configuration globale ────────────────────────────────────────────────────

/// Configuration complète de l'application, sérialisée en TOML.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AppConfig {
    /// Liste des profils de connexion (écrit dans profiles.toml séparé).
    pub profiles: Vec<ConnectionProfile>,
    /// Paramètres visuels (thème, couleurs).
    pub theme: ThemeConfig,
    /// Paramètres du terminal (police, défilement).
    pub terminal: TerminalConfig,
    /// Paramètres par défaut du scan réseau.
    pub network: NetworkDefaults,
}

/// Paramètres d'apparence de l'interface.
#[derive(Debug, Serialize, Deserialize)]
pub struct ThemeConfig {
    /// true = thème sombre, false = thème clair.
    pub dark_mode: bool,
    /// Palette de couleurs du terminal ("Dracula", "Solarized", "One Dark", "Custom").
    pub terminal_theme: String,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self { dark_mode: true, terminal_theme: "Dracula".into() }
    }
}

/// Paramètres de la police et du défilement du terminal.
#[derive(Debug, Serialize, Deserialize)]
pub struct TerminalConfig {
    /// Taille de la police en points (8.0 – 32.0).
    pub font_size: f32,
    /// Nombre maximum de lignes gardées en mémoire pour le défilement.
    pub scrollback_lines: usize,
    /// Préréglage nommé affiché dans les préférences ("Petite", "Normale", etc.).
    pub font_preset: String,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            font_size: 13.0,
            scrollback_lines: 10_000,
            font_preset: "Normale".into(),
        }
    }
}

/// Paramètres réseau par défaut pour le scanner.
#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkDefaults {
    /// Plage CIDR ou étendue à scanner (ex: "192.168.1.0/24").
    pub target_cidr: String,
    /// Port SSH à tester sur chaque hôte.
    pub ssh_port: u16,
    /// Délai de connexion TCP par hôte en millisecondes.
    pub timeout_ms: u64,
    /// Nombre de connexions simultanées lors du scan.
    pub concurrency: usize,
}

impl Default for NetworkDefaults {
    fn default() -> Self {
        Self {
            // Valeur générique ; remplacée par auto-détection au démarrage.
            target_cidr: "192.168.1.0/24".into(),
            ssh_port: 22,
            timeout_ms: 500,
            concurrency: 64,
        }
    }
}

// ─── Chemins et persistance ───────────────────────────────────────────────────

impl AppConfig {
    /// Répertoire de configuration de l'application (`~/.betterssh/`).
    pub fn config_dir() -> PathBuf {
        home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".betterssh")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn profiles_path() -> PathBuf {
        Self::config_dir().join("profiles.toml")
    }

    /// Charge la configuration depuis le disque.
    /// Retourne les valeurs par défaut si le fichier n'existe pas.
    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("lecture de {}", path.display()))?;
        let mut cfg: Self = toml::from_str(&text)
            .with_context(|| format!("parsing de {}", path.display()))?;

        // Les profils sont dans un fichier séparé pour éviter les conflits de merge.
        let ppath = Self::profiles_path();
        if ppath.exists() {
            let ptxt = std::fs::read_to_string(&ppath)?;
            #[derive(Deserialize)]
            struct ProfileList { profiles: Vec<ConnectionProfile> }
            if let Ok(pl) = toml::from_str::<ProfileList>(&ptxt) {
                cfg.profiles = pl.profiles;
            }
        }
        Ok(cfg)
    }

    /// Persiste la configuration et les profils sur le disque.
    pub fn save(&self) -> Result<()> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir)?;

        // Config principale (sans les profils pour éviter la duplication).
        let text = toml::to_string_pretty(self)?;
        std::fs::write(Self::config_path(), text)?;

        // Profils dans leur propre fichier.
        #[derive(Serialize)]
        struct ProfileList<'a> { profiles: &'a Vec<ConnectionProfile> }
        let ptxt = toml::to_string_pretty(&ProfileList { profiles: &self.profiles })?;
        std::fs::write(Self::profiles_path(), ptxt)?;

        Ok(())
    }
}

// ─── Utilitaires ──────────────────────────────────────────────────────────────

/// Génère un UUID v4 simplifié (16 octets aléatoires en hex).
fn uuid() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        rng.gen::<u32>(),
        rng.gen::<u16>(),
        rng.gen::<u16>(),
        rng.gen::<u16>(),
        rng.gen::<u64>() & 0xffffffffffff_u64,
    )
}
