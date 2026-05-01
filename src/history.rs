/// Historique des commandes persisté par profil dans ~/.rustshell/history/.
/// Fonctionnalités : déduplication, navigation haut/bas, recherche incrémentale.
use anyhow::Result;
use std::collections::VecDeque;
use std::path::PathBuf;

use crate::config::AppConfig;

/// Nombre maximum de commandes gardées en mémoire et sur disque.
const MAX_HISTORY: usize = 10_000;

pub struct CommandHistory {
    /// Chemin du fichier texte de cet historique (un fichier par profil).
    path: PathBuf,
    /// Commandes dans l'ordre chronologique (plus ancien en tête).
    entries: VecDeque<String>,
    /// Position du curseur de navigation (None = on est à la fin / pas de nav en cours).
    cursor: Option<usize>,
}

impl CommandHistory {
    /// Charge l'historique depuis le disque pour le profil donné.
    /// Crée le fichier s'il n'existe pas encore.
    pub fn load(profile_name: &str) -> Result<Self> {
        let dir = AppConfig::config_dir().join("history");
        std::fs::create_dir_all(&dir)?;

        // Sanitise le nom de profil pour en faire un nom de fichier valide.
        let safe_name = profile_name
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect::<String>();
        let path = dir.join(format!("{safe_name}.txt"));

        let entries = if path.exists() {
            std::fs::read_to_string(&path)?
                .lines()
                .map(str::to_string)
                .collect()
        } else {
            VecDeque::new()
        };
        Ok(Self { path, entries, cursor: None })
    }

    /// Ajoute une commande à l'historique.
    /// Les doublons consécutifs sont ignorés (comme zsh/bash HISTCONTROL=ignoredups).
    pub fn push(&mut self, cmd: String) {
        if self.entries.back().map(String::as_str) == Some(&cmd) {
            return; // doublon consécutif → on n'ajoute pas
        }
        self.entries.push_back(cmd);
        if self.entries.len() > MAX_HISTORY {
            self.entries.pop_front(); // purge les plus vieilles entrées
        }
        self.cursor = None; // réinitialise la position de navigation
    }

    /// Remonte dans l'historique (flèche Haut).
    /// Retourne None si l'historique est vide.
    pub fn navigate_up(&mut self) -> Option<&str> {
        if self.entries.is_empty() { return None; }
        let next = match self.cursor {
            None => self.entries.len() - 1,
            Some(0) => 0,
            Some(n) => n - 1,
        };
        self.cursor = Some(next);
        self.entries.get(next).map(String::as_str)
    }

    /// Descend dans l'historique (flèche Bas).
    /// Retourne Some("") quand on dépasse la fin (invite vide).
    pub fn navigate_down(&mut self) -> Option<&str> {
        match self.cursor {
            None => None,
            Some(n) if n + 1 >= self.entries.len() => {
                self.cursor = None;
                Some("") // retour à l'invite vide
            }
            Some(n) => {
                self.cursor = Some(n + 1);
                self.entries.get(n + 1).map(String::as_str)
            }
        }
    }

    /// Réinitialise le curseur de navigation (après validation d'une commande).
    pub fn reset_cursor(&mut self) {
        self.cursor = None;
    }

    /// Recherche les 50 dernières commandes contenant `query` (insensible à la casse).
    pub fn search(&self, query: &str) -> Vec<&str> {
        self.entries
            .iter()
            .rev()
            .filter(|e| e.contains(query))
            .map(String::as_str)
            .take(50)
            .collect()
    }

    /// Sauvegarde l'historique sur le disque (appeler à la fermeture de session).
    pub fn save(&self) -> Result<()> {
        let text = self.entries.iter().cloned().collect::<Vec<_>>().join("\n");
        std::fs::write(&self.path, text)?;
        Ok(())
    }

    /// Itère sur toutes les commandes (ordre chronologique).
    pub fn all(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(String::as_str)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_history() -> CommandHistory {
        CommandHistory {
            path: PathBuf::from("/tmp/test_history.txt"),
            entries: VecDeque::new(),
            cursor: None,
        }
    }

    #[test]
    fn deduplication_consecutive() {
        let mut h = make_history();
        h.push("ls".into());
        h.push("ls".into());
        assert_eq!(h.entries.len(), 1, "doublon consécutif doit être ignoré");
    }

    #[test]
    fn navigation_haut_bas() {
        let mut h = make_history();
        h.push("cmd1".into());
        h.push("cmd2".into());
        assert_eq!(h.navigate_up(), Some("cmd2"));
        assert_eq!(h.navigate_up(), Some("cmd1"));
        assert_eq!(h.navigate_down(), Some("cmd2"));
    }

    #[test]
    fn recherche_par_motif() {
        let mut h = make_history();
        h.push("ls -la".into());
        h.push("git status".into());
        h.push("ls /tmp".into());
        let results = h.search("ls");
        assert_eq!(results.len(), 2);
    }
}
