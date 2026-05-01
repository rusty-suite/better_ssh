/// Vault chiffré pour les secrets SSH (mot de passe, hôte, nom d'utilisateur).
///
/// # Format v2 (actuel)
/// - Dérivation de clé : **Argon2id** (m=32 MiB, t=2, p=1) avec sel 128 bits aléatoire.
///   La dérivation s'exécute **une seule fois** au déverrouillage (~50-100 ms).
/// - Chiffrement : **ChaCha20-Poly1305** avec nonce 96 bits aléatoire par message.
///   Chaque chiffrement/déchiffrement est ensuite instantané.
/// - Vérification de clé : tentative de déchiffrement d'un jeton connu (`key_verify`).
///   Aucun hash non salé → pas de table arc-en-ciel possible.
/// - Stockage : vault.toml (texte TOML, champs encodés en base64).
///
/// # Format v1 (legacy, age)
/// - Migré automatiquement vers v2 au premier déverrouillage.
/// - Le code age est conservé uniquement pour cette migration.
use anyhow::{bail, Context, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Read;

use super::AppConfig;

// ─── Constante de vérification ────────────────────────────────────────────────

const KEY_VERIFY_TOKEN: &str = "betterssh-vault-v2";

// ─── Structures de données persistées ────────────────────────────────────────

#[derive(Debug, Default, Serialize, Deserialize)]
struct VaultEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<String>,
    /// Historique de commandes : JSON de Vec<String> chiffré (20 max, plus récent en tête).
    #[serde(skip_serializing_if = "Option::is_none")]
    history: Option<String>,
}

impl VaultEntry {
    fn is_empty(&self) -> bool {
        self.address.is_none()
            && self.username.is_none()
            && self.password.is_none()
            && self.history.is_none()
    }
}

/// Contenu de vault.toml.
#[derive(Debug, Default, Serialize, Deserialize)]
struct VaultData {
    /// Version du format : 2 = ChaCha20+Argon2id (actuel), absent/1 = age (legacy).
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<u32>,
    /// Sel Argon2id (16 octets encodés base64). Absent = format v1.
    #[serde(skip_serializing_if = "Option::is_none")]
    kdf_salt: Option<String>,
    /// Jeton de vérification : ChaCha20-Poly1305(key, KEY_VERIFY_TOKEN) en base64.
    /// Permet de valider la clé sans stocker de hash non salé.
    #[serde(skip_serializing_if = "Option::is_none")]
    key_verify: Option<String>,
    /// Champ legacy (SHA-256 non salé du mot de passe, format v1). Supprimé à la migration.
    #[serde(skip_serializing_if = "Option::is_none")]
    key_hash: Option<String>,
    entries: HashMap<String, VaultEntry>,
}

// ─── Résultat de vérification de clé ─────────────────────────────────────────

pub enum MasterKeyCheck {
    /// Clé correcte (déchiffrement du jeton réussi).
    Ok,
    /// Clé incorrecte (déchiffrement échoué).
    Wrong,
    /// Vault vierge ou ancien sans jeton → autorisé sans vérification.
    Unknown,
}

// ─── API publique ─────────────────────────────────────────────────────────────

/// Gère le chiffrement/déchiffrement des secrets SSH avec une clé maître.
///
/// La clé symétrique 256 bits est dérivée par Argon2id une seule fois
/// à la construction, puis utilisée pour toutes les opérations (instantanées).
#[derive(Clone)]
pub struct Vault {
    path: std::path::PathBuf,
    /// Sel KDF (128 bits). Chargé depuis vault.toml ou généré à la première utilisation.
    salt: [u8; 16],
    /// Clé ChaCha20-Poly1305 dérivée (256 bits). Jamais persistée sur disque.
    key: [u8; 32],
    /// Passphrase conservée uniquement pour la migration v1→v2 (déchiffrement age).
    passphrase: String,
}

impl Vault {
    /// Crée un Vault et dérive la clé depuis la passphrase + sel.
    ///
    /// Si vault.toml existe avec un `kdf_salt`, ce sel est réutilisé.
    /// Sinon un nouveau sel aléatoire est généré (stocké à la première écriture).
    ///
    /// **Coût unique** : Argon2id ~50-100 ms. Toutes les opérations suivantes
    /// (chiffrement, déchiffrement) sont instantanées (ChaCha20-Poly1305).
    pub fn new(master_key: impl Into<String>) -> Self {
        let passphrase: String = master_key.into();
        let path = AppConfig::config_dir().join("vault.toml");

        let salt = load_salt_from_file(&path).unwrap_or_else(|| {
            let mut s = [0u8; 16];
            rand::thread_rng().fill_bytes(&mut s);
            s
        });

        let key = derive_key(&passphrase, &salt);
        Self { path, salt, key, passphrase }
    }

    // ─── Vérification de clé maître ──────────────────────────────────────────

    /// Vérifie la clé maître en tentant de déchiffrer le jeton de vérification.
    /// - `Ok`      → déchiffrement réussi (clé correcte).
    /// - `Wrong`   → déchiffrement échoué (clé incorrecte).
    /// - `Unknown` → pas encore de jeton (vault vierge ou ancien sans hash).
    pub fn master_key_ok(&self) -> Result<MasterKeyCheck> {
        let data = self.load_data()?;

        // Format v2 : vérifie via key_verify (pas de hash non salé → résistant aux rainbow tables).
        if data.version == Some(2) || data.kdf_salt.is_some() {
            return match &data.key_verify {
                None => {
                    log::info!("Vault v2 sans jeton de vérification — accès autorisé");
                    Ok(MasterKeyCheck::Unknown)
                }
                Some(v) => match self.decrypt(v) {
                    Ok(pt) if pt == KEY_VERIFY_TOKEN => {
                        log::info!("Vault v2 : clé vérifiée");
                        Ok(MasterKeyCheck::Ok)
                    }
                    _ => {
                        log::warn!("Vault v2 : clé incorrecte");
                        Ok(MasterKeyCheck::Wrong)
                    }
                },
            };
        }

        // Format v1 (age) : vérification par SHA-256 legacy (uniquement pour détection/migration).
        match &data.key_hash {
            None => {
                log::info!("Vault v1 sans hash — accès autorisé (migration requise)");
                Ok(MasterKeyCheck::Unknown)
            }
            Some(h) if *h == sha256_hex(&self.passphrase) => {
                log::info!("Vault v1 : hash SHA-256 vérifié — migration vers v2 requise");
                Ok(MasterKeyCheck::Ok)
            }
            _ => {
                log::warn!("Vault v1 : clé incorrecte");
                Ok(MasterKeyCheck::Wrong)
            }
        }
    }

    /// Migre le vault du format v1 (age) vers le format v2 (ChaCha20+Argon2id).
    ///
    /// Retourne `true` si une migration a eu lieu. À appeler après un unlock réussi.
    pub fn migrate_if_needed(&self) -> Result<bool> {
        let mut data = self.load_data()?;

        // Déjà v2 → rien à faire.
        if data.version == Some(2) || data.kdf_salt.is_some() {
            return Ok(false);
        }

        if data.entries.is_empty() {
            // Vault vide v1 → met juste à jour le format sans rien à déchiffrer.
            self.save_data(&mut data)?;
            return Ok(true);
        }

        log::info!("Vault : migration v1 (age) → v2 (ChaCha20+Argon2id) — {} profils…",
            data.entries.len());

        let mut migrated = HashMap::new();
        for (profile_id, entry) in &data.entries {
            migrated.insert(profile_id.clone(), VaultEntry {
                address:  self.migrate_age_field(entry.address.as_deref())?,
                username: self.migrate_age_field(entry.username.as_deref())?,
                password: self.migrate_age_field(entry.password.as_deref())?,
                history:  self.migrate_age_field(entry.history.as_deref())?,
            });
        }
        data.entries  = migrated;
        data.key_hash = None;
        self.save_data(&mut data)?;
        log::info!("Vault : migration v2 terminée");
        Ok(true)
    }

    // ─── Stockage en masse (1 seule écriture disque) ──────────────────────────

    /// Chiffre et stocke plusieurs champs d'un profil en **une seule écriture disque**.
    /// Seuls les champs `Some` sont mis à jour ; les autres restent inchangés.
    pub fn store_profile(
        &self,
        profile_id: &str,
        address:  Option<&str>,
        username: Option<&str>,
        password: Option<&str>,
    ) -> Result<()> {
        let mut data = self.load_data()?;
        let entry = data.entries.entry(profile_id.to_string()).or_default();
        if let Some(a) = address  { entry.address  = Some(self.encrypt(a)?); }
        if let Some(u) = username { entry.username = Some(self.encrypt(u)?); }
        if let Some(p) = password { entry.password = Some(self.encrypt(p)?); }
        self.save_data(&mut data)
    }

    /// Déchiffre (address, username, password) en **une seule lecture disque**.
    pub fn get_profile(&self, profile_id: &str)
        -> Result<(Option<String>, Option<String>, Option<String>)>
    {
        let data = self.load_data()?;
        let e = data.entries.get(profile_id);
        Ok((
            self.decrypt_field(e.and_then(|x| x.address.as_deref()))?,
            self.decrypt_field(e.and_then(|x| x.username.as_deref()))?,
            self.decrypt_field(e.and_then(|x| x.password.as_deref()))?,
        ))
    }

    // ─── Champs individuels ───────────────────────────────────────────────────

    pub fn store_password(&self, profile_id: &str, password: &str) -> Result<()> {
        let mut data = self.load_data()?;
        data.entries.entry(profile_id.to_string()).or_default().password =
            Some(self.encrypt(password)?);
        self.save_data(&mut data)
    }

    pub fn get_password(&self, profile_id: &str) -> Result<Option<String>> {
        let data = self.load_data()?;
        self.decrypt_field(data.entries.get(profile_id).and_then(|e| e.password.as_deref()))
    }

    pub fn store_address(&self, profile_id: &str, address: &str) -> Result<()> {
        let mut data = self.load_data()?;
        data.entries.entry(profile_id.to_string()).or_default().address =
            Some(self.encrypt(address)?);
        self.save_data(&mut data)
    }

    pub fn get_address(&self, profile_id: &str) -> Result<Option<String>> {
        let data = self.load_data()?;
        self.decrypt_field(data.entries.get(profile_id).and_then(|e| e.address.as_deref()))
    }

    pub fn store_username(&self, profile_id: &str, username: &str) -> Result<()> {
        let mut data = self.load_data()?;
        data.entries.entry(profile_id.to_string()).or_default().username =
            Some(self.encrypt(username)?);
        self.save_data(&mut data)
    }

    pub fn get_username(&self, profile_id: &str) -> Result<Option<String>> {
        let data = self.load_data()?;
        self.decrypt_field(data.entries.get(profile_id).and_then(|e| e.username.as_deref()))
    }

    // ─── Suppression ──────────────────────────────────────────────────────────

    pub fn remove_password(&self, profile_id: &str) -> Result<()> {
        let mut data = self.load_data()?;
        if let Some(entry) = data.entries.get_mut(profile_id) {
            entry.password = None;
            if entry.is_empty() { data.entries.remove(profile_id); }
        }
        self.save_data(&mut data)
    }

    /// Retourne true si le profil a des entrées dans vault.toml (sans déchiffrer).
    pub fn profile_has_encrypted_data(profile_id: &str) -> bool {
        let path = AppConfig::config_dir().join("vault.toml");
        if !path.exists() { return false; }
        let Ok(text) = std::fs::read_to_string(&path) else { return false; };
        let data: VaultData = toml::from_str(&text).unwrap_or_default();
        data.entries.contains_key(profile_id)
    }

    pub fn remove_profile(&self, profile_id: &str) -> Result<()> {
        let mut data = self.load_data()?;
        data.entries.remove(profile_id);
        self.save_data(&mut data)
    }

    // ─── Historique de commandes chiffré ─────────────────────────────────────

    pub fn get_history(&self, profile_id: &str) -> Result<Vec<String>> {
        let data = self.load_data()?;
        match data.entries.get(profile_id).and_then(|e| e.history.as_deref()) {
            None => Ok(Vec::new()),
            Some(enc) => {
                let json = self.decrypt(enc)?;
                Ok(serde_json::from_str(&json).unwrap_or_default())
            }
        }
    }

    pub fn store_history(&self, profile_id: &str, commands: &[String]) -> Result<()> {
        let trimmed: Vec<&String> = commands.iter().take(20).collect();
        let json = serde_json::to_string(&trimmed)?;
        let encrypted = self.encrypt(&json)?;
        let mut data = self.load_data()?;
        data.entries.entry(profile_id.to_string()).or_default().history = Some(encrypted);
        self.save_data(&mut data)
    }

    // ─── Chiffrement interne (ChaCha20-Poly1305) ──────────────────────────────

    /// Format : base64(nonce_12_octets || ciphertext_avec_tag_poly1305).
    fn encrypt(&self, plaintext: &str) -> Result<String> {
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.key));
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|_| anyhow::anyhow!("chiffrement ChaCha20-Poly1305 échoué"))?;
        let mut out = nonce_bytes.to_vec();
        out.extend_from_slice(&ciphertext);
        Ok(base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &out))
    }

    fn decrypt(&self, encoded: &str) -> Result<String> {
        let raw = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
            .context("décodage base64")?;
        if raw.len() < 12 {
            bail!("données chiffrées trop courtes (< 12 octets)");
        }
        let (nonce_bytes, ciphertext) = raw.split_at(12);
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&self.key));
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| anyhow::anyhow!("déchiffrement échoué — clé incorrecte ou données corrompues"))?;
        Ok(String::from_utf8(plaintext).context("décodage UTF-8")?)
    }

    fn decrypt_field(&self, enc: Option<&str>) -> Result<Option<String>> {
        match enc {
            None      => Ok(None),
            Some(enc) => Ok(Some(self.decrypt(enc)?)),
        }
    }

    // ─── Migration v1 (age) → v2 ─────────────────────────────────────────────

    fn migrate_age_field(&self, enc: Option<&str>) -> Result<Option<String>> {
        match enc {
            None      => Ok(None),
            Some(enc) => Ok(Some(self.encrypt(&age_decrypt(enc, &self.passphrase)?)?)),
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
        data.version  = Some(2);
        data.kdf_salt = Some(base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &self.salt,
        ));
        // Génère le jeton de vérification une seule fois.
        if data.key_verify.is_none() {
            data.key_verify = Some(self.encrypt(KEY_VERIFY_TOKEN)?);
        }
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, toml::to_string_pretty(data)?)?;
        Ok(())
    }
}

// ─── Helpers internes ─────────────────────────────────────────────────────────

fn load_salt_from_file(path: &std::path::Path) -> Option<[u8; 16]> {
    let text = std::fs::read_to_string(path).ok()?;
    let data: VaultData = toml::from_str(&text).ok()?;
    let encoded = data.kdf_salt?;
    let bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD, &encoded,
    ).ok()?;
    if bytes.len() == 16 {
        let mut arr = [0u8; 16];
        arr.copy_from_slice(&bytes);
        Some(arr)
    } else {
        None
    }
}

/// Argon2id avec m=32 MiB, t=2, p=1 ≈ 50-100 ms sur hardware moderne.
/// S'exécute une seule fois par session (au déverrouillage).
fn derive_key(passphrase: &str, salt: &[u8; 16]) -> [u8; 32] {
    let params = Params::new(32768, 2, 1, Some(32))
        .expect("paramètres Argon2 invalides");
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; 32];
    argon2
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .expect("dérivation Argon2id échouée");
    key
}

/// SHA-256 hex (utilisé uniquement pour la compatibilité v1 legacy).
fn sha256_hex(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    format!("{:x}", h.finalize())
}

/// Déchiffre un champ au format age v1 (utilisé uniquement pendant la migration).
fn age_decrypt(encoded: &str, passphrase: &str) -> Result<String> {
    use age::secrecy::SecretString;
    let pass = SecretString::from(passphrase.to_string());
    let raw = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
        .context("base64 (migration age)")?;
    let decryptor = age::Decryptor::new_buffered(raw.as_slice())
        .context("déchiffreur age (migration)")?;
    let mut reader = match decryptor {
        age::Decryptor::Passphrase(d) => d
            .decrypt(&pass, Some(20))
            .context("déchiffrement age (migration)")?,
        _ => bail!("type de déchiffreur age inattendu"),
    };
    let mut plaintext = String::new();
    reader.read_to_string(&mut plaintext)?;
    Ok(plaintext)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_vault(key: &str) -> (Vault, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let mut salt = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut salt);
        let derived = derive_key(key, &salt);
        let v = Vault {
            path:       dir.path().join("vault.toml"),
            salt,
            key:        derived,
            passphrase: key.to_string(),
        };
        (v, dir)
    }

    #[test]
    fn round_trip_password() {
        let (vault, _dir) = temp_vault("clé-test-123");
        vault.store_password("p1", "s3cr3t!").unwrap();
        assert_eq!(vault.get_password("p1").unwrap(), Some("s3cr3t!".into()));
    }

    #[test]
    fn round_trip_profile_bulk() {
        let (vault, _dir) = temp_vault("clé-test");
        vault.store_profile("p1", Some("192.168.1.10"), Some("alice"), Some("pwd")).unwrap();
        let (addr, user, pw) = vault.get_profile("p1").unwrap();
        assert_eq!(addr, Some("192.168.1.10".into()));
        assert_eq!(user, Some("alice".into()));
        assert_eq!(pw,   Some("pwd".into()));
    }

    #[test]
    fn wrong_key_detected() {
        let (vault_a, dir) = temp_vault("bonne-cle");
        vault_a.store_password("p1", "secret").unwrap();

        let bad_key = derive_key("mauvaise-cle", &vault_a.salt);
        let vault_b = Vault {
            path:       dir.path().join("vault.toml"),
            salt:       vault_a.salt,
            key:        bad_key,
            passphrase: "mauvaise-cle".into(),
        };
        assert!(matches!(vault_b.master_key_ok().unwrap(), MasterKeyCheck::Wrong));
    }

    #[test]
    fn remove_profile_purge() {
        let (vault, _dir) = temp_vault("cle");
        vault.store_profile("p1", Some("10.0.0.1"), Some("root"), Some("pw")).unwrap();
        vault.remove_profile("p1").unwrap();
        let (a, u, p) = vault.get_profile("p1").unwrap();
        assert!(a.is_none() && u.is_none() && p.is_none());
    }

    #[test]
    fn profil_absent_retourne_none() {
        let (vault, _dir) = temp_vault("cle");
        assert!(vault.get_password("inexistant").unwrap().is_none());
        let (a, u, p) = vault.get_profile("inexistant").unwrap();
        assert!(a.is_none() && u.is_none() && p.is_none());
    }

    #[test]
    fn round_trip_history() {
        let (vault, _dir) = temp_vault("cle");
        let cmds: Vec<String> = vec!["ls -la".into(), "cd /tmp".into()];
        vault.store_history("p1", &cmds).unwrap();
        let got = vault.get_history("p1").unwrap();
        assert_eq!(got, cmds);
    }
}
