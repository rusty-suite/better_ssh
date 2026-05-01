/// Ressources statiques embarquées dans le binaire.
/// Toutes les constantes textuelles sont lues depuis Cargo.toml à la compilation
/// via `env!` — modifier Cargo.toml suffit, ce fichier ne nécessite aucune retouche.
///
/// Pour changer l'icône : remplacer assets/icon.png et assets/icon.ico.

/// Nom de l'application affiché dans la barre de titre et les menus.
/// (Le nom Cargo est en minuscules ; on garde la casse d'affichage ici.)
pub const APP_NAME: &str = "BetterSSH";

/// Version sémantique (ex. "0.1.0"), lue depuis Cargo.toml à la compilation.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Auteurs / mainteneurs déclarés dans Cargo.toml.
pub const APP_AUTHORS: &str = env!("CARGO_PKG_AUTHORS");

/// Description courte de l'application.
pub const APP_DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");

/// URL du dépôt GitHub officiel.
pub const APP_REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// Licence SPDX (ex. "MIT").
pub const APP_LICENSE: &str = env!("CARGO_PKG_LICENSE");

/// Octets bruts de l'icône PNG 256×256 (Linux, barre de titre egui).
pub const ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

/// Octets bruts de l'icône ICO multi-résolution (Windows .exe).
#[cfg(windows)]
pub const ICON_ICO: &[u8] = include_bytes!("../assets/icon.ico");
