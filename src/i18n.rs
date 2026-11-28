//! Textes de BetterSSH.
//!
//! Le chargement (packs intégrés, copie locale, mise à jour depuis GitHub, repli sur la
//! version intégrée puis sur l'anglais) est fait par `rusty_core::lang` (charte Rusty
//! Suite, section 05). Les écrans de BetterSSH lisent les textes par champs typés
//! (`lang.dlg_save_btn`) : la table plate du kit est convertie une fois en `Lang` à
//! chaque changement de langue, ce qui garde la vérification des clés à la compilation.
use serde::Deserialize;
use std::path::Path;

/// Packs de langue intégrés au binaire (nom de fichier historique pour l'anglais).
pub const PACKS: rusty_core::lang::Packs = &[
    ("EN_en", include_str!("../lang/EN_en.default.toml")),
    ("FR_fr", include_str!("../lang/FR_fr.toml")),
    ("CH_fr", include_str!("../lang/CH_fr.toml")),
    ("DE_de", include_str!("../lang/DE_de.toml")),
    ("CH_de", include_str!("../lang/CH_de.toml")),
    ("IT_it", include_str!("../lang/IT_it.toml")),
    ("CH_it", include_str!("../lang/CH_it.toml")),
];

// ─── Struct Lang ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct Lang {
    /// Description courte (en-tête et fenêtre À propos).
    pub app_description: String,

    // App / SSH messages
    pub app_ssh_connected:    String,
    pub app_ssh_disconnected: String,
    pub app_ssh_error:        String,
    pub app_ssh_signal:       String,

    // Top bar
    pub topbar_prefs:        String,
    pub topbar_scanner_hint: String,

    // Tab bar
    pub tab_close_hint: String,
    pub tab_new_hint:   String,

    // Sidebar
    pub sidebar_title:              String,
    pub sidebar_search_hint:        String,
    pub sidebar_new_connection:     String,
    pub sidebar_no_name:            String,
    pub sidebar_locked_data:        String,
    pub sidebar_vault_locked_hover: String,
    pub sidebar_double_click_hint:  String,
    pub sidebar_delete_hint:        String,
    pub sidebar_edit_hint:          String,

    // Dialog — vault-locked screen
    pub dlg_vault_screen_title:    String,
    pub dlg_vault_screen_subtitle: String,
    pub dlg_vault_key_label:       String,
    pub dlg_unlock_btn:            String,

    // Dialog — form fields
    pub dlg_new_title:     String,
    pub dlg_edit_title:    String,
    pub dlg_field_name:    String,
    pub dlg_field_host:    String,
    pub dlg_field_port:    String,
    pub dlg_field_user:    String,
    pub dlg_field_auth:    String,
    pub dlg_field_password: String,
    pub dlg_field_vault:   String,
    pub dlg_field_keyfile: String,
    pub dlg_field_tags:    String,
    pub dlg_field_jump:    String,
    pub dlg_field_timeout: String,
    pub dlg_hint_name:     String,
    pub dlg_hint_host:     String,
    pub dlg_hint_user:     String,
    pub dlg_hint_tags:     String,
    pub dlg_hint_jump:     String,
    pub dlg_auth_password: String,
    pub dlg_auth_agent:    String,
    pub dlg_auth_key:      String,
    pub dlg_vault_unlocked:  String,
    pub dlg_vault_encrypt:   String,
    pub dlg_vault_locked:    String,
    pub dlg_vault_required:  String,
    pub dlg_vault_key_hint:  String,
    pub dlg_pw_loaded:       String,
    pub dlg_pw_hint_loaded:  String,
    pub dlg_pw_hint:         String,
    pub dlg_browse_btn:      String,
    pub dlg_browse_title:    String,
    pub dlg_save_btn:        String,
    pub dlg_connect_btn:     String,
    pub dlg_cancel_btn:      String,

    // File explorer
    pub fe_favorites:      String,
    pub fe_legend_title:   String,
    pub fe_legend_full:    String,
    pub fe_legend_read:    String,
    pub fe_legend_denied:  String,
    pub fe_legend_unknown: String,
    pub fe_nav_prev:       String,
    pub fe_nav_next:       String,
    pub fe_nav_up:         String,
    pub fe_path_hint:      String,
    pub fe_edit_path_hint: String,
    pub fe_refresh_hint:   String,
    pub fe_new_folder_hint: String,
    pub fe_new_file_hint:  String,
    pub fe_search_hint:    String,
    pub fe_view_list:      String,
    pub fe_view_grid:      String,
    pub fe_col_name:       String,
    pub fe_col_perms:      String,
    pub fe_col_size:       String,
    pub fe_col_modified:   String,
    pub fe_loading:        String,
    pub fe_empty:          String,
    pub fe_no_results:     String,
    pub fe_items:          String,
    pub fe_selected:       String,
    pub fe_ctx_open:       String,
    pub fe_ctx_download:   String,
    pub fe_ctx_rename:     String,
    pub fe_ctx_copy:       String,
    pub fe_ctx_cut:        String,
    pub fe_ctx_delete:     String,
    pub fe_ctx_paste_copy: String,
    pub fe_ctx_paste_move: String,
    pub fe_ctx_paste:      String,
    pub fe_ctx_copy_path:  String,
    pub fe_ctx_create:     String,
    pub fe_ctx_new_folder: String,
    pub fe_ctx_new_file:   String,
    pub fe_toast_copied:      String,
    pub fe_toast_cut:         String,
    pub fe_toast_pasted:      String,
    pub fe_toast_deleted:     String,
    pub fe_toast_renamed:     String,
    pub fe_toast_path_copied: String,
    pub fe_access_root:       String,
    pub fe_access_denied_msg: String,
    pub fe_access_read_perm:  String,
    pub fe_access_write_perm: String,
    pub fe_access_exec_perm:  String,
    pub fe_access_trav_perm:  String,
    pub fe_path_label:   String,
    pub fe_hover_path:   String,
    pub fe_hover_perms:  String,
    pub fe_hover_access: String,
    pub fe_hover_size:   String,
    pub fe_hover_modified: String,


    // Welcome screen
    pub welcome_title:     String,
    pub welcome_new:       String,
    pub welcome_new_hint:  String,
    pub welcome_scan:      String,
    pub welcome_scan_hint: String,

    // Status bar
    pub status_connected: String,
    pub status_disconnected: String,
    pub status_port: String,
    pub status_no_session: String,

    // Common buttons and fields
    pub btn_connect: String,
    pub btn_delete: String,
    pub field_ssh_port: String,
    pub field_concurrency: String,
    pub field_ssh_banner: String,
    pub suffix_connections: String,
    pub size_units: String,
    pub save_as_title: String,
    pub auth_key_label: String,
    pub vault_wrong_password: String,
    pub sidebar_missing_host: String,

    // Welcome screen shortcuts
    pub shortcut_close_tab: String,
    pub shortcut_next_tab: String,
    pub shortcut_sftp: String,
    pub shortcut_monitor: String,
    pub shortcut_snippets: String,
    pub shortcut_zoom: String,
    pub shortcut_history: String,

    // Preferences
    pub prefs_font: String,
    pub prefs_preset: String,
    pub prefs_custom_size: String,
    pub font_preset_tiny: String,
    pub font_preset_small: String,
    pub font_preset_normal: String,
    pub font_preset_comfortable: String,
    pub font_preset_large: String,
    pub font_preset_huge: String,
    pub prefs_network: String,
    pub prefs_network_hint: String,
    pub prefs_default_cidr: String,
    pub prefs_timeout: String,
    pub prefs_save: String,

    // About
    pub about_description: String,

    // Telnet and scan connection dialogs
    pub telnet_title: String,
    pub scan_connect_title_new: String,
    pub scan_connect_title_existing: String,
    pub scan_connect_existing: String,
    pub scan_vault_encrypt: String,
    pub scan_vault_key_hint: String,
    pub scan_agent_hint: String,

    // Network scanner
    pub scan_window_title: String,
    pub scan_invalid_range: String,
    pub scan_remaining: String,
    pub scan_no_results: String,
    pub scan_no_results_hint: String,
    pub scan_params: String,
    pub scan_stop: String,
    pub scan_running: String,
    pub scan_start: String,
    pub scan_reset: String,
    pub scan_reset_hint: String,
    pub scan_target: String,
    pub scan_target_hint: String,
    pub scan_detected: String,
    pub scan_port_default: String,
    pub scan_delay: String,
    pub scan_delay_hint: String,
    pub scan_concurrency_hint: String,
    pub scan_hosts_found: String,
    pub scan_export_csv: String,
    pub scan_filter_hint: String,
    pub scan_col_hostname: String,
    pub scan_col_latency: String,
    pub scan_col_banner: String,
    pub scan_col_version: String,
    pub scan_col_action: String,
    pub scan_connect_hint: String,

    // Snippets
    pub snip_window_title: String,
    pub snip_count: String,
    pub snip_new: String,
    pub snip_filter_hint: String,
    pub snip_pin: String,
    pub snip_unpin: String,
    pub snip_send: String,
    pub snip_new_title: String,
    pub snip_edit_title: String,
    pub snip_command: String,
    pub snip_command_hint: String,
    pub snip_tags_hint: String,
    pub snip_pin_label: String,
    pub snip_pin_check: String,
    pub snip_default_ls: String,
    pub snip_default_df: String,
    pub snip_default_free: String,
    pub snip_default_ps: String,
    pub snip_default_syslog: String,
    pub snip_default_ports: String,
    pub snip_default_load: String,
    pub snip_default_net: String,

    // System monitor
    pub mon_uptime: String,
    pub mon_load: String,
    pub mon_refresh: String,
    pub mon_ram: String,
    pub mon_disks: String,

    // Terminal
    pub term_history: String,
    pub term_copy: String,
    pub term_copy_hint: String,
    pub term_paste: String,
    pub term_clipboard_empty: String,
    pub term_upload_title: String,
    pub term_upload_file: String,
    pub term_upload_size: String,
    pub term_upload_dest: String,
    pub term_upload_btn: String,

    // Session end reasons (inserted into app_ssh_disconnected)
    pub end_by_user: String,
    pub end_exit: String,
    pub end_interrupted: String,
    pub end_closed_by_server: String,

    // Connection errors (inserted into app_ssh_error)
    pub err_timeout: String,
    pub err_tcp: String,
    pub err_password_auth: String,
    pub err_key_load: String,
    pub err_key_auth: String,
    pub err_agent_unsupported: String,
    pub err_auth_rejected: String,
    pub err_open_channel: String,
    pub err_pty: String,
    pub err_shell: String,

    // SFTP operations (toasts)
    pub sftp_op_list: String,
    pub sftp_op_rename: String,
    pub sftp_op_delete: String,
    pub sftp_op_rmdir: String,
    pub sftp_op_mkdir: String,
    pub sftp_op_create: String,
    pub sftp_op_move: String,
    pub sftp_op_download: String,
    pub sftp_op_upload: String,
    pub sftp_access_denied: String,
    pub sftp_read_error: String,

}

impl Default for Lang {
    fn default() -> Self {
        from_kit(&rusty_core::Lang::load(PACKS, Path::new(""), "EN_en"))
    }
}

impl Lang {
    /// Insère la valeur `n` dans un patron comme `"{n} éléments"`.
    pub fn fmt_n(pattern: &str, n: usize) -> String {
        pattern.replace("{n}", &n.to_string())
    }
    /// Insère un nom dans un patron comme `"Renommé en «{name}»"`.
    pub fn fmt_name(pattern: &str, name: &str) -> String {
        pattern.replace("{name}", name)
    }
    /// Remplace plusieurs marqueurs `{nom}` d'un patron (ex. `[("host", "srv")]`).
    pub fn fmt(pattern: &str, args: &[(&str, &str)]) -> String {
        args.iter().fold(pattern.to_string(), |acc, (name, value)| {
            acc.replace(&format!("{{{name}}}"), value)
        })
    }

    /// Taille lisible avec les unités de la langue active (`size_units` = "B,KB,MB,GB").
    /// Point unique de formatage : terminal (transfert) et explorateur SFTP.
    pub fn format_size(&self, bytes: u64) -> String {
        let units: Vec<&str> = self.size_units.split(',').map(str::trim).collect();
        let unit = |i: usize| units.get(i).copied().unwrap_or("?");
        const K: f64 = 1024.0;
        let b = bytes as f64;
        if b < K {
            format!("{bytes} {}", unit(0))
        } else if b < K * K {
            format!("{:.1} {}", b / K, unit(1))
        } else if b < K * K * K {
            format!("{:.1} {}", b / (K * K), unit(2))
        } else {
            format!("{:.2} {}", b / (K * K * K), unit(3))
        }
    }

    /// Libellé d'un préréglage de police (identifiants de `terminal::FONT_PRESETS`).
    pub fn font_preset_label(&self, id: &str) -> &str {
        match id {
            "tiny" => &self.font_preset_tiny,
            "small" => &self.font_preset_small,
            "comfortable" => &self.font_preset_comfortable,
            "large" => &self.font_preset_large,
            "huge" => &self.font_preset_huge,
            _ => &self.font_preset_normal,
        }
    }

    /// Libellé d'une méthode d'authentification (listes déroulantes des dialogues).
    pub fn auth_method(&self, method: &crate::config::AuthMethod) -> String {
        use crate::config::AuthMethod;
        match method {
            AuthMethod::Password => self.dlg_auth_password.clone(),
            AuthMethod::Agent => self.dlg_auth_agent.clone(),
            AuthMethod::PublicKey { identity_file } => {
                Self::fmt(&self.auth_key_label, &[("path", identity_file)])
            }
        }
    }
}

// ─── Chargement ───────────────────────────────────────────────────────────────

/// Charge la langue `chosen` (stem, vide = langue du système) : textes typés de
/// l'application et langue complète du kit (textes communs `[kit]`).
pub fn load_lang(work_dir: &Path, chosen: &str) -> (Lang, rusty_core::Lang) {
    let kit = rusty_core::Lang::load(PACKS, work_dir, chosen);
    (from_kit(&kit), kit)
}

/// Convertit la table plate du kit en `Lang` : les clés de premier niveau (sans point)
/// sont celles de BetterSSH ; l'anglais étant toujours chargé en première couche,
/// aucun champ ne peut manquer.
fn from_kit(kit: &rusty_core::Lang) -> Lang {
    let table: toml::Table = kit
        .keys()
        .filter(|k| !k.contains('.'))
        .map(|k| (k.clone(), toml::Value::String(kit.t(k))))
        .collect();
    table.try_into().unwrap_or_else(|e| {
        // Impossible avec les packs intégrés (test `every_language_builds`).
        log::error!("Textes incomplets : {e}");
        toml::from_str(PACKS[0].1).expect("pack anglais intégré valide")
    })
}

/// Ancien choix de langue (`lang_chosen.txt`) repris comme stem.
pub fn legacy_choice(work_dir: &Path) -> Option<String> {
    let name = std::fs::read_to_string(work_dir.join("lang_chosen.txt")).ok()?;
    let name = name.trim();
    let stem = name.strip_suffix(".toml").unwrap_or(name);
    let stem = stem.strip_suffix(".default").unwrap_or(stem);
    (!stem.is_empty()).then(|| stem.to_string())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn load(stem: &str) -> Lang {
        load_lang(&std::env::temp_dir().join("better_ssh_lang_test"), stem).0
    }

    /// Toutes les langues intégrées ont exactement les clés de l'anglais.
    #[test]
    fn all_languages_have_same_keys() {
        assert_eq!(rusty_core::lang::check_packs(PACKS), Vec::<String>::new());
    }

    /// Chaque langue intégrée donne un `Lang` complet, traduit.
    #[test]
    fn every_language_builds() {
        for (stem, text) in PACKS {
            toml::from_str::<Lang>(text).unwrap_or_else(|e| panic!("{stem}: {e}"));
        }
        assert_eq!(load("FR_fr").btn_delete, "Supprimer");
    }

    /// Un pack local incomplet garde les traductions intégrées de sa langue.
    #[test]
    fn local_layer_keeps_bundled_translations() {
        let dir = std::env::temp_dir().join("better_ssh_lang_local");
        std::fs::create_dir_all(dir.join("lang")).unwrap();
        std::fs::write(rusty_core::lang::local_file(&dir, "FR_fr"), "dlg_save_btn = \"Perso\"\n").unwrap();
        let (lang, _) = load_lang(&dir, "FR_fr");
        assert_eq!(lang.dlg_save_btn, "Perso");
        assert_eq!(lang.btn_delete, "Supprimer");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn format_size_uses_language_units() {
        let fr = load("FR_fr");
        assert_eq!(fr.format_size(512), "512 o");
        assert_eq!(fr.format_size(2048), "2.0 Ko");
        assert_eq!(Lang::default().format_size(3 * 1024 * 1024), "3.0 MB");
    }

    #[test]
    fn fmt_replaces_named_markers() {
        assert_eq!(Lang::fmt("{a}-{b}-{a}", &[("a", "1"), ("b", "2")]), "1-2-1");
    }

    #[test]
    fn legacy_choice_strips_extensions() {
        let dir = std::env::temp_dir().join("better_ssh_lang_legacy");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("lang_chosen.txt"), "CH_fr\n").unwrap();
        assert_eq!(legacy_choice(&dir).as_deref(), Some("CH_fr"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
