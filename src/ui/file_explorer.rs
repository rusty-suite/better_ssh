/// Explorateur de fichiers SFTP.
/// Affiche l'arborescence du serveur distant avec navigation,
/// menu contextuel et boutons d'upload/création de dossier.
use crate::ssh::sftp::RemoteEntry;
use egui::Ui;

// ─── État du panneau ──────────────────────────────────────────────────────────

pub struct FileExplorerState {
    /// Chemin courant affiché (naviguer dans l'arbre le met à jour).
    pub current_path: String,
    /// Contenu du répertoire courant (mis à jour après chaque navigation).
    pub entries: Vec<RemoteEntry>,
    /// true si un chargement SFTP est en cours (affiche un spinner).
    pub loading: bool,
    /// Filtre texte sur les noms de fichiers.
    pub search_query: String,
    /// true si l'utilisateur édite le chemin directement dans la barre de navigation.
    pub breadcrumb_edit: bool,
    /// Texte en cours d'édition dans la barre de navigation.
    pub breadcrumb_input: String,
    /// Chemin de l'entrée sélectionnée (None si aucune sélection).
    pub selected: Option<String>,
}

impl FileExplorerState {
    pub fn new() -> Self {
        Self {
            current_path: "/".into(),
            entries: Vec::new(),
            loading: false,
            search_query: String::new(),
            breadcrumb_edit: false,
            breadcrumb_input: String::new(),
            selected: None,
        }
    }
}

// ─── Rendu egui ──────────────────────────────────────────────────────────────

pub fn render(state: &mut FileExplorerState, ui: &mut Ui) {
    ui.heading("📁 Fichiers (SFTP)");
    ui.separator();

    // ── Barre de navigation (breadcrumb) ──────────────────────────────────────
    render_breadcrumb(state, ui);

    // ── Filtre de fichiers ────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label("🔍");
        ui.text_edit_singleline(&mut state.search_query)
            .on_hover_text("Filtrer les fichiers du répertoire courant");
    });
    ui.separator();

    // ── Contenu du répertoire ─────────────────────────────────────────────────
    if state.loading {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Chargement…");
        });
        return;
    }

    let query = state.search_query.to_lowercase();
    let mut go_into: Option<String> = None;

    egui::ScrollArea::vertical()
        .id_salt("sftp_scroll")
        .show(ui, |ui| {
            for entry in state.entries.iter().filter(|e| {
                query.is_empty() || e.name.to_lowercase().contains(&query)
            }) {
                let icon = if entry.is_dir { "📁" } else { "📄" };
                let selected = state.selected.as_deref() == Some(&entry.path);

                let resp = ui
                    .selectable_label(selected, format!("{icon} {}", entry.name))
                    .on_hover_text(format!(
                        "Taille : {} octets\nPermissions : {:o}\nChemin : {}",
                        entry.size,
                        entry.permissions.unwrap_or(0),
                        entry.path
                    ));

                // Clic simple → sélection ; double-clic → ouvre le dossier.
                if resp.clicked() {
                    state.selected = Some(entry.path.clone());
                }
                if resp.double_clicked() && entry.is_dir {
                    go_into = Some(entry.path.clone());
                }

                // Menu contextuel (clic droit).
                resp.context_menu(|ui| {
                    if entry.is_dir {
                        if ui.button("📂 Ouvrir").clicked() {
                            go_into = Some(entry.path.clone());
                            ui.close_menu();
                        }
                    } else {
                        if ui.button("⬇ Télécharger").clicked() {
                            // TODO: déclencher le téléchargement SFTP
                            ui.close_menu();
                        }
                    }
                    ui.separator();
                    if ui.button("✏ Renommer").clicked()    { ui.close_menu(); }
                    if ui.button("🗑 Supprimer").clicked()  { ui.close_menu(); }
                    if ui.button("📋 Copier le chemin").clicked() {
                        ui.output_mut(|o| o.copied_text = entry.path.clone());
                        ui.close_menu();
                    }
                });
            }
        });

    // Navigation dans le sous-dossier (hors boucle pour éviter le borrow double).
    if let Some(path) = go_into {
        state.current_path = path;
        state.loading = true;
    }

    // ── Boutons d'action ──────────────────────────────────────────────────────
    ui.separator();
    ui.horizontal(|ui| {
        if ui.button("⬆ Upload…").on_hover_text("Envoyer un fichier local vers ce dossier").clicked() {
            // TODO: rfd::AsyncFileDialog + upload SFTP
        }
        if ui.button("📁 Nouveau dossier").clicked() {
            // TODO: demander le nom puis créer via SFTP mkdir
        }
    });
}

/// Affiche la barre de navigation par segments (breadcrumb) ou un champ d'édition.
fn render_breadcrumb(state: &mut FileExplorerState, ui: &mut Ui) {
    ui.horizontal(|ui| {
        if state.breadcrumb_edit {
            // Mode édition : l'utilisateur tape un chemin manuellement.
            let resp = ui.text_edit_singleline(&mut state.breadcrumb_input);
            if resp.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                state.current_path = state.breadcrumb_input.clone();
                state.breadcrumb_edit = false;
                state.loading = true;
            }
        } else {
            // Mode lecture : segments cliquables du chemin courant.
            // On collecte les parties en String pour éviter de garder un borrow sur current_path.
            let parts: Vec<String> = state
                .current_path
                .split('/')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();

            let mut new_path: Option<String> = None;

            // Segment racine "/"
            if ui.small_button("/").clicked() {
                new_path = Some("/".into());
            }

            // Segments intermédiaires (chacun reconstruit le chemin absolu jusqu'à lui).
            for (i, part) in parts.iter().enumerate() {
                ui.label("›");
                let path = "/".to_string() + &parts[..=i].join("/");
                if ui.small_button(part.as_str()).clicked() {
                    new_path = Some(path);
                }
            }

            // Bouton crayon pour passer en mode édition directe.
            if ui.small_button("✏").on_hover_text("Éditer le chemin").clicked() {
                state.breadcrumb_input = state.current_path.clone();
                state.breadcrumb_edit = true;
            }

            // Applique la navigation si un segment a été cliqué.
            if let Some(p) = new_path {
                state.current_path = p;
                state.loading = true;
            }
        }
    });
}
