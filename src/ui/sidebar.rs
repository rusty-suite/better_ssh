/// Barre latérale gauche : liste des profils de connexion avec recherche,
/// tri et bouton de création. Double-clic sur un profil → ouvre un onglet SSH.
use crate::app::BetterSshApp;
use crate::config::{AuthMethod, ConnectionProfile, MasterKeyCheck, Vault};
use crate::ui::icons as ph;
use egui::Ui;

// ─── État de la barre latérale ────────────────────────────────────────────────

pub struct SidebarState {
    /// Liste complète des profils sauvegardés.
    pub profiles: Vec<ConnectionProfile>,
    /// Texte de recherche / filtre live sur les profils.
    pub search: String,
    /// true = la fenêtre de création/édition de profil est ouverte.
    pub show_new_profile: bool,
    /// Profil en cours d'édition (clone temporaire, pas encore sauvegardé).
    pub edit_profile: Option<ConnectionProfile>,
    /// Mot de passe saisi dans le formulaire (en mémoire non persistée).
    pub pending_password: String,
    /// Saisie de la clé maître du vault dans le dialogue de profil.
    pub vault_key_input: String,
    /// true si pending_password a été pré-chargé automatiquement depuis le vault.
    pub vault_password_loaded: bool,
    /// Message d'erreur de déverrouillage vault (mauvais mot de passe, etc.).
    pub vault_error: Option<String>,
}

impl SidebarState {
    pub fn new(profiles: Vec<ConnectionProfile>) -> Self {
        Self {
            profiles,
            search: String::new(),
            show_new_profile: false,
            edit_profile: None,
            pending_password: String::new(),
            vault_key_input: String::new(),
            vault_password_loaded: false,
            vault_error: None,
        }
    }
}

// ─── Rendu egui ──────────────────────────────────────────────────────────────

pub fn render(app: &mut BetterSshApp, ui: &mut Ui) {
    ui.heading(&app.lang.sidebar_title);
    ui.separator();

    // ── Barre de recherche ────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(ph::MAGNIFYING_GLASS);
        ui.text_edit_singleline(&mut app.sidebar.search)
            .on_hover_text(&app.lang.sidebar_search_hint);
    });
    ui.add_space(4.0);

    // ── Bouton nouveau profil ─────────────────────────────────────────────────
    if ui.button(&app.lang.sidebar_new_connection).clicked() {
        app.sidebar.edit_profile = Some(ConnectionProfile::default());
        app.sidebar.pending_password.clear();
        app.sidebar.vault_key_input.clear();
        app.sidebar.vault_password_loaded = false;
        app.sidebar.show_new_profile = true;
    }

    ui.separator();
    ui.add_space(4.0);

    // ── Liste des profils filtrée ─────────────────────────────────────────────
    let search = app.sidebar.search.to_lowercase();
    let mut to_open:           Option<usize> = None;
    let mut to_connect_direct: Option<usize> = None;
    let mut to_edit:           Option<usize> = None;
    let mut to_delete:         Option<usize> = None;

    egui::ScrollArea::vertical().show(ui, |ui| {
        // Filtre les profils selon la recherche (nom, hôte, tags).
        let matching: Vec<usize> = (0..app.sidebar.profiles.len())
            .filter(|&i| {
                let p = &app.sidebar.profiles[i];
                search.is_empty()
                    || p.name.to_lowercase().contains(&search)
                    || p.host.to_lowercase().contains(&search)
                    || p.tags.iter().any(|t| t.to_lowercase().contains(&search))
            })
            .collect();

        for i in matching {
            let profile = &app.sidebar.profiles[i];
            let is_active = app.tabs.iter().any(|t| t.profile.id == profile.id);
            let indicator = if is_active { "●" } else { "○" };
            let vault_locked = profile.host.is_empty() || profile.username.is_empty();
            // Nom de session (toujours visible).
            let session_name = if profile.name.is_empty() {
                if vault_locked { format!("{} {}", ph::LOCK, app.lang.sidebar_no_name) }
                else { profile.host.clone() }
            } else {
                profile.name.clone()
            };
            // Détails de connexion sur la deuxième ligne.
            let conn_detail = if vault_locked {
                app.lang.sidebar_locked_data.clone()
            } else {
                format!("{}@{}:{}", profile.username, profile.host, profile.port)
            };
            let hover = if vault_locked {
                format!("{}\n{}", app.lang.sidebar_vault_locked_hover, app.lang.sidebar_double_click_hint)
            } else {
                format!("{}@{}:{}\n{}", profile.username, profile.host, profile.port, app.lang.sidebar_double_click_hint)
            };
            let tags = profile.tags.clone();

            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(4.0, 2.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            let resp = ui.selectable_label(
                                is_active,
                                format!("{indicator} {session_name}"),
                            );
                            if resp.double_clicked() { to_open = Some(i); }
                            // Menu contextuel (clic droit) sur la ligne du profil.
                            resp.context_menu(|ui| {
                                if ui.button(format!("{} Connecter", ph::PLUG)).clicked() {
                                    to_connect_direct = Some(i);
                                    ui.close_menu();
                                }
                                ui.separator();
                                if ui.button(format!("{} Modifier", ph::PENCIL)).clicked() {
                                    to_edit = Some(i);
                                    ui.close_menu();
                                }
                                if ui.button(
                                    egui::RichText::new(format!("{} Supprimer", ph::TRASH))
                                        .color(egui::Color32::from_rgb(220, 70, 70))
                                ).clicked() {
                                    to_delete = Some(i);
                                    ui.close_menu();
                                }
                            });
                            resp.on_hover_text(&hover);
                            // Détails user@host:port ou indication vault verrouillé.
                            let detail_color = if vault_locked {
                                egui::Color32::from_rgb(180, 140, 60)
                            } else {
                                ui.visuals().weak_text_color()
                            };
                            ui.label(
                                egui::RichText::new(&conn_detail).small().color(detail_color)
                            );
                        });

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(egui::Button::new(
                                egui::RichText::new(ph::TRASH).color(egui::Color32::from_rgb(220, 70, 70))
                            )).on_hover_text(&app.lang.sidebar_delete_hint).clicked() {
                                to_delete = Some(i);
                            }
                            if ui.small_button(ph::PENCIL).on_hover_text(&app.lang.sidebar_edit_hint).clicked() {
                                to_edit = Some(i);
                            }
                        });
                    });
                    if !tags.is_empty() {
                        ui.horizontal(|ui| {
                            for tag in &tags {
                                ui.label(egui::RichText::new(tag).small().weak());
                            }
                        });
                    }
                });
        }
    });

    // ── Traitement des actions (hors boucle pour éviter les double-borrows) ───
    if let Some(i) = to_open {
        let profile_id = app.sidebar.profiles[i].id.clone();

        // Si un onglet connecté existe déjà pour ce profil, on bascule dessus
        // plutôt que d'ouvrir un second dialogue de connexion.
        if let Some(tab_idx) = app.tabs.iter().position(|t| t.profile.id == profile_id && t.connected) {
            app.active_tab = tab_idx;
        } else {
            // Aucun onglet actif → ouvre le dialogue de connexion.
            let mut profile = app.sidebar.profiles[i].clone();
            if let Some(vault) = &app.vault {
                if profile.host.is_empty() {
                    profile.host = vault.get_address(&profile.id).ok().flatten().unwrap_or_default();
                }
                if profile.username.is_empty() {
                    profile.username = vault.get_username(&profile.id).ok().flatten().unwrap_or_default();
                }
            }
            let (pw, loaded) = if matches!(profile.auth_method, AuthMethod::Password) {
                if let Some(vault) = &app.vault {
                    match vault.get_password(&profile.id) {
                        Ok(Some(p)) => (p, true),
                        _           => (String::new(), false),
                    }
                } else {
                    (String::new(), false)
                }
            } else {
                (String::new(), false)
            };
            app.sidebar.pending_password      = pw;
            app.sidebar.vault_password_loaded = loaded;
            app.sidebar.vault_key_input.clear();
            app.sidebar.edit_profile = Some(profile);
            app.sidebar.show_new_profile = true;
        }
    }
    if let Some(i) = to_connect_direct {
        let profile_id = app.sidebar.profiles[i].id.clone();

        // Déjà connecté → bascule sur l'onglet existant.
        if let Some(tab_idx) = app.tabs.iter().position(|t| t.profile.id == profile_id && t.connected) {
            app.active_tab = tab_idx;
        } else if app.vault.is_none() && Vault::profile_has_encrypted_data(&profile_id) {
            // Vault verrouillé et profil chiffré → affiche le dialogue de déverrouillage.
            // L'utilisateur devra entrer la clé vault, puis la connexion se fera.
            app.sidebar.pending_password.clear();
            app.sidebar.vault_password_loaded = false;
            app.sidebar.vault_key_input.clear();
            app.sidebar.edit_profile = Some(app.sidebar.profiles[i].clone());
            app.sidebar.show_new_profile = true;
        } else {
            // Vault déverrouillé (ou aucune donnée chiffrée) → connexion directe.
            let mut profile = app.sidebar.profiles[i].clone();

            // Lecture groupée : adresse, utilisateur, mot de passe (1 seule I/O vault).
            let (addr, user, vault_pw) = if let Some(vault) = &app.vault {
                vault.get_profile(&profile.id).unwrap_or_default()
            } else {
                (None, None, None)
            };
            if profile.host.is_empty()     { profile.host     = addr.unwrap_or_default(); }
            if profile.username.is_empty() { profile.username = user.unwrap_or_default(); }

            let pw = if matches!(profile.auth_method, AuthMethod::Password) {
                vault_pw
            } else {
                None
            };

            // Si auth Password sans mot de passe disponible → dialogue pour le saisir.
            if matches!(profile.auth_method, AuthMethod::Password) && pw.is_none() {
                app.sidebar.pending_password.clear();
                app.sidebar.vault_password_loaded = false;
                app.sidebar.vault_key_input.clear();
                app.sidebar.edit_profile = Some(profile);
                app.sidebar.show_new_profile = true;
            } else {
                // Connexion immédiate sans afficher le formulaire.
                app.open_profile(profile, pw);
            }
        }
    }
    if let Some(i) = to_edit {
        let mut profile = app.sidebar.profiles[i].clone();
        // Charge hôte, utilisateur et mot de passe depuis le vault.
        if let Some(vault) = &app.vault {
            if profile.host.is_empty() {
                profile.host = vault.get_address(&profile.id).ok().flatten().unwrap_or_default();
            }
            if profile.username.is_empty() {
                profile.username = vault.get_username(&profile.id).ok().flatten().unwrap_or_default();
            }
        }
        let (pw, loaded) = if matches!(profile.auth_method, AuthMethod::Password) {
            if let Some(vault) = &app.vault {
                match vault.get_password(&profile.id) {
                    Ok(Some(p)) => (p, true),
                    _           => (String::new(), false),
                }
            } else {
                (String::new(), false)
            }
        } else {
            (String::new(), false)
        };
        app.sidebar.pending_password      = pw;
        app.sidebar.vault_password_loaded = loaded;
        app.sidebar.vault_key_input.clear();
        app.sidebar.edit_profile = Some(profile);
        app.sidebar.show_new_profile = true;
    }
    if let Some(i) = to_delete {
        let id = app.sidebar.profiles[i].id.clone();
        // Supprime aussi les secrets du vault pour ce profil.
        if let Some(vault) = &app.vault {
            if let Err(e) = vault.remove_profile(&id) {
                log::error!("Impossible de supprimer les secrets vault du profil {id} : {e}");
            }
        }
        app.sidebar.profiles.remove(i);
        app.save_config();
    }

    // ── Dialogue de création/édition ──────────────────────────────────────────
    if app.sidebar.show_new_profile {
        render_profile_dialog(app, ui.ctx());
    }
}

// ─── Dialogue profil ──────────────────────────────────────────────────────────

/// Action choisie dans le dialogue de création/édition.
enum DialogAction { Save, Connect, Cancel, None }

/// Fenêtre modale de création ou d'édition d'un profil SSH.
/// Travaille sur un clone pour éviter les double-borrows de `app.sidebar`.
fn render_profile_dialog(app: &mut BetterSshApp, ctx: &egui::Context) {
    // Clone temporaire : édité dans la fenêtre, réintégré seulement si Save/Connect.
    // IMPORTANT : pas d'hydratation automatique ici — elle écraserait les saisies de
    // l'utilisateur à chaque frame. L'hydratation a lieu une seule fois dans les
    // handlers to_open/to_edit, ou via le bouton « Déverrouiller » ci-dessous.
    let mut profile = app
        .sidebar
        .edit_profile
        .clone()
        .unwrap_or_else(ConnectionProfile::default);
    let mut pending_password = app.sidebar.pending_password.clone();
    let mut vault_key_input  = app.sidebar.vault_key_input.clone();
    let mut action = DialogAction::None;
    // true si un des boutons/champs "Déverrouiller" a été activé ce frame.
    // Le traitement s'effectue après la fermeture de la fenêtre pour éviter
    // les conflits de borrow sur `profile` et `pending_password`.
    let mut pending_unlock = false;
    // Passe le focus au champ suivant dans la séquence clavier du formulaire.
    let mut focus_next = false;

    let title = if profile.name.is_empty() {
        app.lang.dlg_new_title.as_str()
    } else {
        app.lang.dlg_edit_title.as_str()
    };
    // true si ce profil a des données chiffrées dans vault.toml et que le vault est verrouillé.
    let needs_vault_unlock = app.vault.is_none()
        && Vault::profile_has_encrypted_data(&profile.id);

    egui::Window::new(title)
        .default_size([440.0, 520.0])
        .collapsible(false)
        .show(ctx, |ui| {
            if needs_vault_unlock {
                // ── Vue réduite : vault verrouillé ─────────────────────────────
                ui.add_space(8.0);
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new(ph::LOCK).size(32.0));
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(&app.lang.dlg_vault_screen_title).strong());
                    ui.label(egui::RichText::new(&app.lang.dlg_vault_screen_subtitle).small().weak());
                });
                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(&app.lang.dlg_vault_key_label);
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut vault_key_input)
                            .password(true)
                            .hint_text(&app.lang.dlg_vault_key_hint)
                            .desired_width(200.0),
                    );
                    if resp.changed() { app.sidebar.vault_error = None; }
                    if enter_in(&resp, ui) && !vault_key_input.is_empty() {
                        pending_unlock = true;
                    }
                    let can_unlock = !vault_key_input.is_empty();
                    if ui.add_enabled(can_unlock, egui::Button::new(&app.lang.dlg_unlock_btn)).clicked() {
                        pending_unlock = true;
                    }
                });
                if let Some(err) = &app.sidebar.vault_error.clone() {
                    ui.label(
                        egui::RichText::new(format!("{} {err}", ph::WARNING))
                            .color(egui::Color32::from_rgb(220, 70, 70))
                            .small(),
                    );
                }
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button(&app.lang.dlg_cancel_btn).clicked() { action = DialogAction::Cancel; }
                });
            } else {
                // ── Vue complète : vault déverrouillé ou nouveau profil ────────
                egui::Grid::new("profile_grid")
                    .num_columns(2)
                    .spacing([8.0, 8.0])
                    .show(ui, |ui| {
                        ui.label(&app.lang.dlg_field_name);
                        let resp = ui.text_edit_singleline(&mut profile.name)
                            .on_hover_text(&app.lang.dlg_hint_name);
                        // Entrée → passe au champ Hôte.
                        if focus_next { resp.request_focus(); focus_next = false; }
                        if enter_in(&resp, ui) { focus_next = true; }
                        ui.end_row();

                        ui.label(&app.lang.dlg_field_host);
                        ui.vertical(|ui| {
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut profile.host)
                                    .hint_text(&app.lang.dlg_hint_host),
                            ).on_hover_text(&app.lang.dlg_hint_host);
                            if focus_next { resp.request_focus(); focus_next = false; }
                            // Entrée → passe au champ Port.
                            if enter_in(&resp, ui) { focus_next = true; }
                            if profile.host.is_empty() {
                                ui.label(
                                    egui::RichText::new(format!("{} Adresse manquante — à renseigner", ph::WARNING))
                                        .small()
                                        .color(egui::Color32::from_rgb(240, 160, 40)),
                                );
                            }
                        });
                        ui.end_row();

                        ui.label(&app.lang.dlg_field_port);
                        let mut port_str = profile.port.to_string();
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut port_str).desired_width(60.0)
                        );
                        if focus_next { resp.request_focus(); focus_next = false; }
                        if resp.changed() { profile.port = port_str.parse().unwrap_or(22); }
                        // Entrée → parse le port ET passe au champ Utilisateur.
                        if enter_in(&resp, ui) {
                            profile.port = port_str.parse().unwrap_or(22);
                            focus_next = true;
                        }
                        ui.end_row();

                        ui.label(&app.lang.dlg_field_user);
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut profile.username)
                                .hint_text(&app.lang.dlg_hint_user),
                        );
                        if focus_next { resp.request_focus(); focus_next = false; }
                        // Entrée → mot de passe si auth Password, sinon Connecter.
                        if enter_in(&resp, ui) {
                            if profile.auth_method == AuthMethod::Password {
                                focus_next = true;
                            } else {
                                action = DialogAction::Connect;
                            }
                        }
                        ui.end_row();

                        ui.label(&app.lang.dlg_field_auth);
                        egui::ComboBox::new("auth_method_combo", "")
                            .selected_text(profile.auth_method.to_string())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut profile.auth_method, AuthMethod::Password,
                                    &app.lang.dlg_auth_password,
                                );
                                ui.selectable_value(
                                    &mut profile.auth_method, AuthMethod::Agent,
                                    &app.lang.dlg_auth_agent,
                                );
                                if ui.selectable_label(
                                    matches!(profile.auth_method, AuthMethod::PublicKey { .. }),
                                    &app.lang.dlg_auth_key,
                                ).clicked() {
                                    profile.auth_method = AuthMethod::PublicKey {
                                        identity_file: format!(
                                            "{}/.ssh/id_ed25519",
                                            dirs::home_dir().unwrap_or_default().display()
                                        ),
                                    };
                                }
                            });
                        ui.end_row();

                        if profile.auth_method == AuthMethod::Password {
                            ui.label(&app.lang.dlg_field_password);
                            ui.vertical(|ui| {
                                if app.sidebar.vault_password_loaded {
                                    ui.label(
                                        egui::RichText::new(
                                            format!("{} {}", ph::CHECK, app.lang.dlg_pw_loaded)
                                        )
                                        .small()
                                        .color(egui::Color32::from_rgb(80, 200, 80)),
                                    );
                                }
                                let pw_hint = if app.sidebar.vault_password_loaded {
                                    app.lang.dlg_pw_hint_loaded.as_str()
                                } else {
                                    app.lang.dlg_pw_hint.as_str()
                                };
                                let resp = ui.add(
                                    egui::TextEdit::singleline(&mut pending_password)
                                        .password(true)
                                        .hint_text(pw_hint),
                                );
                                if focus_next { resp.request_focus(); focus_next = false; }
                                // Entrée dans le dernier champ → Connecter.
                                if enter_in(&resp, ui) { action = DialogAction::Connect; }
                            });
                            ui.end_row();
                        }

                        // ── Section vault ──────────────────────────────────────
                        ui.label(&app.lang.dlg_field_vault);
                        ui.vertical(|ui| {
                            if app.vault.is_some() {
                                ui.label(
                                    egui::RichText::new(
                                        format!("{} {}", ph::LOCK_OPEN, app.lang.dlg_vault_unlocked)
                                    )
                                    .small()
                                    .color(egui::Color32::from_rgb(80, 200, 80)),
                                );
                                ui.label(
                                    egui::RichText::new(&app.lang.dlg_vault_encrypt)
                                        .small()
                                        .weak(),
                                );
                            } else {
                                ui.label(
                                    egui::RichText::new(
                                        format!("{} {}", ph::LOCK, app.lang.dlg_vault_locked)
                                    )
                                    .small()
                                    .weak(),
                                );
                                ui.label(
                                    egui::RichText::new(&app.lang.dlg_vault_required)
                                        .small()
                                        .weak(),
                                );
                                ui.horizontal(|ui| {
                                    let resp = ui.add(
                                        egui::TextEdit::singleline(&mut vault_key_input)
                                            .password(true)
                                            .hint_text(&app.lang.dlg_vault_key_hint)
                                            .desired_width(160.0),
                                    );
                                    if resp.changed() { app.sidebar.vault_error = None; }
                                    if enter_in(&resp, ui) && !vault_key_input.is_empty() {
                                        pending_unlock = true;
                                    }
                                    let can_unlock = !vault_key_input.is_empty();
                                    if ui.add_enabled(can_unlock, egui::Button::new(
                                        &app.lang.dlg_unlock_btn
                                    )).clicked() {
                                        pending_unlock = true;
                                    }
                                    if let Some(err) = &app.sidebar.vault_error.clone() {
                                        ui.label(
                                            egui::RichText::new(format!("{} {err}", ph::WARNING))
                                                .color(egui::Color32::from_rgb(220, 70, 70))
                                                .small(),
                                        );
                                    }
                                });
                            }
                        });
                        ui.end_row();

                        if let AuthMethod::PublicKey { identity_file } = &mut profile.auth_method {
                            ui.label(&app.lang.dlg_field_keyfile);
                            ui.horizontal(|ui| {
                                ui.text_edit_singleline(identity_file);
                                if ui.button(&app.lang.dlg_browse_btn).clicked() {
                                    if let Some(p) = rfd::FileDialog::new()
                                        .set_title(&app.lang.dlg_browse_title)
                                        .pick_file()
                                    {
                                        *identity_file = p.to_string_lossy().into_owned();
                                    }
                                }
                            });
                            ui.end_row();
                        }

                        ui.label(&app.lang.dlg_field_tags);
                        let mut tags_str = profile.tags.join(", ");
                        if ui.text_edit_singleline(&mut tags_str)
                            .on_hover_text(&app.lang.dlg_hint_tags)
                            .changed()
                        {
                            profile.tags = tags_str
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                        }
                        ui.end_row();

                        ui.label(&app.lang.dlg_field_jump);
                        let mut jump = profile.jump_host.clone().unwrap_or_default();
                        if ui.text_edit_singleline(&mut jump)
                            .on_hover_text(&app.lang.dlg_hint_jump)
                            .changed()
                        {
                            profile.jump_host = if jump.is_empty() { None } else { Some(jump) };
                        }
                        ui.end_row();

                        ui.label(&app.lang.dlg_field_timeout);
                        ui.add(
                            egui::Slider::new(&mut profile.connection_timeout_secs, 5..=120)
                                .suffix(" s"),
                        );
                        ui.end_row();
                    });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    if ui.button(&app.lang.dlg_save_btn).clicked()    { action = DialogAction::Save; }
                    if ui.button(&app.lang.dlg_connect_btn).clicked() { action = DialogAction::Connect; }
                    if ui.button(&app.lang.dlg_cancel_btn).clicked()  { action = DialogAction::Cancel; }
                });
            }
        });

    // ── Traitement du déverrouillage vault (bouton OU touche Entrée) ─────────
    // S'exécute AVANT le writeback pour que les modifications de `profile` et
    // `pending_password` soient incluses dans app.sidebar.edit_profile.
    if pending_unlock && !vault_key_input.is_empty() {
        let vault = Vault::new(vault_key_input.clone());
        match vault.master_key_ok() {
            Ok(MasterKeyCheck::Wrong) => {
                app.sidebar.vault_error = Some("Mot de passe incorrect.".into());
            }
            _ => {
                app.sidebar.vault_error = None;
                if let Err(e) = vault.migrate_if_needed() {
                    log::warn!("Migration vault : {e}");
                }
                if let Ok((addr, user, pw)) = vault.get_profile(&profile.id) {
                    if profile.host.is_empty()     { profile.host     = addr.unwrap_or_default(); }
                    if profile.username.is_empty() { profile.username = user.unwrap_or_default(); }
                    if profile.auth_method == AuthMethod::Password {
                        if let Some(p) = pw {
                            pending_password = p;
                            app.sidebar.vault_password_loaded = true;
                        }
                    }
                }
                app.vault = Some(vault);
                app.hydrate_profiles_from_vault();
                vault_key_input.clear();
            }
        }
    }

    // Réécriture des clones édités dans l'état de la sidebar.
    app.sidebar.edit_profile    = Some(profile.clone());
    app.sidebar.pending_password = pending_password;
    app.sidebar.vault_key_input  = vault_key_input;

    // Application de l'action choisie (hors closure pour éviter les conflits de borrow).
    match action {
        DialogAction::Save => {
            if app.vault.is_none() && !app.sidebar.vault_key_input.is_empty() {
                app.vault = Some(Vault::new(app.sidebar.vault_key_input.clone()));
                app.hydrate_profiles_from_vault();
            }
            // Une seule écriture disque pour les trois champs (rapide avec ChaCha20).
            if let Some(vault) = &app.vault {
                let pw = if app.sidebar.pending_password.is_empty() {
                    None
                } else {
                    Some(app.sidebar.pending_password.as_str())
                };
                if let Err(e) = vault.store_profile(
                    &profile.id,
                    if profile.host.is_empty()     { None } else { Some(&profile.host) },
                    if profile.username.is_empty() { None } else { Some(&profile.username) },
                    pw,
                ) {
                    log::error!("Impossible de sauvegarder dans le vault : {e}");
                }
            }
            upsert_profile(&mut app.sidebar.profiles, profile);
            app.save_config();
            app.sidebar.edit_profile = None;
            app.sidebar.pending_password.clear();
            app.sidebar.vault_key_input.clear();
            app.sidebar.vault_password_loaded = false;
            app.sidebar.show_new_profile = false;
        }
        DialogAction::Connect => {
            if app.vault.is_none() && !app.sidebar.vault_key_input.is_empty() {
                app.vault = Some(Vault::new(app.sidebar.vault_key_input.clone()));
                app.hydrate_profiles_from_vault();
            }

            // Détermine le mot de passe à utiliser pour la session.
            let pw = if !app.sidebar.pending_password.is_empty() {
                Some(app.sidebar.pending_password.clone())
            } else if app.sidebar.vault_password_loaded {
                app.vault.as_ref().and_then(|v| v.get_password(&profile.id).ok().flatten())
            } else {
                None
            };

            // Une seule écriture disque pour hôte, utilisateur et mot de passe éventuel.
            if let Some(vault) = &app.vault {
                let pw_to_store = pw.as_deref().filter(|_| !app.sidebar.vault_password_loaded
                    || !app.sidebar.pending_password.is_empty());
                if let Err(e) = vault.store_profile(
                    &profile.id,
                    if profile.host.is_empty()     { None } else { Some(&profile.host) },
                    if profile.username.is_empty() { None } else { Some(&profile.username) },
                    pw_to_store,
                ) {
                    log::error!("Impossible de sauvegarder dans le vault : {e}");
                }
                // Re-hydrate depuis le vault si les champs sont encore vides.
                if profile.host.is_empty() || profile.username.is_empty() {
                    if let Ok((addr, user, _)) = vault.get_profile(&profile.id) {
                        if profile.host.is_empty()     { profile.host     = addr.unwrap_or_default(); }
                        if profile.username.is_empty() { profile.username = user.unwrap_or_default(); }
                    }
                }
            }

            upsert_profile(&mut app.sidebar.profiles, profile.clone());
            app.save_config();

            app.open_profile(profile, pw);
            app.sidebar.edit_profile = None;
            app.sidebar.pending_password.clear();
            app.sidebar.vault_key_input.clear();
            app.sidebar.vault_password_loaded = false;
            app.sidebar.show_new_profile = false;
        }
        DialogAction::Cancel => {
            app.sidebar.edit_profile = None;
            app.sidebar.pending_password.clear();
            app.sidebar.vault_key_input.clear();
            app.sidebar.vault_password_loaded = false;
            app.sidebar.show_new_profile = false;
        }
        DialogAction::None => {}
    }
}

/// Insère ou met à jour un profil dans la liste (upsert par ID).
fn upsert_profile(profiles: &mut Vec<ConnectionProfile>, profile: ConnectionProfile) {
    match profiles.iter().position(|x| x.id == profile.id) {
        Some(i) => profiles[i] = profile,
        None    => profiles.push(profile),
    }
}

/// Retourne true si le champ TextEdit a perdu le focus via la touche Entrée.
/// Utilisé pour la navigation clavier dans le formulaire de profil.
fn enter_in(resp: &egui::Response, ui: &egui::Ui) -> bool {
    resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
}
