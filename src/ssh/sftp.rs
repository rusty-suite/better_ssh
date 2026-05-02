/// Client SFTP construit au-dessus d'une session SSH russh.
/// Offre les opérations courantes : liste de répertoire, téléchargement,
/// upload, création de dossier, suppression et renommage.
use anyhow::Result;
use russh_sftp::client::SftpSession;
use std::path::PathBuf;

// ─── Entrée de répertoire ─────────────────────────────────────────────────────

/// Représentation d'un fichier ou dossier distant, normalisée pour l'UI.
#[derive(Debug, Clone)]
pub struct RemoteEntry {
    /// Nom du fichier/dossier (sans chemin parent).
    pub name: String,
    /// Chemin absolu complet sur le serveur distant.
    pub path: String,
    /// true si c'est un répertoire.
    pub is_dir: bool,
    /// Taille en octets (0 si inconnue ou pour les dossiers).
    pub size: u64,
    /// Timestamp de dernière modification (secondes Unix), si disponible.
    pub modified: Option<u64>,
    /// Bits de permissions Unix (ex: 0o755), si disponibles.
    pub permissions: Option<u32>,
}

// ─── Client SFTP ─────────────────────────────────────────────────────────────

/// Wrapping de `russh_sftp::client::SftpSession` avec des méthodes de haut niveau.
pub struct SftpClient {
    session: SftpSession,
}

impl SftpClient {
    pub fn new(session: SftpSession) -> Self {
        Self { session }
    }

    /// Liste le contenu d'un répertoire distant.
    /// Les entrées sont triées : dossiers en premier, puis fichiers, alphabétiquement.
    pub async fn list_dir(&self, path: &str) -> Result<Vec<RemoteEntry>> {
        let dir = self.session.read_dir(path).await?;
        let mut entries = Vec::new();
        for item in dir {
            let meta = item.metadata();
            entries.push(RemoteEntry {
                name: item.file_name(),
                // Construit le chemin absolu en évitant les doubles slashes.
                path: format!("{}/{}", path.trim_end_matches('/'), item.file_name()),
                is_dir: meta.is_dir(),
                size: meta.size.unwrap_or(0),
                modified: meta.mtime.map(|t| t as u64),
                permissions: meta.permissions,
            });
        }
        // Dossiers d'abord (pour ressembler à un explorateur standard).
        entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        });
        Ok(entries)
    }

    /// Télécharge un fichier distant vers le disque local.
    /// Retourne la taille en octets téléchargés.
    pub async fn download_file(&self, remote_path: &str, local_path: &PathBuf) -> Result<u64> {
        let data = self.session.read(remote_path).await?;
        let size = data.len() as u64;
        tokio::fs::write(local_path, data).await?;
        Ok(size)
    }

    /// Upload un fichier local vers le serveur distant.
    /// Retourne la taille en octets envoyés.
    pub async fn upload_file(&self, local_path: &PathBuf, remote_path: &str) -> Result<u64> {
        let data = tokio::fs::read(local_path).await?;
        let size = data.len() as u64;
        self.session.write(remote_path, &data).await?;
        Ok(size)
    }

    /// Crée un répertoire distant (équivalent de `mkdir`).
    pub async fn mkdir(&self, path: &str) -> Result<()> {
        self.session.create_dir(path).await?;
        Ok(())
    }

    /// Supprime un fichier distant (`rm`).
    pub async fn remove_file(&self, path: &str) -> Result<()> {
        self.session.remove_file(path).await?;
        Ok(())
    }

    /// Renomme ou déplace un fichier/dossier distant (`mv`).
    pub async fn rename(&self, from: &str, to: &str) -> Result<()> {
        self.session.rename(from, to).await?;
        Ok(())
    }
}
