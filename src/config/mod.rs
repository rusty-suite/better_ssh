/// Module de configuration : profils SSH, vault chiffré, paramètres globaux.
pub mod profile;
pub mod vault;

pub use profile::{AppConfig, AuthMethod, ConnectionProfile, NetworkDefaults, TerminalConfig};
pub use vault::Vault;
