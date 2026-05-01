use anyhow::{Context, Result};
use russh::keys::key::KeyPair;
use std::path::PathBuf;

pub async fn load_key(path: &str, passphrase: Option<&str>) -> Result<KeyPair> {
    let expanded = shellexpand::tilde(path).into_owned();
    let path_buf = PathBuf::from(&expanded);
    let pem = tokio::fs::read(&path_buf)
        .await
        .with_context(|| format!("reading key {}", path_buf.display()))?;
    let pem_str = std::str::from_utf8(&pem).context("key is not valid UTF-8")?;
    russh::keys::decode_secret_key(pem_str, passphrase).context("decoding private key")
}

pub fn default_key_paths() -> Vec<String> {
    let home = dirs::home_dir().unwrap_or_default();
    vec![
        home.join(".ssh").join("id_ed25519").to_string_lossy().into_owned(),
        home.join(".ssh").join("id_rsa").to_string_lossy().into_owned(),
        home.join(".ssh").join("id_ecdsa").to_string_lossy().into_owned(),
    ]
}
