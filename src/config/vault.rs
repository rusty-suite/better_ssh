/// Vault chiffré pour les mots de passe SSH.
/// Utilise la bibliothèque `age` (chiffrement asymétrique/passphrase).
/// Les entrées sont encodées en base64 et stockées dans vault.toml.
use age::secrecy::SecretString;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};

use super::AppConfig;

// ─── Structure de données persistée ──────────────────────────────────────────

/// Contenu du fichier vault.toml : map profile_id → données chiffrées (base64).
#[derive(Debug, Default, Serialize, Deserialize)]
struct VaultData {
    entries: HashMap<String, String>,
}

// ─── API publique ─────────────────────────────────────────────────────────────

/// Gère le chiffrement/déchiffrement des mots de passe avec une clé maître.
/// La clé maître elle-même n'est pas stockée sur disque.
pub struct Vault {
    /// Chemin vers vault.toml (modifiable dans les tests).
    path: std::path::PathBuf,
    /// Phrase secrète maître utilisée pour chiffrer/déchiffrer avec age.
    master_key: String,
}

impl Vault {
    /// Crée un nouveau vault avec la clé maître donnée.
    pub fn new(master_key: impl Into<String>) -> Self {
        Self {
            path: AppConfig::config_dir().join("vault.toml"),
            master_key: master_key.into(),
        }
    }

    /// Chiffre et stocke un mot de passe pour un profil.
    pub fn store_password(&self, profile_id: &str, password: &str) -> Result<()> {
        let mut data = self.load_data()?;
        let encrypted = self.encrypt(password)?;
        data.entries.insert(profile_id.to_string(), encrypted);
        self.save_data(&data)
    }

    /// Retourne le mot de passe déchiffré d'un profil, ou None s'il n'existe pas.
    pub fn get_password(&self, profile_id: &str) -> Result<Option<String>> {
        let data = self.load_data()?;
        match data.entries.get(profile_id) {
            None => Ok(None),
            Some(enc) => Ok(Some(self.decrypt(enc)?)),
        }
    }

    /// Supprime l'entrée d'un profil du vault.
    pub fn remove_password(&self, profile_id: &str) -> Result<()> {
        let mut data = self.load_data()?;
        data.entries.remove(profile_id);
        self.save_data(&data)
    }

    // ─── Chiffrement interne ──────────────────────────────────────────────────

    /// Chiffre `plaintext` avec la clé maître puis encode en base64.
    fn encrypt(&self, plaintext: &str) -> Result<String> {
        let passphrase = SecretString::from(self.master_key.clone());
        let encryptor = age::Encryptor::with_user_passphrase(passphrase);
        let mut encrypted = Vec::new();
        let mut writer = encryptor
            .wrap_output(&mut encrypted)
            .context("création du chiffreur age")?;
        writer.write_all(plaintext.as_bytes())?;
        writer.finish()?;
        Ok(base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &encrypted,
        ))
    }

    /// Décode le base64 puis déchiffre avec la clé maître.
    fn decrypt(&self, encoded: &str) -> Result<String> {
        let passphrase = SecretString::from(self.master_key.clone());
        let raw = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            encoded,
        )
        .context("décodage base64")?;
        let decryptor = age::Decryptor::new_buffered(raw.as_slice())
            .context("création du déchiffreur age")?;
        let mut reader = match decryptor {
            age::Decryptor::Passphrase(d) => d
                .decrypt(&passphrase, Some(20))
                .context("déchiffrement age passphrase")?,
            _ => anyhow::bail!("type de déchiffreur age inattendu"),
        };
        let mut plaintext = String::new();
        reader.read_to_string(&mut plaintext)?;
        Ok(plaintext)
    }

    // ─── Persistance ─────────────────────────────────────────────────────────

    fn load_data(&self) -> Result<VaultData> {
        if !self.path.exists() {
            return Ok(VaultData::default());
        }
        let text = std::fs::read_to_string(&self.path)?;
        Ok(toml::from_str(&text).unwrap_or_default())
    }

    fn save_data(&self, data: &VaultData) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, toml::to_string_pretty(data)?)?;
        Ok(())
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_vault() -> (Vault, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let v = Vault {
            path: dir.path().join("vault.toml"),
            master_key: "clé-test-123".into(),
        };
        (v, dir)
    }

    #[test]
    fn round_trip_password() {
        let (vault, _dir) = temp_vault();
        vault.store_password("profil-1", "s3cr3t!").unwrap();
        let got = vault.get_password("profil-1").unwrap();
        assert_eq!(got, Some("s3cr3t!".to_string()));
    }

    #[test]
    fn profil_absent_retourne_none() {
        let (vault, _dir) = temp_vault();
        assert!(vault.get_password("inexistant").unwrap().is_none());
    }
}
