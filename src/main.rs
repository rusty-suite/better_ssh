#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
// Une partie de l'API SSH/SFTP (redimensionnement, déconnexion explicite, historique
// persistant, alertes d'empreinte) est prête mais pas encore reliée à l'interface.
#![allow(dead_code)]

// =============================================================================
// BetterSSH : point d'entrée
//
// Avant l'interface : dossier de données (portable, ou ancien `~/.betterssh` s'il
// contient déjà des profils), journal, réglages (l'ancien choix de langue est repris),
// contrôle de la dernière mise à jour (retour arrière automatique) et instance unique
// (un second lancement ramène la fenêtre ouverte).
// =============================================================================

mod app;
mod assets;
mod config;
mod history;
mod i18n;
mod network;
mod ssh;
mod ui;

use anyhow::Result;
use eframe::NativeOptions;
use egui::ViewportBuilder;
use rusty_core::{instance, journal, settings, update};

fn main() -> Result<()> {
    let work_dir = config::AppConfig::config_dir();
    journal::init(app::APP_ID, app::VERSION, &work_dir);
    rusty_core::lang::install_local(i18n::PACKS, &work_dir, app::VERSION);

    let mut settings: app::Settings = settings::load(&work_dir);
    if settings.common.language.is_empty() {
        // Choix fait dans une version précédente (dossier actuel ou anciens dossiers).
        let previous = std::iter::once(work_dir.clone())
            .chain(config::AppConfig::legacy_lang_dirs())
            .find_map(|d| i18n::legacy_choice(&d));
        if let Some(stem) = previous {
            settings.common.language = stem;
            let _ = settings::save(&work_dir, &settings);
        }
    }
    let rolled_back = match update::startup(&work_dir, app::VERSION) {
        update::Startup::Relaunched => return Ok(()),
        update::Startup::RolledBack(v) => Some(v),
        update::Startup::Normal => None,
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args_rx = match instance::start(app::APP_ID, &args) {
        instance::Instance::Secondary => return Ok(()),
        instance::Instance::Primary(rx) => rx,
    };
    update::mark_healthy_later(work_dir.clone());

    // Runtime tokio actif pendant toute la vie de l'interface : `tokio::spawn` fonctionne
    // depuis le fil egui et `Handle::current()` est valide dans `BetterSshApp::new`.
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let _guard = rt.enter();

    let options = NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title(assets::APP_NAME)
            .with_inner_size([1180.0, 560.0])
            .with_min_inner_size([800.0, 480.0])
            .with_icon(load_icon()),
        // Centrée, 560 pt de haut : la fenêtre reste au-dessus de la barre des tâches
        // sur un écran 1080p à 150 % (le centrage ignore la barre des tâches).
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        assets::APP_NAME,
        options,
        Box::new(move |cc| Ok(Box::new(app::BetterSshApp::new(cc, settings, args_rx, rolled_back)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe : {e}"))?;
    Ok(())
}

fn load_icon() -> egui::IconData {
    let image = image::load_from_memory(assets::ICON_PNG)
        .unwrap_or_else(|_| image::DynamicImage::new_rgba8(1, 1));
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    egui::IconData { rgba: rgba.into_raw(), width, height }
}
