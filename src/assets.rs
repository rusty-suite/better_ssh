/// Ressources statiques embarquées dans le binaire.
/// Pour changer le logo : remplacer assets/icon.png et assets/icon.ico
/// sans modifier ce fichier ni le code métier.

/// Nom de l'application affiché dans la barre de titre et les menus.
pub const APP_NAME: &str = "BetterSSH";

/// Version lue depuis Cargo.toml à la compilation.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Octets bruts de l'icône PNG 256×256 (Linux, barre de titre egui).
pub const ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

/// Octets bruts de l'icône ICO multi-résolution (Windows .exe).
#[cfg(windows)]
pub const ICON_ICO: &[u8] = include_bytes!("../assets/icon.ico");
