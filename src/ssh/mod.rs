/// Module SSH : gestion des sessions, authentification par clé et SFTP.
/// Toutes les opérations réseau tournent dans des tokio tasks séparées du
/// thread UI pour ne jamais bloquer l'interface.
pub mod key_auth;
pub mod session;
pub mod sftp;
