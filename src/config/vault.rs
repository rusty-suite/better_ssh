/// Vault chiffré pour les secrets SSH (mot de passe, hôte, nom d'utilisateur).
/// Utilise la bibliothèque `age` (chiffrement asymétrique/passphrase).
/// Les entrées sont encodées en base64 et stockées dans vault.toml.
/// Un SHA-256 du mot de passe maître est stocké pour vérifier la clé avant déchiffrement.
use age::secrecy::SecretString;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Read, Write};

use super::AppConfig;

// ─── Structure de données persistée ──────────────────────────────────────────

/// Secrets chiffrés associés à un profil.
/// Format vault.toml par entrée :
///   [entries.<profile-id>]
///   address  = "<base64(age(ip_ou_hostname))>"
///   username = "<base64(age(login))>"
///   password = "<base64(age(mot_de_passe))>"
#[derive(Debug, Default, Serialize, Deserialize)]
struct VaultEntry {
    /// Adresse IP ou nom d'hôte chiffré (clé TOML : `address`).
    #[serde(skip_serializing_if = "Option::is_none")]
    address: Option<String>,
    /// Nom d'utilisateur SSH chiffré (clé TOML : `username`).
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<String>,
    /// Mot de passe SSH chiffré (clé TOML : `password`).
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<String>,
}

impl VaultEntry {
    fn is_empty(&self) -> bool {
        self.address.is_none() && self.username.is_none() && self.password.is_none()
    }
}

/// Contenu du fichier vault.toml : map profile_id → secrets chiffrés.
#[derive(Debug, Default, Serialize, Deserialize)]
struct VaultData {
    /// SHA-256 (hex) du mot de passe maître.
    /// Absent sur les vaults créés avant cette version → accepté sans vérification.
    #[serde(skip_serializing_if = "Option::is_none")]
    key_hash: Option<String>,
    entries: HashMap<String, VaultEntry>,
}

// ─── Résultat de la vérification de clé maître ───────────────────────────────

/// Résultat retourné par [`Vault::master_key_ok`].
pub enum MasterKeyCheck {
    /// Le hash correspond → clé correcte.
    Ok,
    /// Le hash ne correspond pas → clé incorrecte.
    Wrong,
    /// Aucun hash stocké (vault vierge ou format ancien) → indéterminé, autorisé.
    Unknown,
}

// ─── API publique ─────────────────────────────────────────────────────────────

/// Gère le chiffrement/déchiffrement des secrets SSH avec une clé maître.
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

    // ─── Vérification de la clé maître ───────────────────────────────────────

    /// Vérifie si le mot de passe maître correspond au hash stocké.
    /// - `Ok`      → hash présent et correspondant.
    /// - `Wrong`   → hash présent mais différent → mot de passe incorrect.
    /// - `Unknown` → aucun hash stocké (vault vierge/ancien) → autorisé.
    pub fn master_key_ok(&self) -> Result<MasterKeyCheck> {
        let data = self.load_data()?;
        match &data.key_hash {
            None    => {
                log::info!("Vault : aucun hash stocké — accès autorisé sans vérification");
                Ok(MasterKeyCheck::Unknown)
            }
            Some(h) => {
                if *h == sha256_hex(&self.master_key) {
                    log::info!("Vault : clé maître vérifiée avec succès");
                    Ok(MasterKeyCheck::Ok)
                } else {
                    log::warn!("Vault : tentative de déverrouillage avec un mot de passe incorrect");
                    Ok(MasterKeyCheck::Wrong)
                }
            }
        }
    }

    // ─── Mot de passe ─────────────────────────────────────────────────────────

    /// Chiffre et stocke le mot de passe pour un profil.
    pub fn store_password(&self, profile_id: &str, password: &str) -> Result<()> {
        let mut data = self.load_data()?;
        data.entries.entry(profile_id.to_string()).or_default().password =
            Some(self.encrypt(password)?);
        self.save_data(&mut data)
    }

    /// Retourne le mot de passe déchiffré d'un profil, ou None s'il n'existe pas.
    pub fn get_password(&self, profile_id: &str) -> Result<Option<String>> {
        let data = self.load_data()?;
        self.decrypt_field(data.entries.get(profile_id).and_then(|e| e.password.as_deref()))
    }

    // ─── Adresse ──────────────────────────────────────────────────────────────

    /// Chiffre et stocke l'adresse IP/hostname pour un profil.
    pub fn store_address(&self, profile_id: &str, address: &str) -> Result<()> {
        let mut data = self.load_data()?;
        data.entries.entry(profile_id.to_string()).or_default().address =
            Some(self.encrypt(address)?);
        self.save_data(&mut data)
    }

    /// Retourne l'adresse IP/hostname déchiffrée d'un profil, ou None si absente.
    pub fn get_address(&self, profile_id: &str) -> Result<Option<String>> {
        let data = self.load_data()?;
        self.decrypt_field(data.entries.get(profile_id).and_then(|e| e.address.as_deref()))
    }

    // ─── Nom d'utilisateur ────────────────────────────────────────────────────

    /// Chiffre et stocke le nom d'utilisateur pour un profil.
    pub fn store_username(&self, profile_id: &str, username: &str) -> Result<()> {
        let mut data = self.load_data()?;
        data.entries.entry(profile_id.to_string()).or_default().username =
            Some(self.encrypt(username)?);
        self.save_data(&mut data)
    }

    /// Retourne le nom d'utilisateur déchiffré d'un profil, ou None si absent.
    pub fn get_username(&self, profile_id: &str) -> Result<Option<String>> {
        let data = self.load_data()?;
        self.decrypt_field(data.entries.get(profile_id).and_then(|e| e.username.as_deref()))
    }

    // ─── Suppression ─────────────────────────────────────────────────────────

    /// Supprime uniquement le mot de passe d'un profil dans le vault.
    pub fn remove_password(&self, profile_id: &str) -> Result<()> {
        let mut data = self.load_data()?;
        if let Some(entry) = data.entries.get_mut(profile_id) {
            entry.password = None;
            if entry.is_empty() { data.entries.remove(profile_id); }
        }
        self.save_data(&mut data)
    }

    /// Retourne true si le profil a des entrées chiffrées dans vault.toml
    /// (sans déchiffrer — utilisable même vault verrouillé).
    pub fn profile_has_encrypted_data(profile_id: &str) -> bool {
        let path = AppConfig::config_dir().join("vault.toml");
        if !path.exists() { return false; }
        let Ok(text) = std::fs::read_to_string(&path) else { return false; };
        let data: VaultData = toml::from_str(&text).unwrap_or_default();
        data.entries.contains_key(profile_id)
    }

    /// Supprime tous les secrets d'un profil (appelé quand le profil est supprimé).
    pub fn remove_profile(&self, profile_id: &str) -> Result<()> {
        let mut data = self.load_data()?;
        data.entries.remove(profile_id);
        self.save_data(&mut data)
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

    /// Déchiffre un champ optionnel (factorise `get_*`).
    fn decrypt_field(&self, enc: Option<&str>) -> Result<Option<String>> {
        match enc {
            None      => Ok(None),
            Some(enc) => Ok(Some(self.decrypt(enc)?)),
        }
    }

    // ─── Persistance ─────────────────────────────────────────────────────────

    fn load_data(&self) -> Result<VaultData> {
        if !self.path.exists() {
            return Ok(VaultData::default());
        }
        let text = std::fs::read_to_string(&self.path)?;
        Ok(toml::from_str(&text).unwrap_or_default())
    }

    fn save_data(&self, data: &mut VaultData) -> Result<()> {
        // Stocke le hash de la clé maître à la première écriture.
        if data.key_hash.is_none() {
            data.key_hash = Some(sha256_hex(&self.master_key));
        }
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, toml::to_string_pretty(data)?)?;
        Ok(())
    }
}

// ─── Helper interne ───────────────────────────────────────────────────────────

fn sha256_hex(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    format!("{:x}", h.finalize())
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
    fn round_trip_host_username() {
        let (vault, _dir) = temp_vault();
        vault.store_address("profil-1", "192.168.1.10").unwrap();
        vault.store_username("profil-1", "alice").unwrap();
        assert_eq!(vault.get_address("profil-1").unwrap(), Some("192.168.1.10".to_string()));
        assert_eq!(vault.get_username("profil-1").unwrap(), Some("alice".to_string()));
    }

    #[test]
    fn remove_profile_purge_toutes_les_entrees() {
        let (vault, _dir) = temp_vault();
        vault.store_password("profil-1", "pwd").unwrap();
        vault.store_address("profil-1", "10.0.0.1").unwrap();
        vault.remove_profile("profil-1").unwrap();
        assert!(vault.get_password("profil-1").unwrap().is_none());
        assert!(vault.get_address("profil-1").unwrap().is_none());
    }

    #[test]
    fn profil_absent_retourne_none() {
        let (vault, _dir) = temp_vault();
        assert!(vault.get_password("inexistant").unwrap().is_none());
        assert!(vault.get_address("inexistant").unwrap().is_none());
        assert!(vault.get_username("inexistant").unwrap().is_none());
    }
}
