/// Module de configuration : profils SSH, vault chiffré, paramètres globaux.
pub mod profile;
pub mod vault;

pub use profile::{AppConfig, AuthMethod, ConnectionProfile};
pub use vault::{MasterKeyCheck, Vault};
