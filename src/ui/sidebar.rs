/// Barre latérale gauche : liste des profils de connexion avec recherche,
/// tri et bouton de création. Double-clic sur un profil → ouvre un onglet SSH.
use crate::app::BetterSshApp;
use crate::config::{AuthMethod, ConnectionProfile, Vault};
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
        }
    }
}

// ─── Rendu egui ──────────────────────────────────────────────────────────────

pub fn render(app: &mut BetterSshApp, ui: &mut Ui) {
    ui.heading("Profils SSH");
    ui.separator();

    // ── Barre de recherche ────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label("🔍");
        ui.text_edit_singleline(&mut app.sidebar.search)
            .on_hover_text("Filtre par nom, hôte ou étiquette");
    });
    ui.add_space(4.0);

    // ── Bouton nouveau profil ─────────────────────────────────────────────────
    if ui.button("＋ Nouvelle connexion").clicked() {
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
    let mut to_open: Option<usize> = None;
    let mut to_edit: Option<usize> = None;
    let mut to_delete: Option<usize> = None;

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
            // Indicateur visuel si une session est déjà ouverte pour ce profil.
            let is_active = app.tabs.iter().any(|t| t.profile.id == profile.id);
            let indicator = if is_active { "●" } else { "○" };
            // Quand le vault est verrouillé, hôte et utilisateur sont vides.
            let vault_locked = profile.host.is_empty() || profile.username.is_empty();
            let label = if vault_locked {
                format!("{indicator} {} [🔒]", profile.display_name())
            } else {
                format!("{indicator} {}@{}", profile.username, profile.display_name())
            };
            let tags = profile.tags.clone();

            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(4.0, 2.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let resp = ui.selectable_label(is_active, &label);
                        // Double-clic → ouvrir la session
                        if resp.double_clicked() { to_open = Some(i); }
                        let hover = if vault_locked {
                            format!("Port : {}\n🔒 Vault verrouillé — déverrouillez pour voir l'hôte\nDouble-clic pour connecter", profile.port)
                        } else {
                            format!("{}:{}\nDouble-clic pour connecter", profile.host, profile.port)
                        };
                        resp.on_hover_text(hover);

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("🗑").on_hover_text("Supprimer").clicked() {
                                to_delete = Some(i);
                            }
                            if ui.small_button("✏").on_hover_text("Modifier").clicked() {
                                to_edit = Some(i);
                            }
                        });
                    });
                    // Affiche les étiquettes sous le nom du profil.
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
        let mut profile = app.sidebar.profiles[i].clone();
        // Charge hôte et utilisateur depuis le vault si disponible.
        if let Some(vault) = &app.vault {
            if profile.host.is_empty() {
                profile.host = vault.get_host(&profile.id).ok().flatten().unwrap_or_default();
            }
            if profile.username.is_empty() {
                profile.username = vault.get_username(&profile.id).ok().flatten().unwrap_or_default();
            }
        }
        // Ouvre toujours le dialogue (nécessaire pour déverrouiller le vault si besoin).
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
    if let Some(i) = to_edit {
        let mut profile = app.sidebar.profiles[i].clone();
        // Charge hôte, utilisateur et mot de passe depuis le vault.
        if let Some(vault) = &app.vault {
            if profile.host.is_empty() {
                profile.host = vault.get_host(&profile.id).ok().flatten().unwrap_or_default();
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

    let title = if profile.name.is_empty() { "Nouvelle connexion" } else { "Modifier le profil" };

    egui::Window::new(title)
        .default_size([440.0, 520.0])
        .collapsible(false)
        .show(ctx, |ui| {
            egui::Grid::new("profile_grid")
                .num_columns(2)
                .spacing([8.0, 8.0])
                .show(ui, |ui| {
                    // ── Champs de base ─────────────────────────────────────────
                    ui.label("Nom :");
                    ui.text_edit_singleline(&mut profile.name)
                        .on_hover_text("Nom affiché dans la barre latérale");
                    ui.end_row();

                    ui.label("Hôte :");
                    ui.text_edit_singleline(&mut profile.host)
                        .on_hover_text("Adresse IP ou nom DNS");
                    ui.end_row();

                    ui.label("Port :");
                    let mut port_str = profile.port.to_string();
                    if ui.add(
                        egui::TextEdit::singleline(&mut port_str).desired_width(60.0)
                    ).changed() {
                        profile.port = port_str.parse().unwrap_or(22);
                    }
                    ui.end_row();

                    ui.label("Utilisateur :");
                    ui.text_edit_singleline(&mut profile.username);
                    ui.end_row();

                    // ── Méthode d'authentification ─────────────────────────────
                    ui.label("Authentification :");
                    egui::ComboBox::new("auth_method_combo", "")
                        .selected_text(profile.auth_method.to_string())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut profile.auth_method, AuthMethod::Password, "Mot de passe"
                            );
                            ui.selectable_value(
                                &mut profile.auth_method, AuthMethod::Agent, "Agent SSH"
                            );
                            if ui.selectable_label(
                                matches!(profile.auth_method, AuthMethod::PublicKey { .. }),
                                "Clé privée",
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

                    // Champ mot de passe (masqué) si auth par password.
                    if profile.auth_method == AuthMethod::Password {
                        ui.label("Mot de passe :");
                        ui.vertical(|ui| {
                            if app.sidebar.vault_password_loaded {
                                ui.label(
                                    egui::RichText::new("✓ Chargé depuis le vault")
                                        .small()
                                        .color(egui::Color32::from_rgb(80, 200, 80)),
                                );
                            }
                            ui.add(
                                egui::TextEdit::singleline(&mut pending_password)
                                    .password(true)
                                    .hint_text(if app.sidebar.vault_password_loaded {
                                        "Laisser vide pour réutiliser le vault"
                                    } else {
                                        "Mot de passe SSH"
                                    }),
                            );
                        });
                        ui.end_row();
                    }

                    // ── Section vault (toujours visible) ──────────────────────
                    // Hôte et utilisateur sont chiffrés → vault requis pour tous les profils.
                    ui.label("Vault :");
                    ui.vertical(|ui| {
                        if app.vault.is_some() {
                            ui.label(
                                egui::RichText::new("🔓 Vault déverrouillé")
                                    .small()
                                    .color(egui::Color32::from_rgb(80, 200, 80)),
                            );
                            ui.label(
                                egui::RichText::new(
                                    "Hôte, utilisateur et mot de passe seront chiffrés."
                                )
                                .small()
                                .weak(),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new("🔒 Vault verrouillé").small().weak(),
                            );
                            ui.label(
                                egui::RichText::new(
                                    "Requis pour chiffrer hôte et utilisateur."
                                )
                                .small()
                                .weak(),
                            );
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut vault_key_input)
                                        .password(true)
                                        .hint_text("Clé maître du vault")
                                        .desired_width(160.0),
                                );
                                // Déverrouille le vault et charge immédiatement les valeurs
                                // chiffrées dans les champs du formulaire.
                                let can_unlock = !vault_key_input.is_empty();
                                if ui.add_enabled(can_unlock, egui::Button::new("🔓 Déverrouiller")).clicked() {
                                    app.vault = Some(Vault::new(vault_key_input.clone()));
                                    app.hydrate_profiles_from_vault();
                                    // Hydrate aussi le profil en cours d'édition.
                                    if let (Some(vault), Some(ep)) =
                                        (&app.vault, &mut app.sidebar.edit_profile)
                                    {
                                        if ep.host.is_empty() {
                                            ep.host = vault.get_host(&ep.id)
                                                .ok().flatten().unwrap_or_default();
                                        }
                                        if ep.username.is_empty() {
                                            ep.username = vault.get_username(&ep.id)
                                                .ok().flatten().unwrap_or_default();
                                        }
                                        if ep.auth_method == AuthMethod::Password {
                                            if let Ok(Some(pw)) = vault.get_password(&ep.id) {
                                                app.sidebar.pending_password = pw;
                                                app.sidebar.vault_password_loaded = true;
                                            }
                                        }
                                    }
                                    vault_key_input.clear();
                                }
                            });
                        }
                    });
                    ui.end_row();

                    // Sélecteur de fichier clé si auth par clé privée.
                    if let AuthMethod::PublicKey { identity_file } = &mut profile.auth_method {
                        ui.label("Fichier clé :");
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(identity_file);
                            if ui.button("…").on_hover_text("Parcourir…").clicked() {
                                if let Some(p) = rfd::FileDialog::new()
                                    .set_title("Sélectionner la clé privée")
                                    .pick_file()
                                {
                                    *identity_file = p.to_string_lossy().into_owned();
                                }
                            }
                        });
                        ui.end_row();
                    }

                    // ── Options avancées ───────────────────────────────────────
                    ui.label("Étiquettes :");
                    let mut tags_str = profile.tags.join(", ");
                    if ui.text_edit_singleline(&mut tags_str)
                        .on_hover_text("Séparées par des virgules (ex: prod, web)")
                        .changed()
                    {
                        profile.tags = tags_str
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                    }
                    ui.end_row();

                    ui.label("Hôte de saut :");
                    let mut jump = profile.jump_host.clone().unwrap_or_default();
                    if ui.text_edit_singleline(&mut jump)
                        .on_hover_text("Bastion / ProxyJump (ex: bastion.example.com)")
                        .changed()
                    {
                        profile.jump_host = if jump.is_empty() { None } else { Some(jump) };
                    }
                    ui.end_row();

                    ui.label("Timeout (s) :");
                    ui.add(
                        egui::Slider::new(&mut profile.connection_timeout_secs, 5..=120)
                            .suffix(" s"),
                    );
                    ui.end_row();
                });

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(6.0);

            // ── Boutons d'action ───────────────────────────────────────────────
            ui.horizontal(|ui| {
                if ui.button("💾 Sauvegarder").clicked()    { action = DialogAction::Save; }
                if ui.button("🔌 Connecter").clicked()      { action = DialogAction::Connect; }
                if ui.button("✕ Annuler").clicked()         { action = DialogAction::Cancel; }
            });
        });

    // Réécriture des clones édités dans l'état de la sidebar.
    app.sidebar.edit_profile    = Some(profile.clone());
    app.sidebar.pending_password = pending_password;
    app.sidebar.vault_key_input  = vault_key_input;

    // Application de l'action choisie (hors closure pour éviter les conflits de borrow).
    match action {
        DialogAction::Save => {
            // Déverrouille le vault si une clé vient d'être saisie.
            if app.vault.is_none() && !app.sidebar.vault_key_input.is_empty() {
                app.vault = Some(Vault::new(app.sidebar.vault_key_input.clone()));
                app.hydrate_profiles_from_vault();
            }
            // Chiffre et sauvegarde hôte, utilisateur et mot de passe dans le vault.
            if let Some(vault) = &app.vault {
                if !profile.host.is_empty() {
                    if let Err(e) = vault.store_host(&profile.id, &profile.host) {
                        log::error!("Impossible de sauvegarder l'hôte dans le vault : {e}");
                    }
                }
                if !profile.username.is_empty() {
                    if let Err(e) = vault.store_username(&profile.id, &profile.username) {
                        log::error!("Impossible de sauvegarder l'utilisateur dans le vault : {e}");
                    }
                }
                if !app.sidebar.pending_password.is_empty() {
                    if let Err(e) = vault.store_password(&profile.id, &app.sidebar.pending_password) {
                        log::error!("Impossible de sauvegarder le mot de passe dans le vault : {e}");
                    }
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
            // Déverrouille le vault si une clé vient d'être saisie.
            if app.vault.is_none() && !app.sidebar.vault_key_input.is_empty() {
                app.vault = Some(Vault::new(app.sidebar.vault_key_input.clone()));
                app.hydrate_profiles_from_vault();
            }
            // Chiffre et sauvegarde hôte, utilisateur et mot de passe dans le vault.
            if let Some(vault) = &app.vault {
                if !profile.host.is_empty() {
                    if let Err(e) = vault.store_host(&profile.id, &profile.host) {
                        log::error!("Impossible de sauvegarder l'hôte dans le vault : {e}");
                    }
                }
                if !profile.username.is_empty() {
                    if let Err(e) = vault.store_username(&profile.id, &profile.username) {
                        log::error!("Impossible de sauvegarder l'utilisateur dans le vault : {e}");
                    }
                }
            }
            upsert_profile(&mut app.sidebar.profiles, profile.clone());
            app.save_config();

            let pw = if !app.sidebar.pending_password.is_empty() {
                // Nouveau mot de passe saisi → le stocker dans le vault si disponible.
                if let Some(vault) = &app.vault {
                    if let Err(e) = vault.store_password(&profile.id, &app.sidebar.pending_password) {
                        log::error!("Impossible de sauvegarder le mot de passe dans le vault : {e}");
                    }
                }
                Some(app.sidebar.pending_password.clone())
            } else if app.sidebar.vault_password_loaded {
                // Mot de passe vide mais pré-chargé du vault → le recharger pour la session.
                app.vault.as_ref()
                    .and_then(|v| v.get_password(&profile.id).ok().flatten())
            } else {
                None
            };

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
