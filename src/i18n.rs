/// Système de localisation de BetterSSH.
/// Charge les fichiers TOML de langue depuis le répertoire de travail
/// (ou utilise les fichiers embarqués à la compilation comme repli).
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::path::PathBuf;

// ─── Fichiers embarqués ───────────────────────────────────────────────────────

const EN_DEFAULT: &str = include_str!("../lang/EN_en.default.toml");
const FR_FR:      &str = include_str!("../lang/FR_fr.toml");
const CH_FR:      &str = include_str!("../lang/CH_fr.toml");
const DE_DE:      &str = include_str!("../lang/DE_de.toml");
const CH_DE:      &str = include_str!("../lang/CH_de.toml");
const IT_IT:      &str = include_str!("../lang/IT_it.toml");
const CH_IT:      &str = include_str!("../lang/CH_it.toml");

pub const KNOWN_LANGS: &[&str] = &[
    "EN_en", "FR_fr", "CH_fr", "DE_de", "CH_de", "IT_it", "CH_it",
];

const GITHUB_RAW: &str =
    "https://raw.githubusercontent.com/rusty-suite/better_ssh/main/lang";

// ─── Struct Lang ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct Lang {
    pub lang_name: String,
    pub lang_code: String,

    // App / SSH messages
    pub app_ssh_connected:    String,
    pub app_ssh_disconnected: String,
    pub app_ssh_error:        String,
    pub app_ssh_signal:       String,

    // Top bar
    pub topbar_theme_light:  String,
    pub topbar_theme_dark:   String,
    pub topbar_theme_hint:   String,
    pub topbar_prefs:        String,
    pub topbar_scanner:      String,
    pub topbar_scanner_hint: String,
    pub topbar_lang_hint:    String,

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
    pub fe_item_count:   String,
    pub fe_hover_path:   String,
    pub fe_hover_perms:  String,
    pub fe_hover_access: String,
    pub fe_hover_size:   String,
    pub fe_hover_modified: String,

    // Language window
    pub lang_win_title:       String,
    pub lang_active_label:    String,
    pub lang_open_btn:        String,
    pub lang_open_btn_hint:   String,
    pub lang_badge_default:   String,
    pub lang_no_internet_msg: String,
    pub lang_local_section:   String,
    pub lang_remote_section:  String,
    pub lang_refresh_btn:     String,
    pub lang_refresh_btn_hint: String,
    pub lang_download_btn:    String,
    pub lang_local_badge:     String,
    pub lang_builtin_badge:   String,
    pub lang_downloaded_badge: String,
    pub lang_remote_badge:    String,
    pub lang_installed_badge: String,
    pub lang_remote_loading:  String,
    pub lang_remote_empty:    String,
    pub lang_remote_error:    String,
    pub lang_local_empty:     String,
    pub lang_status_offline:  String,
    pub lang_status_ready:    String,
    pub lang_status_installing: String,
    pub lang_status_installed: String,
    pub lang_local_path_label: String,
    pub lang_close_btn:       String,

    // Welcome screen
    pub welcome_title:     String,
    pub welcome_new:       String,
    pub welcome_new_hint:  String,
    pub welcome_scan:      String,
    pub welcome_scan_hint: String,
}

impl Default for Lang {
    fn default() -> Self {
        toml::from_str(EN_DEFAULT).expect("embedded EN default is valid TOML")
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
    /// Insère une langue dans un patron comme `"Langue [{lang}]"`.
    pub fn fmt_lang(pattern: &str, lang: &str) -> String {
        pattern.replace("{lang}", lang)
    }
}

// ─── Métadonnées d'un fichier de langue ───────────────────────────────────────

#[derive(Clone)]
pub struct LangFile {
    /// Identifiant fichier (ex : `"CH_fr"`, `"EN_en"`).
    pub stem: String,
    /// Nom affiché (ex : `"Français (CH)"`).
    pub name: String,
    /// Code BCP-47 (ex : `"fr_CH"`).
    pub lang_code: String,
    /// true si ce fichier provient du répertoire de travail (pas de l'embarqué).
    pub from_disk: bool,
    /// Nom réel du fichier (ex : `"FR_fr.toml"`).
    pub file_name: String,
    /// Chemin ou emplacement utile pour l'affichage.
    pub location: String,
}

// ─── Détection du répertoire de travail ─────────────────────────────────────

/// Détermine le répertoire de données de l'application selon l'environnement.
/// Ordre de priorité :
///   1. `%APPDATA%\rusty-suite\betterssh\`  (Windows installé)
///   2. `~/.betterssh/`                     (compat. versiones antérieures)
///   3. `~/betterssh/`                       (standalone)
pub fn detect_work_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let p = PathBuf::from(appdata)
                .join("rusty-suite")
                .join("betterssh");
            if p.exists() || std::fs::create_dir_all(&p).is_ok() {
                return p;
            }
        }
    }
    if let Some(home) = dirs::home_dir() {
        // Compat. ancienne version
        let legacy = home.join(".betterssh");
        if legacy.exists() {
            return legacy;
        }
        let standalone = home.join("betterssh");
        if standalone.exists() || std::fs::create_dir_all(&standalone).is_ok() {
            return standalone;
        }
    }
    PathBuf::from(".")
}

// ─── Correspondance locale système ───────────────────────────────────────────

/// Choisit le stem de langue le mieux adapté à la locale système détectée.
pub fn detect_system_lang_stem() -> &'static str {
    let locale = sys_locale::get_locale().unwrap_or_default();
    map_locale_to_stem(&locale)
}

fn map_locale_to_stem(locale: &str) -> &'static str {
    let l = locale.to_lowercase();
    if l.starts_with("fr_ch") || l.starts_with("fr-ch") { return "CH_fr"; }
    if l.starts_with("de_ch") || l.starts_with("de-ch") { return "CH_de"; }
    if l.starts_with("it_ch") || l.starts_with("it-ch") { return "CH_it"; }
    if l.starts_with("fr")    { return "FR_fr"; }
    if l.starts_with("de")    { return "DE_de"; }
    if l.starts_with("it")    { return "IT_it"; }
    "EN_en"
}

// ─── Chargement de la langue ─────────────────────────────────────────────────

/// Charge la langue choisie et retourne `(Lang, stem_actif)`.
/// Ordre :
///   1. Lit `{work_dir}/lang_chosen.txt` pour le choix sauvegardé ;
///   2. Lit `{work_dir}/lang/{stem}.toml` pour un override sur disque ;
///   3. Utilise l'embarqué correspondant ;
///   4. Replie sur `EN_en` si rien ne correspond.
pub fn load_lang(work_dir: &PathBuf) -> (Lang, String) {
    let stem = read_chosen(work_dir)
        .unwrap_or_else(|| detect_system_lang_stem().to_string());

    let overlay = read_disk_lang(work_dir, &stem)
        .or_else(|| embedded_lang(&stem).map(str::to_string));

    let Some(overlay) = overlay else {
        return (Lang::default(), "EN_en".to_string());
    };

    (merge_with_default(&overlay), stem)
}

/// Lit le choix sauvegardé dans `lang_chosen.txt`.
fn read_chosen(work_dir: &PathBuf) -> Option<String> {
    let s = std::fs::read_to_string(work_dir.join("lang_chosen.txt")).ok()?;
    let stem = s.trim().to_string();
    if stem.is_empty() { None } else { Some(stem) }
}

/// Lit un fichier `.toml` depuis `{work_dir}/lang/`.
fn read_disk_lang(work_dir: &PathBuf, stem: &str) -> Option<String> {
    std::fs::read_to_string(work_dir.join("lang").join(format!("{stem}.toml"))).ok()
}

/// Retourne le texte TOML embarqué pour un stem donné.
pub fn embedded_lang(stem: &str) -> Option<&'static str> {
    match stem {
        "EN_en" => Some(EN_DEFAULT),
        "FR_fr" => Some(FR_FR),
        "CH_fr" => Some(CH_FR),
        "DE_de" => Some(DE_DE),
        "CH_de" => Some(CH_DE),
        "IT_it" => Some(IT_IT),
        "CH_it" => Some(CH_IT),
        _ => None,
    }
}

/// Fusionne un texte TOML de langue avec les valeurs par défaut EN.
/// Les clés présentes dans `overlay` écrasent celles du défaut.
fn merge_with_default(overlay: &str) -> Lang {
    let mut base: toml::Value = toml::from_str(EN_DEFAULT)
        .unwrap_or(toml::Value::Table(toml::map::Map::new()));
    let over: toml::Value = toml::from_str(overlay)
        .unwrap_or(toml::Value::Table(toml::map::Map::new()));

    if let (toml::Value::Table(b), toml::Value::Table(o)) = (&mut base, over) {
        for (k, v) in o {
            b.insert(k, v);
        }
    }

    let merged = toml::to_string(&base).unwrap_or_default();
    toml::from_str(&merged).unwrap_or_default()
}

// ─── Persistance du choix ─────────────────────────────────────────────────────

pub fn save_lang_choice(work_dir: &PathBuf, stem: &str) {
    let _ = std::fs::write(work_dir.join("lang_chosen.txt"), stem);
}

// ─── Inventaire des langues disponibles ─────────────────────────────────────

/// Construit la liste des langues disponibles (embarquées + sur disque).
pub fn list_lang_files(work_dir: &PathBuf) -> Vec<LangFile> {
    let mut files: Vec<LangFile> = Vec::new();

    // Langues embarquées
    for &stem in KNOWN_LANGS {
        if let Some(text) = embedded_lang(stem) {
            if let Some(file) = parse_lang_toml(
                stem.to_string(),
                text,
                false,
                embedded_file_name(stem).to_string(),
                "embedded".to_string(),
            ) {
                files.push(file);
            }
        }
    }

    // Les fichiers sur disque écrasent l'embarqué s'ils ont le même stem.
    for disk_file in list_local_lang_files(work_dir) {
        if let Some(pos) = files.iter().position(|f| f.stem == disk_file.stem) {
            files[pos] = disk_file;
        } else {
            files.push(disk_file);
        }
    }

    files.sort_by(|a, b| a.name.cmp(&b.name));
    files
}

/// Liste les fichiers `.toml` réellement présents dans `{work_dir}/lang/`.
pub fn list_local_lang_files(work_dir: &PathBuf) -> Vec<LangFile> {
    let mut files: Vec<LangFile> = Vec::new();

    let lang_dir = work_dir.join("lang");
    if let Ok(entries) = std::fs::read_dir(&lang_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") { continue; }
            let stem = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) if !s.ends_with(".default") => s.to_string(),
                _ => continue,
            };
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
                    if let Some(file) = parse_lang_toml(
                        stem,
                        &text,
                        true,
                        file_name.to_string(),
                        path.display().to_string(),
                    ) {
                        files.push(file);
                    }
                }
            }
        }
    }

    files.sort_by(|a, b| a.name.cmp(&b.name));
    files
}

/// Liste les langues présentes sur GitHub.
pub fn list_remote_lang_files() -> anyhow::Result<Vec<LangFile>> {
    let url = "https://api.github.com/repos/rusty-suite/better_ssh/contents/lang";
    let response = ureq::get(url)
        .set("User-Agent", "betterssh")
        .set("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(10))
        .call()?;
    let body = response.into_string()?;
    let entries: Vec<JsonValue> = serde_json::from_str(&body)?;
    let mut files = Vec::new();

    for entry in entries {
        let Some(name) = entry.get("name").and_then(|v| v.as_str()) else { continue; };
        if !name.ends_with(".toml") { continue; }
        let stem = name.trim_end_matches(".toml").trim_end_matches(".default").to_string();
        let download_url = entry
            .get("download_url")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        if download_url.is_empty() {
            continue;
        }

        let text = ureq::get(&download_url)
            .set("User-Agent", "betterssh")
            .timeout(std::time::Duration::from_secs(10))
            .call()?
            .into_string()?;

        if let Some(file) = parse_lang_toml(stem, &text, false, name.to_string(), download_url) {
            files.push(file);
        }
    }

    files.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(files)
}

// ─── Téléchargement d'une langue depuis GitHub ───────────────────────────────

/// Télécharge `{stem}.toml` depuis GitHub et l'écrit dans `{work_dir}/lang/`.
/// Retourne `Ok(texte)` si réussi.
pub fn download_lang(work_dir: &PathBuf, stem: &str) -> anyhow::Result<String> {
    let url = format!("{GITHUB_RAW}/{stem}.toml");
    let text = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .call()?
        .into_string()?;
    let lang_dir = work_dir.join("lang");
    std::fs::create_dir_all(&lang_dir)?;
    std::fs::write(lang_dir.join(format!("{stem}.toml")), &text)?;
    Ok(text)
}

pub fn install_embedded_lang(work_dir: &PathBuf, stem: &str) -> anyhow::Result<String> {
    let text = embedded_lang(stem)
        .ok_or_else(|| anyhow::anyhow!("embedded language not found: {stem}"))?
        .to_string();
    let lang_dir = work_dir.join("lang");
    std::fs::create_dir_all(&lang_dir)?;
    std::fs::write(lang_dir.join(format!("{stem}.toml")), &text)?;
    Ok(text)
}

fn parse_lang_toml(
    stem: String,
    text: &str,
    from_disk: bool,
    file_name: String,
    location: String,
) -> Option<LangFile> {
    let val = toml::from_str::<toml::Value>(text).ok()?;
    let name = val.get("lang_name")
        .and_then(|v| v.as_str())
        .unwrap_or(&stem)
        .to_string();
    let lang_code = val.get("lang_code")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Some(LangFile { stem, name, lang_code, from_disk, file_name, location })
}

fn embedded_file_name(stem: &str) -> &'static str {
    match stem {
        "EN_en" => "EN_en.default.toml",
        "FR_fr" => "FR_fr.toml",
        "CH_fr" => "CH_fr.toml",
        "DE_de" => "DE_de.toml",
        "CH_de" => "CH_de.toml",
        "IT_it" => "IT_it.toml",
        "CH_it" => "CH_it.toml",
        _ => "unknown.toml",
    }
}
