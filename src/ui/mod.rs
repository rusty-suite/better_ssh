/// Point d'entrée du rendu de l'interface.
/// Organise les panneaux egui : barre du haut, barre latérale, zone centrale,
/// barre de statut, et fenêtres modales (préférences, snippets, scan réseau).
pub mod file_explorer;
pub mod icons;
pub mod network_scan;
pub mod sidebar;
pub mod snippets;
pub mod system_monitor;
pub mod tab_bar;
pub mod terminal;

use crate::app::{BetterSshApp, ScanConnectDialog, VERSION};
use crate::config::{AuthMethod, ConnectionProfile, MasterKeyCheck, Vault};
use crate::i18n::Lang;
use crate::ssh::session::SftpCommand;
use crate::ui::file_explorer::SftpRequest;
use crate::ui::icons as ph;
use crate::ui::network_scan::ScanAction;
use egui::Context;
use rusty_ui::{tokens as tk, ChromeAction};

// ─── Rendu principal ──────────────────────────────────────────────────────────

/// Rendu complet d'une frame : appelle chaque sous-panneau dans l'ordre.
/// Ordre imposé par egui : panneaux du haut et du bas, puis latéral, puis central.
pub fn render(app: &mut BetterSshApp, ctx: &Context) {
    let mut action = render_header(app, ctx);
    let a = render_status_bar(app, ctx);
    if a != ChromeAction::None {
        action = a;
    }
    render_session_bar(app, ctx);
    render_sidebar(app, ctx);
    render_main_area(app, ctx);
    render_modals(app, ctx);
    match action {
        ChromeAction::Settings => app.show_preferences = !app.show_preferences,
        ChromeAction::About => app.show_about = true,
        ChromeAction::Language => app.show_language = true,
        ChromeAction::Update => app.show_update = true,
        ChromeAction::None => {}
    }
    render_kit_windows(app, ctx);
    app.toasts.show(ctx);
}

/// Nom, description et version affichés dans l'en-tête et la fenêtre À propos.
fn app_info(app: &BetterSshApp) -> rusty_ui::AppInfo {
    rusty_ui::AppInfo {
        name: crate::assets::APP_NAME.into(),
        description: app.lang.app_description.clone(),
        version: VERSION.into(),
    }
}

/// Fenêtres communes de la charte : Langue, À propos, Mise à jour.
fn render_kit_windows(app: &mut BetterSshApp, ctx: &Context) {
    let kit = app.kit_lang.clone();
    let tr = |k: &str| kit.t(k);
    let languages: Vec<(String, String)> = rusty_core::lang::STEMS
        .iter()
        .map(|s| (s.to_string(), rusty_core::lang::language_name(crate::i18n::PACKS, s)))
        .collect();
    let folder = app.work_dir.join("lang").display().to_string();
    if let Some(stem) = rusty_ui::language_window(ctx, &mut app.show_language, &tr, &kit.stem, &languages, &folder) {
        app.settings.common.language = stem;
        app.save_settings();
        app.reload_language(ctx);
    }
    let info = app_info(app);
    rusty_ui::about_window(
        ctx,
        &mut app.show_about,
        &tr,
        &info,
        crate::assets::APP_LICENSE,
        crate::assets::APP_REPOSITORY,
        crate::assets::APP_AUTHORS,
    );
    if let Some(version) = app.update.as_ref().map(|r| r.version.clone()) {
        let installing = app.install_rx.is_some();
        let choice = rusty_ui::update_window(ctx, &mut app.show_update, &tr, &version, installing);
        if choice == rusty_ui::UpdateChoice::Install && !installing {
            app.start_install();
        }
    }
}

// ─── En-tête et barre des sessions ────────────────────────────────────────────

/// En-tête de la charte : logo, nom, description, version, Langue et Paramètres.
fn render_header(app: &mut BetterSshApp, ctx: &Context) -> ChromeAction {
    let info = app_info(app);
    let code = app.kit_lang.code();
    rusty_ui::app_header(ctx, &mut app.logo, &info, &code)
}

/// Barre des sessions : onglets à gauche, outils (scan réseau, snippets, À propos)
/// à droite.
fn render_session_bar(app: &mut BetterSshApp, ctx: &Context) {
    let panel = egui::TopBottomPanel::top("session_bar")
        .frame(
            egui::Frame::new()
                .fill(tk::BG_PANEL)
                .stroke(egui::Stroke::new(1.0, tk::BORDER))
                .inner_margin(egui::Margin::symmetric(tk::SPACE_4 as i8, tk::SPACE_1 as i8)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                tab_bar::render(app, ui);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let about = rusty_ui::texts(ui.ctx()).about.clone();
                    if rusty_ui::icon_button(ui, ph::INFO, &about).clicked() {
                        app.show_about = true;
                    }
                    if rusty_ui::icon_button(ui, ph::CODE, &app.lang.snip_window_title).clicked() {
                        app.show_snippets = !app.show_snippets;
                    }
                    if rusty_ui::icon_button(ui, ph::MAGNIFYING_GLASS, &app.lang.topbar_scanner_hint).clicked() {
                        app.show_network_scan = !app.show_network_scan;
                    }
                });
            });
        });
    rusty_ui::keyboard::register_rect(ctx, rusty_ui::Zone::Toolbar, panel.response.rect);
}

// ─── Barre latérale ───────────────────────────────────────────────────────────

/// Panneau latéral gauche : liste des profils, recherche, création.
fn render_sidebar(app: &mut BetterSshApp, ctx: &Context) {
    egui::SidePanel::left("sidebar")
        .default_width(220.0)
        .width_range(150.0..=350.0)
        .show(ctx, |ui| {
            sidebar::render(app, ui);
        });
}

// ─── Zone centrale ────────────────────────────────────────────────────────────

/// Zone centrale : terminal + panneaux optionnels (SFTP, monitoring).
fn render_main_area(app: &mut BetterSshApp, ctx: &Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        if app.tabs.is_empty() {
            // Pas de session ouverte → page d'accueil avec raccourcis.
            render_welcome(app, ui);
            return;
        }
        let idx = app.active_tab;
        if idx >= app.tabs.len() { return; }

        let show_explorer = app.tabs[idx].show_file_explorer;
        let show_monitor  = app.tabs[idx].show_system_monitor;

        // Le moniteur système s'affiche en bas dans un panneau séparé.
        if show_monitor {
            egui::TopBottomPanel::bottom("sys_monitor")
                .default_height(200.0)
                .show_inside(ui, |ui| {
                    system_monitor::render(&mut app.tabs[idx].system_monitor, ui, &app.lang);
                });
        }

        // L'explorateur SFTP s'affiche à droite en split-pane.
        if show_explorer {
            let username    = app.tabs[idx].profile.username.clone();
            let current_uid = app.tabs[idx].file_explorer.current_uid;
            let mut sftp_req: Option<file_explorer::SftpRequest> = None;
            egui::SidePanel::right("file_explorer")
                .default_width(360.0)
                .show_inside(ui, |ui| {
                    sftp_req = file_explorer::render(
                        &mut app.tabs[idx].file_explorer, ui, &app.lang, &username, current_uid,
                    );
                });
            // Traitement de la requête SFTP retournée par l'explorateur.
            if let Some(req) = sftp_req {
                handle_sftp_request(app, idx, req);
            }

            // Fallback : premier chargement si l'explorateur est visible,
            // la session connectée, mais aucun listage n'a encore eu lieu.
            if app.tabs[idx].connected
                && !app.tabs[idx].file_explorer.loaded
                && !app.tabs[idx].file_explorer.loading
            {
                let path = app.tabs[idx].file_explorer.current_path.clone();
                app.tabs[idx].file_explorer.loading = true;
                if let Some(session) = &app.tabs[idx].session {
                    session.send_sftp(SftpCommand::ListDir(path));
                }
            }
        }

        // Le terminal occupe le reste de la zone centrale.
        // Si l'utilisateur a validé une saisie, on la transmet à la session SSH.
        let modal_open = app.sidebar.show_new_profile
            || app.show_preferences
            || app.show_snippets
            || app.show_network_scan
            || app.pending_scan_connect.is_some();
        if let Some(bytes) = terminal::render(&mut app.tabs[idx].terminal, ui, modal_open, &app.lang) {
            if let Some(session) = &app.tabs[idx].session {
                session.send_input(bytes);
            }
        }

        // Upload SFTP confirmé dans la popup de transfert de fichier.
        if let Some(upload) = app.tabs[idx].terminal.upload_confirmed.take() {
            if let Some(session) = &app.tabs[idx].session {
                session.upload_file(upload.content, upload.remote_path);
            }
        }
    });
}

// ─── Barre de statut ──────────────────────────────────────────────────────────

/// Barre d'état de la charte (réseau, langues, mise à jour, version) ; l'état de la
/// session active (connexion, utilisateur@hôte, port) est affiché juste au-dessus.
fn render_status_bar(app: &mut BetterSshApp, ctx: &Context) -> ChromeAction {
    let languages = app
        .kit_lang
        .tf("kit.languages_count", &[("count", &rusty_core::lang::STEMS.len().to_string())]);
    let update = app.update.as_ref().map(|r| r.version.clone());
    let action = rusty_ui::status_bar(
        ctx,
        &rusty_ui::StatusInfo { online: app.online, languages: &languages, update: update.as_deref(), version: VERSION },
    );
    egui::TopBottomPanel::bottom("session_status")
        .frame(
            egui::Frame::new()
                .fill(tk::BG_PANEL)
                .stroke(egui::Stroke::new(1.0, tk::BORDER))
                .inner_margin(egui::Margin::symmetric(tk::SPACE_4 as i8, 2)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let caption = |s: &str| rusty_ui::Text::Caption.rich(s);
                if let Some(tab) = app.tabs.get(app.active_tab) {
                    let (status, color) = if tab.connected {
                        (app.lang.status_connected.as_str(), tk::SUCCESS)
                    } else {
                        (app.lang.status_disconnected.as_str(), tk::DANGER)
                    };
                    ui.label(caption(status).color(color));
                    ui.separator();
                    ui.label(caption(&format!("{}@{}", tab.profile.username, tab.profile.host)).color(tk::TEXT_SECONDARY));
                    ui.separator();
                    ui.label(caption(&Lang::fmt_n(&app.lang.status_port, tab.profile.port as usize)).color(tk::TEXT_SECONDARY));
                    ui.separator();
                    ui.label(caption(&tab.profile.display_name()).color(tk::TEXT_PRIMARY));
                } else {
                    ui.label(caption(&app.lang.status_no_session).color(tk::TEXT_MUTED));
                }
            });
        });
    action
}

// ─── Fenêtres modales ─────────────────────────────────────────────────────────

/// Affiche les fenêtres flottantes (préférences, snippets, scan réseau, connexion scan).
fn render_modals(app: &mut BetterSshApp, ctx: &Context) {
    if app.pending_scan_connect.is_some() {
        render_scan_connect_dialog(app, ctx);
    }

    if app.telnet_dialog.is_some() {
        render_telnet_dialog(app, ctx);
    }

    if app.show_preferences {
        render_preferences(app, ctx);
    }

    if app.show_snippets {
        egui::Window::new(&app.lang.snip_window_title)
            .open(&mut app.show_snippets)
            .default_size([520.0, 420.0])
            .show(ctx, |ui| {
                snippets::render(&mut app.snippets, ui, &app.lang);
            });
    }

    // La fenêtre de scan réseau retourne une action (connexion éventuelle).
    if app.show_network_scan {
        let mut open = app.show_network_scan;
        let mut action = ScanAction::None;
        egui::Window::new(&app.lang.scan_window_title)
            .open(&mut open)
            .default_size([780.0, 560.0])
            .show(ctx, |ui| {
                action = network_scan::render(&mut app.network_scan, ui, &app.lang);
            });
        app.show_network_scan = open;

        // Quand l'utilisateur clique "Connecter" dans le scanner : ouvre le dialogue.
        if let ScanAction::Connect(result) = action {
            let ip_str = result.ip.to_string();

            // Cherche un profil existant pour cette adresse IP.
            let existing = app.sidebar.profiles.iter()
                .find(|p| p.host == ip_str)
                .cloned();

            let (username, auth_method, identity_file, existing_profile_id) = match &existing {
                Some(p) => {
                    let key = if let AuthMethod::PublicKey { identity_file } = &p.auth_method {
                        identity_file.clone()
                    } else {
                        String::new()
                    };
                    (p.username.clone(), p.auth_method.clone(), key, Some(p.id.clone()))
                }
                None => ("root".to_string(), AuthMethod::Password, String::new(), None),
            };

            // Tente de charger le mot de passe et le nom d'utilisateur depuis le vault.
            let (password, vault_password_loaded) =
                if let (Some(vault), Some(id)) = (&app.vault, &existing_profile_id) {
                    match vault.get_password(id) {
                        Ok(Some(pw)) => (pw, true),
                        _            => (String::new(), false),
                    }
                } else {
                    (String::new(), false)
                };

            let mut username = username;
            if username.is_empty() || username == "root" {
                if let (Some(vault), Some(id)) = (&app.vault, &existing_profile_id) {
                    if let Ok(Some(u)) = vault.get_username(id) {
                        if !u.is_empty() { username = u; }
                    }
                }
            }

            app.pending_scan_connect = Some(ScanConnectDialog {
                scan_result: result,
                username,
                password,
                auth_method,
                identity_file,
                vault_key_input: String::new(),
                vault_password_loaded,
                is_new: existing.is_none(),
                existing_profile_id,
                vault_error: None,
            });
        }
    }
}

// ─── Écran d'accueil ──────────────────────────────────────────────────────────

/// Affiché quand aucun onglet n'est ouvert. Liste les raccourcis principaux.
fn render_welcome(app: &BetterSshApp, ui: &mut egui::Ui) {
    ui.centered_and_justified(|ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            ui.heading(&app.lang.welcome_title);
            ui.add_space(28.0);

            egui::Frame::new()
                .stroke(egui::Stroke::new(1.0, egui::Color32::DARK_GRAY))
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    egui::Grid::new("shortcuts_grid")
                        .num_columns(2)
                        .spacing([24.0, 6.0])
                        .show(ui, |ui| {
                            shortcut(ui, &app.lang.welcome_new_hint,  &app.lang.welcome_new);
                            shortcut(ui, &app.lang.welcome_scan_hint, &app.lang.welcome_scan);
                            shortcut(ui, "Ctrl+W",       &app.lang.shortcut_close_tab);
                            shortcut(ui, "Ctrl+Tab",     &app.lang.shortcut_next_tab);
                            shortcut(ui, "F2",           &app.lang.shortcut_sftp);
                            shortcut(ui, "F3",           &app.lang.shortcut_monitor);
                            shortcut(ui, "F4",           &app.lang.shortcut_snippets);
                            shortcut(ui, "Ctrl+,",       &app.lang.topbar_prefs);
                            shortcut(ui, "Ctrl+Scroll",  &app.lang.shortcut_zoom);
                            shortcut(ui, "Ctrl+R",       &app.lang.shortcut_history);
                        });
                });
        });
    });
}

/// Affiche une ligne raccourci/description dans la grille d'accueil.
fn shortcut(ui: &mut egui::Ui, keys: &str, desc: &str) {
    ui.label(egui::RichText::new(keys).monospace().strong());
    ui.label(desc);
    ui.end_row();
}

// ─── Fenêtre de préférences ───────────────────────────────────────────────────

/// Fenêtre de préférences : police du terminal, réseau (À propos : fenêtre du kit).
fn render_preferences(app: &mut BetterSshApp, ctx: &Context) {
    let mut open = app.show_preferences;

    egui::Window::new(&app.lang.topbar_prefs)
        .open(&mut open)
        // Taille fixe : la fenêtre reste dans la fenêtre principale, le contenu défile.
        .fixed_size([560.0, (ctx.screen_rect().height() - 220.0).clamp(200.0, 460.0)])
        .collapsible(false)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                // ── Section Police du terminal ─────────────────────────────────
                ui.heading(&app.lang.prefs_font);
                ui.separator();

                // Préréglages nommés (taille de police)
                ui.label(&app.lang.prefs_preset);
                let current_size = app.config.terminal.font_size;
                let _current_preset = app.config.terminal.font_preset.clone();

                // Grille de boutons pour les préréglages
                ui.horizontal_wrapped(|ui| {
                    for (id, size) in terminal::FONT_PRESETS {
                        let selected = (current_size - size).abs() < 0.1;
                        if ui.selectable_label(selected, app.lang.font_preset_label(id))
                            .on_hover_text(format!("{} pt", size))
                            .clicked()
                        {
                            let s = *size;
                            app.config.terminal.font_preset = id.to_string();
                            app.apply_font_size(ctx, s);
                            app.save_config();
                        }
                    }
                });

                ui.add_space(6.0);

                // Curseur de taille personnalisée
                ui.label(&app.lang.prefs_custom_size);
                let mut size = app.config.terminal.font_size;
                let resp = ui.add(
                    egui::Slider::new(&mut size, 8.0_f32..=32.0)
                        .suffix(" pt")
                        .step_by(0.5)
                        .clamping(egui::SliderClamping::Always),
                );
                if resp.changed() {
                    app.config.terminal.font_preset = "custom".into();
                    app.apply_font_size(ctx, size);
                    app.save_config();
                }

                // Aperçu en temps réel de la police
                ui.add_space(4.0);
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(20, 20, 30))
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("user@server:~$ ls -lah /var/log/")
                                .font(egui::FontId::monospace(app.config.terminal.font_size))
                                .color(egui::Color32::from_rgb(100, 220, 100)),
                        );
                        ui.label(
                            egui::RichText::new("drwxr-xr-x  12 root  root   4,0K  jan  15  syslog")
                                .font(egui::FontId::monospace(app.config.terminal.font_size))
                                .color(egui::Color32::from_rgb(200, 200, 200)),
                        );
                    });

                ui.add_space(10.0);

                // ── Section Paramètres réseau par défaut ──────────────────────
                ui.heading(&app.lang.prefs_network);
                ui.separator();
                ui.label(&app.lang.prefs_network_hint);
                ui.add_space(4.0);

                egui::Grid::new("net_prefs_grid")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label(&app.lang.prefs_default_cidr);
                        ui.text_edit_singleline(&mut app.config.network.target_cidr);
                        ui.end_row();

                        ui.label(&app.lang.field_ssh_port);
                        let mut port_str = app.config.network.ssh_port.to_string();
                        if ui.text_edit_singleline(&mut port_str).changed() {
                            app.config.network.ssh_port = port_str.parse().unwrap_or(22);
                        }
                        ui.end_row();

                        ui.label(&app.lang.prefs_timeout);
                        ui.add(
                            egui::Slider::new(&mut app.config.network.timeout_ms, 100..=5000)
                                .suffix(" ms"),
                        );
                        ui.end_row();

                        ui.label(&app.lang.field_concurrency);
                        ui.add(
                            egui::Slider::new(&mut app.config.network.concurrency, 4..=256)
                                .suffix(app.lang.suffix_connections.as_str()),
                        );
                        ui.end_row();
                    });

                ui.add_space(12.0);

                // ── Bouton Sauvegarder ────────────────────────────────────────
                if ui.button(ph::with_text(ph::FLOPPY_DISK, &app.lang.prefs_save)).clicked() {
                    app.save_config();
                }

            });
        });

    app.show_preferences = open;
}

// ─── Dialogue Telnet ──────────────────────────────────────────────────────────

/// Fenêtre modale de saisie hôte/port pour une connexion Telnet brute.
fn render_telnet_dialog(app: &mut BetterSshApp, ctx: &Context) {
    let mut dlg = match app.telnet_dialog.take() {
        Some(d) => d,
        None => return,
    };

    let mut do_connect = false;
    let mut do_cancel  = false;

    // Titre sans emoji hors-Noto (voir commentaire dans sidebar.rs).
    egui::Window::new(format!(">_ {}", app.lang.telnet_title))
        .default_size([320.0, 160.0])
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            egui::Grid::new("telnet_dialog_grid")
                .num_columns(2)
                .spacing([8.0, 8.0])
                .show(ui, |ui| {
                    ui.label(&app.lang.dlg_field_host);
                    ui.text_edit_singleline(&mut dlg.host)
                        .on_hover_text(&app.lang.dlg_hint_host);
                    ui.end_row();

                    ui.label(&app.lang.dlg_field_port);
                    ui.add(
                        egui::TextEdit::singleline(&mut dlg.port)
                            .desired_width(80.0)
                            .hint_text("23"),
                    );
                    ui.end_row();
                });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                let port_ok = dlg.port.parse::<u16>().is_ok();
                let can_connect = !dlg.host.is_empty() && port_ok;

                ui.add_enabled_ui(can_connect, |ui| {
                    if ui.button(format!(">_ {}", app.lang.btn_connect)).clicked() {
                        do_connect = true;
                    }
                });
                if ui.button(&app.lang.dlg_cancel_btn).clicked() {
                    do_cancel = true;
                }
            });
        });

    if do_cancel { return; }
    if do_connect {
        let host = dlg.host.clone();
        let port: u16 = dlg.port.parse().unwrap_or(23);
        app.open_telnet(host, port);
        return;
    }
    // Pas d'action → maintient le dialogue ouvert.
    app.telnet_dialog = Some(dlg);
}

// ─── Dialogue de connexion depuis le scan réseau ──────────────────────────────

/// Fenêtre modale de saisie des identifiants affichée après "Connecter" dans le scanner.
/// - Première connexion : formulaire vide (ou pré-rempli avec les valeurs par défaut).
/// - Reconnexion : pré-rempli depuis le profil existant ; mot de passe chargé du vault.
/// - Vault : déverrouillable dans le même dialogue ; le mot de passe est chiffré sur disque.
fn render_scan_connect_dialog(app: &mut BetterSshApp, ctx: &Context) {
    // Clone l'état pour travailler dans la closure egui (évite les conflits de borrow).
    let mut dlg = match app.pending_scan_connect.take() {
        Some(d) => d,
        None => return,
    };

    let ip_str   = dlg.scan_result.ip.to_string();
    let hostname = dlg.scan_result.hostname.clone().unwrap_or_else(|| ip_str.clone());
    let banner   = dlg.scan_result.ssh_banner.clone().unwrap_or_else(|| "-".into());

    let title = if dlg.is_new {
        Lang::fmt(&app.lang.scan_connect_title_new, &[("host", &hostname)])
    } else {
        Lang::fmt(&app.lang.scan_connect_title_existing, &[("host", &hostname)])
    };

    let mut do_connect = false;
    let mut do_cancel  = false;

    egui::Window::new(title)
        .default_size([460.0, 440.0])
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            // ── Informations de l'hôte ────────────────────────────────────────
            egui::Frame::new()
                .fill(ui.visuals().faint_bg_color)
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong(&app.lang.dlg_field_host);
                        ui.label(&ip_str);
                        if hostname != ip_str {
                            ui.label(egui::RichText::new(format!("({hostname})")).weak());
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.strong(&app.lang.field_ssh_banner);
                        ui.label(egui::RichText::new(&banner).monospace().small());
                    });
                    if !dlg.is_new {
                        ui.label(
                            egui::RichText::new(&app.lang.scan_connect_existing)
                                .small()
                                .color(egui::Color32::from_rgb(80, 180, 80)),
                        );
                    }
                });

            ui.add_space(8.0);

            egui::Grid::new("scan_connect_grid")
                .num_columns(2)
                .spacing([8.0, 8.0])
                .show(ui, |ui| {
                    // ── Nom d'utilisateur ─────────────────────────────────────
                    ui.label(&app.lang.dlg_field_user);
                    ui.text_edit_singleline(&mut dlg.username);
                    ui.end_row();

                    // ── Méthode d'authentification ────────────────────────────
                    ui.label(&app.lang.dlg_field_auth);
                    egui::ComboBox::new("scan_auth_combo", "")
                        .selected_text(app.lang.auth_method(&dlg.auth_method))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut dlg.auth_method, AuthMethod::Password, &app.lang.dlg_auth_password,
                            );
                            ui.selectable_value(
                                &mut dlg.auth_method, AuthMethod::Agent, &app.lang.dlg_auth_agent,
                            );
                            if ui.selectable_label(
                                matches!(dlg.auth_method, AuthMethod::PublicKey { .. }),
                                &app.lang.dlg_auth_key,
                            ).clicked() {
                                dlg.auth_method = AuthMethod::PublicKey {
                                    identity_file: dlg.identity_file.clone(),
                                };
                            }
                        });
                    ui.end_row();

                    // ── Champs selon la méthode ───────────────────────────────
                    match &mut dlg.auth_method {
                        AuthMethod::Password => {
                            ui.label(&app.lang.dlg_field_password);
                            ui.vertical(|ui| {
                                if dlg.vault_password_loaded {
                                    // Mot de passe chargé depuis le vault → indicateur vert.
                                    ui.label(
                                        egui::RichText::new(&app.lang.dlg_pw_loaded)
                                            .small()
                                            .color(egui::Color32::from_rgb(80, 200, 80)),
                                    );
                                    // Permet quand même de remplacer le mot de passe.
                                    ui.add(
                                        egui::TextEdit::singleline(&mut dlg.password)
                                            .password(true)
                                            .hint_text(&app.lang.dlg_pw_hint_loaded),
                                    );
                                } else {
                                    ui.add(
                                        egui::TextEdit::singleline(&mut dlg.password)
                                            .password(true)
                                            .hint_text(&app.lang.dlg_pw_hint),
                                    );
                                }
                            });
                            ui.end_row();

                            // ══════════════════════════════════════════════════
                            // SECTION VAULT — NE JAMAIS SUPPRIMER CE BLOC
                            // Ce champ de clé maître DOIT être présent dans TOUT
                            // dialogue de création/connexion de session, sans exception.
                            // ══════════════════════════════════════════════════
                            ui.label(&app.lang.dlg_field_vault);
                            ui.vertical(|ui| {
                                if app.vault.is_some() {
                                    ui.label(
                                        egui::RichText::new(&app.lang.dlg_vault_unlocked)
                                            .small()
                                            .color(egui::Color32::from_rgb(80, 200, 80)),
                                    );
                                    ui.label(
                                        egui::RichText::new(&app.lang.scan_vault_encrypt)
                                            .small()
                                            .weak(),
                                    );
                                } else {
                                    ui.label(
                                        egui::RichText::new(&app.lang.dlg_vault_locked).small().weak(),
                                    );
                                    if ui.add(
                                        egui::TextEdit::singleline(&mut dlg.vault_key_input)
                                            .password(true)
                                            .hint_text(&app.lang.scan_vault_key_hint),
                                    ).changed() {
                                        dlg.vault_error = None;
                                    }
                                    if let Some(err) = &dlg.vault_error.clone() {
                                        ui.label(
                                            ph::colored(ph::WARNING, err, 12.0, rusty_ui::tokens::DANGER),
                                        );
                                    }
                                }
                            });
                            ui.end_row();
                        }
                        AuthMethod::PublicKey { identity_file } => {
                            ui.label(&app.lang.dlg_field_keyfile);
                            ui.horizontal(|ui| {
                                if ui.text_edit_singleline(identity_file).changed() {
                                    dlg.identity_file = identity_file.clone();
                                }
                                if ui.button("…").clicked() {
                                    if let Some(p) = rfd::FileDialog::new()
                                        .set_title(&app.lang.dlg_browse_title)
                                        .pick_file()
                                    {
                                        *identity_file = p.to_string_lossy().into_owned();
                                        dlg.identity_file = identity_file.clone();
                                    }
                                }
                            });
                            ui.end_row();
                        }
                        AuthMethod::Agent => {
                            ui.label("");
                            ui.label(
                                egui::RichText::new(&app.lang.scan_agent_hint)
                                    .small()
                                    .weak(),
                            );
                            ui.end_row();
                        }
                    }
                });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                let can_connect = !dlg.username.is_empty()
                    && match &dlg.auth_method {
                        AuthMethod::Password   => dlg.vault_password_loaded || !dlg.password.is_empty(),
                        AuthMethod::PublicKey { identity_file } => !identity_file.is_empty(),
                        AuthMethod::Agent      => true,
                    };

                ui.add_enabled_ui(can_connect, |ui| {
                    if ui.button(ph::with_text(ph::PLUG, &app.lang.btn_connect)).clicked() {
                        do_connect = true;
                    }
                });
                if ui.button(&app.lang.dlg_cancel_btn).clicked() {
                    do_cancel = true;
                }
            });
        });

    if do_cancel {
        // Dialogue fermé sans connexion → on ne remet pas dlg dans l'app.
        return;
    }

    if do_connect {
        // ── 1. Déverrouiller ou créer le vault si une clé a été saisie ────────
        if app.vault.is_none() && !dlg.vault_key_input.is_empty() {
            let vault = Vault::new(dlg.vault_key_input.clone());
            if matches!(vault.master_key_ok(), Ok(MasterKeyCheck::Wrong)) {
                dlg.vault_error = Some(app.lang.vault_wrong_password.clone());
                app.pending_scan_connect = Some(dlg);
                return;
            }
            dlg.vault_error = None;
            app.vault = Some(vault);
        }

        // ── 2. Construire ou mettre à jour le profil ──────────────────────────
        let host = ip_str.clone();
        let name = dlg.scan_result.hostname.clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| host.clone());

        let profile_id = dlg.existing_profile_id.clone().unwrap_or_else(|| {
            // Génère un nouvel ID hex.
            use rand::Rng;
            format!("{:016x}", rand::thread_rng().gen::<u64>())
        });

        let profile = ConnectionProfile {
            id: profile_id.clone(),
            name,
            host,
            port: dlg.scan_result.ssh_banner
                .as_deref()
                .map(|_| 22u16)  // port 22 par défaut (détection banner confirme SSH)
                .unwrap_or(22),
            username: dlg.username.clone(),
            auth_method: dlg.auth_method.clone(),
            tags: Vec::new(),
            color_tag: None,
            last_connected: None,
            jump_host: None,
            connection_timeout_secs: 30,
        };

        // Sauvegarde le profil dans la liste.
        match app.sidebar.profiles.iter().position(|p| p.id == profile_id) {
            Some(i) => app.sidebar.profiles[i] = profile.clone(),
            None    => app.sidebar.profiles.push(profile.clone()),
        }
        app.save_config();

        // ── 3. Stocker hôte, utilisateur et mot de passe dans le vault ──────────
        if let Some(vault) = &app.vault {
            if let Err(e) = vault.store_address(&profile_id, &profile.host) {
                log::error!("Impossible de sauvegarder l'hôte dans le vault : {e}");
            }
            if let Err(e) = vault.store_username(&profile_id, &dlg.username) {
                log::error!("Impossible de sauvegarder l'utilisateur dans le vault : {e}");
            }
        }

        let password_to_use = if !dlg.password.is_empty() {
            if let Some(vault) = &app.vault {
                if let Err(e) = vault.store_password(&profile_id, &dlg.password) {
                    log::error!("Impossible de sauvegarder dans le vault : {e}");
                }
            }
            Some(dlg.password.clone())
        } else if dlg.vault_password_loaded {
            // Mot de passe vide mais chargé du vault → le recharger pour la session.
            app.vault.as_ref()
                .and_then(|v| v.get_password(&profile_id).ok().flatten())
        } else {
            None
        };

        // ── 4. Ouvrir la session SSH ──────────────────────────────────────────
        app.open_profile(profile, password_to_use);
        return;
    }

    // Pas d'action → remet le dialogue dans l'app pour continuer l'affichage.
    app.pending_scan_connect = Some(dlg);
}

// ─── Gestion des requêtes SFTP de l'explorateur ───────────────────────────────

/// Traduit une requête UI de l'explorateur en commande SFTP réelle et l'envoie
/// à la task SFTP de l'onglet courant. Le résultat arrivera via SessionEvent.
fn handle_sftp_request(app: &mut BetterSshApp, tab_idx: usize, req: SftpRequest) {
    if tab_idx >= app.tabs.len() { return; }

    match req {
        SftpRequest::ListDir(path) => {
            app.tabs[tab_idx].file_explorer.loading = true;
            app.tabs[tab_idx].file_explorer.entries.clear();
            if let Some(session) = &app.tabs[tab_idx].session {
                session.send_sftp(SftpCommand::ListDir(path));
            }
        }
        SftpRequest::Rename { from, to } => {
            if let Some(session) = &app.tabs[tab_idx].session {
                session.send_sftp(SftpCommand::Rename { from, to });
            }
        }
        SftpRequest::DeletePaths(paths) => {
            // Identifie les dossiers avant le retrait optimiste de l'affichage.
            let dir_paths: std::collections::HashSet<String> = app.tabs[tab_idx]
                .file_explorer.entries.iter()
                .filter(|e| e.is_dir && paths.contains(&e.path))
                .map(|e| e.path.clone())
                .collect();
            // Retrait optimiste : la liste se rafraîchira via SftpOpResult.
            let paths_set: std::collections::HashSet<String> = paths.iter().cloned().collect();
            app.tabs[tab_idx].file_explorer.entries
                .retain(|e| !paths_set.contains(&e.path));
            if let Some(session) = &app.tabs[tab_idx].session {
                for path in paths {
                    if dir_paths.contains(&path) {
                        session.send_sftp(SftpCommand::DeleteDir(path));
                    } else {
                        session.send_sftp(SftpCommand::Delete(path));
                    }
                }
            }
        }
        SftpRequest::MovePaths { paths, dest } => {
            if let Some(session) = &app.tabs[tab_idx].session {
                session.send_sftp(SftpCommand::MovePaths { paths, dest });
            }
        }
        SftpRequest::Mkdir(path) => {
            if let Some(session) = &app.tabs[tab_idx].session {
                session.send_sftp(SftpCommand::Mkdir(path));
            }
        }
        SftpRequest::CreateFile(path) => {
            if let Some(session) = &app.tabs[tab_idx].session {
                session.send_sftp(SftpCommand::CreateFile(path));
            }
        }
        SftpRequest::Download { remote } => {
            // Demande le chemin local via un dialogue natif.
            if let Some(local) = rfd::FileDialog::new()
                .set_title(&app.lang.save_as_title)
                .set_file_name(remote.rsplit('/').next().unwrap_or("fichier"))
                .save_file()
            {
                if let Some(session) = &app.tabs[tab_idx].session {
                    session.send_sftp(SftpCommand::Download { remote, local });
                }
            }
        }
    }
}
