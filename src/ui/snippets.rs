/// Gestionnaire de snippets / macros de commandes.
/// Permet de sauvegarder des commandes fréquentes avec nom, description et tags.
/// Les snippets épinglés apparaissent en tête de liste.
use crate::i18n::Lang;
use crate::ui::icons as ph;
use egui::Ui;
use serde::{Deserialize, Serialize};

// ─── Modèle de données ────────────────────────────────────────────────────────

/// Une commande sauvegardée avec ses métadonnées.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snippet {
    /// Identifiant unique (hex aléatoire).
    pub id: String,
    /// Nom court affiché dans la liste.
    pub name: String,
    /// Description optionnelle (aide-mémoire).
    pub description: String,
    /// La commande à envoyer au terminal (peut contenir des variables ${…}).
    pub command: String,
    /// Étiquettes pour filtrer / grouper.
    pub tags: Vec<String>,
    /// true = affiché en haut de liste quelle que soit la recherche.
    pub pinned: bool,
}

impl Snippet {
    fn new(name: &str, command: &str) -> Self {
        Self {
            id: uuid_simple(),
            name: name.into(),
            description: String::new(),
            command: command.into(),
            tags: Vec::new(),
            pinned: false,
        }
    }
}

// ─── État du gestionnaire ─────────────────────────────────────────────────────

pub struct SnippetsState {
    /// Bibliothèque complète de snippets.
    pub snippets: Vec<Snippet>,
    /// Filtre texte sur le nom et la commande.
    pub search: String,
    /// Snippet en cours d'édition (clone temporaire).
    pub editing: Option<Snippet>,
    /// true = fenêtre d'édition ouverte.
    pub show_editor: bool,
}

impl SnippetsState {
    /// Initialise avec quelques snippets utiles par défaut, nommés dans la langue active.
    pub fn new(lang: &Lang) -> Self {
        Self {
            snippets: vec![
                Snippet::new(&lang.snip_default_ls,     "ls -lah"),
                Snippet::new(&lang.snip_default_df,     "df -h"),
                Snippet::new(&lang.snip_default_free,   "free -h"),
                Snippet::new(&lang.snip_default_ps,     "ps aux --sort=-%cpu | head -20"),
                Snippet::new(&lang.snip_default_syslog, "tail -f /var/log/syslog"),
                Snippet::new(&lang.snip_default_ports,  "ss -tlnp"),
                Snippet::new(&lang.snip_default_load,   "uptime && cat /proc/loadavg"),
                Snippet::new(&lang.snip_default_net,    "ip addr show"),
            ],
            search: String::new(),
            editing: None,
            show_editor: false,
        }
    }
}

// ─── Rendu egui ──────────────────────────────────────────────────────────────

pub fn render(state: &mut SnippetsState, ui: &mut Ui, lang: &Lang) {
    // ── En-tête ───────────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.heading(&lang.shortcut_snippets);
        ui.label(
            egui::RichText::new(Lang::fmt_n(&lang.snip_count, state.snippets.len()))
                .small().weak(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(&lang.snip_new).clicked() {
                state.editing = Some(Snippet::new("", ""));
                state.show_editor = true;
            }
        });
    });
    ui.separator();

    // ── Barre de recherche ────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(ph::rich(ph::MAGNIFYING_GLASS, ph::SIZE));
        ui.text_edit_singleline(&mut state.search)
            .on_hover_text(&lang.snip_filter_hint);
    });
    ui.add_space(4.0);

    let query = state.search.to_lowercase();
    let mut to_edit:   Option<usize> = None;
    let mut to_delete: Option<usize> = None;
    let mut to_pin:    Option<usize> = None;

    // ── Liste des snippets ────────────────────────────────────────────────────
    egui::ScrollArea::vertical()
        .id_salt("snippets_scroll")
        .show(ui, |ui| {
            // Filtre + tri : épinglés d'abord, puis alphabétique.
            let mut indices: Vec<usize> = (0..state.snippets.len())
                .filter(|&i| {
                    let s = &state.snippets[i];
                    query.is_empty()
                        || s.name.to_lowercase().contains(&query)
                        || s.command.to_lowercase().contains(&query)
                        || s.description.to_lowercase().contains(&query)
                        || s.tags.iter().any(|t| t.to_lowercase().contains(&query))
                })
                .collect();
            indices.sort_by(|&a, &b| {
                // Épinglés d'abord, puis par nom.
                state.snippets[b].pinned.cmp(&state.snippets[a].pinned)
                    .then(state.snippets[a].name.cmp(&state.snippets[b].name))
            });

            for i in indices {
                let s = state.snippets[i].clone();

                egui::Frame::new()
                    .stroke(egui::Stroke::new(1.0, ui.visuals().window_stroke.color))
                    .inner_margin(egui::Margin::same(6))
                    .outer_margin(egui::Margin::symmetric(0, 2))
                    .show(ui, |ui| {
                        // ── En-tête du snippet ────────────────────────────────
                        ui.horizontal(|ui| {
                            if s.pinned {
                                ui.label(ph::rich(ph::PUSH_PIN, 12.0));
                            }
                            ui.strong(&s.name);

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button(ph::rich(ph::TRASH, 14.0)).on_hover_text(&lang.btn_delete).clicked() {
                                    to_delete = Some(i);
                                }
                                if ui.small_button(ph::rich(ph::PENCIL_SIMPLE, 14.0)).on_hover_text(&lang.sidebar_edit_hint).clicked() {
                                    to_edit = Some(i);
                                }
                                let pin_label = if s.pinned { &lang.snip_unpin } else { &lang.snip_pin };
                                if ui.small_button(pin_label).clicked() {
                                    to_pin = Some(i);
                                }
                            });
                        });

                        // ── Commande en monospace ─────────────────────────────
                        ui.label(
                            egui::RichText::new(&s.command)
                                .monospace()
                                .color(ui.visuals().code_bg_color.linear_multiply(5.0)),
                        );

                        // Description si présente.
                        if !s.description.is_empty() {
                            ui.label(egui::RichText::new(&s.description).small().weak());
                        }

                        // Tags.
                        if !s.tags.is_empty() {
                            ui.horizontal(|ui| {
                                for tag in &s.tags {
                                    ui.label(egui::RichText::new(format!("#{tag}")).small().weak());
                                }
                            });
                        }

                        // Bouton d'exécution.
                        if ui.button(&lang.snip_send).clicked() {
                            // TODO: envoyer `s.command` vers la session SSH active
                            log::debug!("Snippet sélectionné : {}", s.command);
                        }
                    });
            }
        });

    // ── Application des actions ───────────────────────────────────────────────
    if let Some(i) = to_edit {
        state.editing = Some(state.snippets[i].clone());
        state.show_editor = true;
    }
    if let Some(i) = to_delete {
        state.snippets.remove(i);
    }
    if let Some(i) = to_pin {
        state.snippets[i].pinned = !state.snippets[i].pinned;
    }

    // ── Fenêtre d'édition ─────────────────────────────────────────────────────
    if state.show_editor {
        render_editor(state, ui.ctx(), lang);
    }
}

/// Fenêtre modale de création/édition d'un snippet.
/// Travaille sur un clone de `state.editing` pour éviter les double-borrows.
fn render_editor(state: &mut SnippetsState, ctx: &egui::Context, lang: &Lang) {
    let mut snippet = state.editing.clone().unwrap_or_else(|| Snippet::new("", ""));
    let mut save   = false;
    let mut cancel = false;

    let title = if snippet.name.is_empty() { &lang.snip_new_title } else { &lang.snip_edit_title };

    egui::Window::new(title)
        .default_size([420.0, 340.0])
        .collapsible(false)
        .show(ctx, |ui| {
            egui::Grid::new("snippet_edit_grid")
                .num_columns(2)
                .spacing([8.0, 8.0])
                .show(ui, |ui| {
                    ui.label(&lang.dlg_field_name);
                    ui.text_edit_singleline(&mut snippet.name);
                    ui.end_row();

                    ui.label(&lang.snip_command);
                    ui.add(
                        egui::TextEdit::multiline(&mut snippet.command)
                            .font(egui::TextStyle::Monospace)
                            .desired_rows(3)
                            .hint_text(&lang.snip_command_hint),
                    );
                    ui.end_row();

                    ui.label(&lang.about_description);
                    ui.text_edit_singleline(&mut snippet.description);
                    ui.end_row();

                    ui.label(&lang.dlg_field_tags);
                    let mut tags_str = snippet.tags.join(", ");
                    if ui.text_edit_singleline(&mut tags_str)
                        .on_hover_text(&lang.snip_tags_hint)
                        .changed()
                    {
                        snippet.tags = tags_str
                            .split(',')
                            .map(|t| t.trim().to_string())
                            .filter(|t| !t.is_empty())
                            .collect();
                    }
                    ui.end_row();

                    ui.label(&lang.snip_pin_label);
                    ui.checkbox(&mut snippet.pinned, &lang.snip_pin_check);
                    ui.end_row();
                });

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(&lang.dlg_save_btn).clicked() { save = true; }
                if ui.button(&lang.dlg_cancel_btn).clicked() { cancel = true; }
            });
        });

    // Réintègre les modifications dans l'état.
    state.editing = Some(snippet.clone());

    if save {
        let s = snippet;
        match state.snippets.iter().position(|x| x.id == s.id) {
            Some(i) => state.snippets[i] = s,
            None    => state.snippets.push(s),
        }
        state.editing    = None;
        state.show_editor = false;
    } else if cancel {
        state.editing    = None;
        state.show_editor = false;
    }
}

// ─── Utilitaires ──────────────────────────────────────────────────────────────

/// Génère un ID unique court (16 caractères hex).
fn uuid_simple() -> String {
    use rand::Rng;
    format!("{:016x}", rand::thread_rng().gen::<u64>())
}
