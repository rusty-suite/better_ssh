/// Explorateur de fichiers SFTP à deux panneaux.
/// Panneau gauche : favoris / accès rapide.
/// Panneau droit  : barre d'outils, contenu (grille ou liste), barre de statut.
/// La permission d'accès de l'utilisateur courant est codée par couleur.
use crate::ssh::sftp::RemoteEntry;
use crate::ui::icons as ph;
use egui::{Color32, FontId, Key, Modifiers, Pos2, Rect, ScrollArea, Stroke, Ui, Vec2};
use std::collections::HashSet;
use std::time::{Duration, Instant};

// ─── Types publics ────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
pub enum ViewMode { Grid, List }

#[derive(Clone, PartialEq)]
pub enum ClipOp { Copy, Cut }

#[derive(Clone)]
pub struct ClipEntry {
    pub op: ClipOp,
    pub paths: Vec<String>,
}

/// Notification brève non-bloquante (toast).
pub struct Toast {
    pub msg: String,
    pub born: Instant,
}

/// Opération SFTP demandée par l'explorateur au parent (app.rs / mod.rs).
pub enum SftpRequest {
    ListDir(String),
    Rename { from: String, to: String },
    DeletePaths(Vec<String>),
    MovePaths { paths: Vec<String>, dest: String },
    Mkdir(String),
    CreateFile(String),
    Download { remote: String },
}

// ─── État ─────────────────────────────────────────────────────────────────────

pub struct FileExplorerState {
    // Navigation
    pub current_path: String,
    pub entries: Vec<RemoteEntry>,
    pub loading: bool,
    pub nav_back: Vec<String>,
    pub nav_forward: Vec<String>,
    pub breadcrumb_edit: bool,
    pub breadcrumb_input: String,
    // Sélection
    pub selected: HashSet<String>,
    pub last_active: Option<String>,
    pub lasso_start: Option<Pos2>,
    pub lasso_end: Option<Pos2>,
    // Drag & drop
    pub dnd_active: bool,
    pub dnd_hover_target: Option<String>,
    // Renommage inline
    pub rename_path: Option<String>,
    pub rename_buf: String,
    pub rename_request_focus: bool,
    // Presse-papiers
    pub clipboard: Option<ClipEntry>,
    // UI
    pub view_mode: ViewMode,
    pub search_query: String,
    // Toasts
    pub toasts: Vec<Toast>,
    // Favoris (label, chemin)
    pub favorites: Vec<(String, String)>,
    // Cache des rects d'items pour le lasso (rempli chaque frame)
    item_rects: Vec<(String, Rect)>,
    /// true dès qu'un premier listage SFTP a été reçu (évite la boucle de rechargement).
    pub loaded: bool,
    /// UID numérique de l'utilisateur connecté (reçu via SftpUid au démarrage SFTP).
    pub current_uid: Option<u32>,
    /// Erreur de listage du répertoire courant (accès refusé, etc.). None si aucun problème.
    pub dir_error: Option<String>,
}

impl FileExplorerState {
    pub fn new() -> Self {
        Self {
            current_path: "/".into(),
            entries: Vec::new(),
            loading: false,
            nav_back: Vec::new(),
            nav_forward: Vec::new(),
            breadcrumb_edit: false,
            breadcrumb_input: String::new(),
            selected: HashSet::new(),
            last_active: None,
            lasso_start: None,
            lasso_end: None,
            dnd_active: false,
            dnd_hover_target: None,
            rename_path: None,
            rename_buf: String::new(),
            rename_request_focus: false,
            clipboard: None,
            view_mode: ViewMode::List,
            search_query: String::new(),
            toasts: Vec::new(),
            favorites: vec![
                (format!("{} /root", ph::HOUSE),     "/root".into()),
                (format!("{} /home", ph::HOUSE),     "/home".into()),
                (format!("{} /etc", ph::GEAR),       "/etc".into()),
                (format!("{} /var/log", ph::CLIPBOARD), "/var/log".into()),
                (format!("{} /tmp", ph::FOLDERS),    "/tmp".into()),
                (format!("{} /opt", ph::PACKAGE),    "/opt".into()),
                (format!("{} /etc/ssh", ph::LOCK),   "/etc/ssh".into()),
                (format!("{} /var/www", ph::GLOBE),  "/var/www".into()),
            ],
            item_rects: Vec::new(),
            loaded: false,
            current_uid: None,
            dir_error: None,
        }
    }

    pub fn add_toast(&mut self, msg: impl Into<String>) {
        self.toasts.push(Toast { msg: msg.into(), born: Instant::now() });
    }

    /// Navigue vers un nouveau chemin (mémorise le chemin actuel dans l'historique).
    pub fn navigate_to(&mut self, path: String) -> SftpRequest {
        self.nav_back.push(self.current_path.clone());
        self.nav_forward.clear();
        self.current_path = path.clone();
        self.loading = true;
        self.dir_error = None;
        self.selected.clear();
        self.last_active = None;
        SftpRequest::ListDir(path)
    }

    pub fn navigate_back(&mut self) -> Option<SftpRequest> {
        let prev = self.nav_back.pop()?;
        self.nav_forward.push(self.current_path.clone());
        self.current_path = prev.clone();
        self.loading = true;
        self.dir_error = None;
        self.selected.clear();
        Some(SftpRequest::ListDir(prev))
    }

    pub fn navigate_forward(&mut self) -> Option<SftpRequest> {
        let next = self.nav_forward.pop()?;
        self.nav_back.push(self.current_path.clone());
        self.current_path = next.clone();
        self.loading = true;
        self.dir_error = None;
        self.selected.clear();
        Some(SftpRequest::ListDir(next))
    }

    pub fn navigate_up(&mut self) -> Option<SftpRequest> {
        let path = std::path::Path::new(&self.current_path);
        let parent = path.parent()?.to_string_lossy().into_owned();
        if parent.is_empty() { return None; }
        Some(self.navigate_to(parent))
    }
}

// ─── Helpers permissions ──────────────────────────────────────────────────────

/// Couleur d'accès selon les permissions Unix et l'utilisateur SSH courant.
/// Vert = accès complet, Jaune = lecture seule, Rouge = refusé, Gris = inconnu.
fn access_color(entry: &RemoteEntry, username: &str, current_uid: Option<u32>) -> Color32 {
    if username == "root" {
        return Color32::from_rgb(80, 210, 80);
    }
    let Some(perm) = entry.permissions else {
        return Color32::from_rgb(140, 140, 140);
    };
    let bits = if let (Some(ouid), Some(cuid)) = (entry.owner_uid, current_uid) {
        if ouid == cuid { (perm >> 6) & 0o7 } else { perm & 0o7 }
    } else {
        perm & 0o7
    };
    let can_r = bits & 0o4 != 0;
    let can_w = bits & 0o2 != 0;
    let can_x = bits & 0o1 != 0;
    if entry.is_dir {
        match (can_r, can_w, can_x) {
            (true, true, true)  => Color32::from_rgb(80,  210, 80),
            (true, _,    true)  => Color32::from_rgb(220, 200, 60),
            _                   => Color32::from_rgb(220, 80,  80),
        }
    } else {
        match (can_r, can_w) {
            (true, true)  => Color32::from_rgb(80,  210, 80),
            (true, false) => Color32::from_rgb(220, 200, 60),
            _             => Color32::from_rgb(220, 80,  80),
        }
    }
}

fn fmt_perms(p: u32) -> String {
    let b = |bits: u32| format!(
        "{}{}{}",
        if bits & 4 != 0 { 'r' } else { '-' },
        if bits & 2 != 0 { 'w' } else { '-' },
        if bits & 1 != 0 { 'x' } else { '-' },
    );
    format!("{}{}{}", b(p >> 6 & 7), b(p >> 3 & 7), b(p & 7))
}

fn fmt_size(s: u64) -> String {
    if s < 1024 { format!("{s} o") }
    else if s < 1_048_576 { format!("{:.1} Ko", s as f64 / 1024.0) }
    else if s < 1_073_741_824 { format!("{:.1} Mo", s as f64 / 1_048_576.0) }
    else { format!("{:.2} Go", s as f64 / 1_073_741_824.0) }
}

fn fmt_date(ts: u64) -> String {
    let days = ts / 86400;
    let y = 1970 + days / 365;
    let m = (days % 365) / 30 + 1;
    let d = (days % 365) % 30 + 1;
    format!("{y:04}-{m:02}-{d:02}")
}

fn access_label(
    entry: &RemoteEntry,
    username: &str,
    current_uid: Option<u32>,
    lang: &crate::i18n::Lang,
) -> String {
    if username == "root" { return lang.fe_access_root.clone(); }
    let perm = entry.permissions.unwrap_or(0);
    let bits = if let (Some(ouid), Some(cuid)) = (entry.owner_uid, current_uid) {
        if ouid == cuid { (perm >> 6) & 7 } else { perm & 7 }
    } else { perm & 7 };
    let mut parts: Vec<&str> = vec![];
    if bits & 4 != 0 { parts.push(&lang.fe_access_read_perm); }
    if bits & 2 != 0 { parts.push(&lang.fe_access_write_perm); }
    if bits & 1 != 0 {
        parts.push(if entry.is_dir { &lang.fe_access_trav_perm } else { &lang.fe_access_exec_perm });
    }
    if parts.is_empty() { lang.fe_access_denied_msg.clone() } else { parts.join(" + ") }
}

// ─── Raccourcis clavier ───────────────────────────────────────────────────────

fn handle_shortcuts(
    state: &mut FileExplorerState,
    ui: &mut Ui,
    req: &mut Option<SftpRequest>,
    lang: &crate::i18n::Lang,
) {
    if state.rename_path.is_some() { return; }
    if ui.ctx().memory(|m| m.focused().is_some()) { return; }
    ui.input_mut(|i| {
        if i.consume_key(Modifiers::NONE, Key::Delete) && !state.selected.is_empty() {
            let paths: Vec<String> = state.selected.iter().cloned().collect();
            state.add_toast(crate::i18n::Lang::fmt_n(&lang.fe_toast_deleted, paths.len()));
            state.selected.clear();
            *req = Some(SftpRequest::DeletePaths(paths));
        }
        if i.consume_key(Modifiers::NONE, Key::F2) {
            if let Some(active) = state.last_active.clone() {
                let name = state.entries.iter()
                    .find(|e| e.path == active)
                    .map(|e| e.name.clone()).unwrap_or_default();
                state.rename_path = Some(active);
                state.rename_buf = name;
                state.rename_request_focus = true;
            }
        }
        if i.consume_key(Modifiers::CTRL, Key::A) {
            state.selected = state.entries.iter().map(|e| e.path.clone()).collect();
        }
        if i.consume_key(Modifiers::CTRL, Key::C) && !state.selected.is_empty() {
            state.clipboard = Some(ClipEntry {
                op: ClipOp::Copy,
                paths: state.selected.iter().cloned().collect(),
            });
            state.add_toast(lang.fe_toast_copied.clone());
        }
        if i.consume_key(Modifiers::CTRL, Key::X) && !state.selected.is_empty() {
            state.clipboard = Some(ClipEntry {
                op: ClipOp::Cut,
                paths: state.selected.iter().cloned().collect(),
            });
            state.add_toast(lang.fe_toast_cut.clone());
        }
        if i.consume_key(Modifiers::CTRL, Key::V) {
            if let Some(clip) = state.clipboard.clone() {
                let dest = state.current_path.clone();
                if clip.op == ClipOp::Cut { state.clipboard = None; }
                state.add_toast(crate::i18n::Lang::fmt_n(&lang.fe_toast_pasted, clip.paths.len()));
                *req = Some(SftpRequest::MovePaths { paths: clip.paths, dest });
            }
        }
    });
}

// ─── Point d'entrée du rendu ──────────────────────────────────────────────────

/// `lang`        : traductions actives de l'interface.
/// `username`    : nom d'utilisateur SSH (pour les droits).
/// `current_uid` : UID numérique de l'utilisateur courant si connu.
/// Retourne `Some(SftpRequest)` si une opération SFTP doit être déclenchée.
pub fn render(
    state: &mut FileExplorerState,
    ui: &mut Ui,
    lang: &crate::i18n::Lang,
    username: &str,
    current_uid: Option<u32>,
) -> Option<SftpRequest> {
    let mut req: Option<SftpRequest> = None;

    state.toasts.retain(|t| t.born.elapsed() < Duration::from_secs(3));
    handle_shortcuts(state, ui, &mut req, lang);

    let total_h = ui.available_height();
    let total_w = ui.available_width();
    let sidebar_w = 148.0_f32.min(total_w * 0.28);

    ui.horizontal(|ui| {
        // Panneau latéral gauche
        ui.allocate_ui_with_layout(
            Vec2::new(sidebar_w, total_h),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| render_sidebar(state, ui, lang, &mut req),
        );

        ui.separator();

        // Zone principale
        ui.vertical(|ui| {
            render_toolbar(state, ui, lang, &mut req);
            ui.separator();

            let status_h = 22.0;
            let content_h = (ui.available_height() - status_h - 6.0).max(40.0);
            ui.allocate_ui(Vec2::new(ui.available_width(), content_h), |ui| {
                render_content(state, ui, lang, username, current_uid, &mut req);
            });
            ui.separator();
            render_status_bar(state, ui, lang, username);
        });
    });

    render_toasts(state, ui);
    req
}

// ─── Panneau latéral ─────────────────────────────────────────────────────────

fn render_sidebar(
    state: &mut FileExplorerState,
    ui: &mut Ui,
    lang: &crate::i18n::Lang,
    req: &mut Option<SftpRequest>,
) {
    ui.label(egui::RichText::new(&lang.fe_favorites).strong().small());
    ui.add_space(2.0);
    let favs: Vec<(String, String)> = state.favorites.clone();
    for (label, path) in &favs {
        let active = state.current_path == *path;
        let resp = ui.selectable_label(active, egui::RichText::new(label).small());
        if resp.clicked() { *req = Some(state.navigate_to(path.clone())); }
        resp.on_hover_text(path.as_str());
    }

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(4.0);
    ui.label(egui::RichText::new(&lang.fe_legend_title).small().weak());
    ui.add_space(2.0);
    for (color, label) in [
        (Color32::from_rgb(80, 210, 80),  lang.fe_legend_full.as_str()),
        (Color32::from_rgb(220, 200, 60), lang.fe_legend_read.as_str()),
        (Color32::from_rgb(220, 80,  80), lang.fe_legend_denied.as_str()),
        (Color32::from_rgb(140, 140, 140), lang.fe_legend_unknown.as_str()),
    ] {
        ui.horizontal(|ui| {
            ui.colored_label(color, "●");
            ui.label(egui::RichText::new(label).small().weak());
        });
    }
}

// ─── Barre d'outils ──────────────────────────────────────────────────────────

fn render_toolbar(
    state: &mut FileExplorerState,
    ui: &mut Ui,
    lang: &crate::i18n::Lang,
    req: &mut Option<SftpRequest>,
) {
    ui.horizontal(|ui| {
        let can_back    = !state.nav_back.is_empty();
        let can_forward = !state.nav_forward.is_empty();

        if ui.add_enabled(can_back, egui::Button::new(ph::ARROW_LEFT))
            .on_hover_text(&lang.fe_nav_prev).clicked()
        {
            if let Some(r) = state.navigate_back() { *req = Some(r); }
        }
        if ui.add_enabled(can_forward, egui::Button::new(ph::ARROW_RIGHT))
            .on_hover_text(&lang.fe_nav_next).clicked()
        {
            if let Some(r) = state.navigate_forward() { *req = Some(r); }
        }
        if ui.button(ph::ARROW_UP).on_hover_text(&lang.fe_nav_up).clicked() {
            if let Some(r) = state.navigate_up() { *req = Some(r); }
        }

        ui.separator();

        // Fil d'Ariane
        if state.breadcrumb_edit {
            let resp = ui.add_sized(
                [180.0, 20.0],
                egui::TextEdit::singleline(&mut state.breadcrumb_input).hint_text(&lang.fe_path_hint),
            );
            if resp.lost_focus() || ui.input(|i| i.key_pressed(Key::Enter)) {
                let p = state.breadcrumb_input.clone();
                *req = Some(state.navigate_to(p));
                state.breadcrumb_edit = false;
            }
            if ui.input(|i| i.key_pressed(Key::Escape)) {
                state.breadcrumb_edit = false;
            }
        } else {
            let parts: Vec<String> = state.current_path.split('/')
                .filter(|s| !s.is_empty()).map(str::to_string).collect();
            let mut go: Option<String> = None;
            if ui.small_button("/").clicked() { go = Some("/".into()); }
            for (i, part) in parts.iter().enumerate() {
                ui.label("›");
                if ui.small_button(part.as_str()).clicked() {
                    go = Some("/".to_string() + &parts[..=i].join("/"));
                }
            }
            if ui.small_button(ph::PENCIL).on_hover_text(&lang.fe_edit_path_hint).clicked() {
                state.breadcrumb_input = state.current_path.clone();
                state.breadcrumb_edit = true;
            }
            if let Some(p) = go { *req = Some(state.navigate_to(p)); }
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (vl, vl_tip) = if state.view_mode == ViewMode::Grid {
                (ph::LIST,      lang.fe_view_list.as_str())
            } else {
                (ph::GRID_FOUR, lang.fe_view_grid.as_str())
            };
            if ui.small_button(vl).on_hover_text(vl_tip).clicked() {
                state.view_mode = if state.view_mode == ViewMode::Grid { ViewMode::List } else { ViewMode::Grid };
            }
            if ui.small_button(ph::ARROWS_CLOCKWISE).on_hover_text(&lang.fe_refresh_hint).clicked() {
                state.loading = true;
                *req = Some(SftpRequest::ListDir(state.current_path.clone()));
            }
            if ui.small_button(format!("{}+", ph::FOLDER)).on_hover_text(&lang.fe_new_folder_hint).clicked() {
                let base = format!("{}/new_folder", state.current_path.trim_end_matches('/'));
                let path = unique_name(&state.entries, &base);
                let fname = filename_of(&path);
                state.rename_path = Some(path.clone());
                state.rename_buf = fname;
                state.rename_request_focus = true;
                *req = Some(SftpRequest::Mkdir(path));
            }
            if ui.small_button(format!("{}+", ph::FILE)).on_hover_text(&lang.fe_new_file_hint).clicked() {
                let base = format!("{}/new_file", state.current_path.trim_end_matches('/'));
                let path = unique_name(&state.entries, &base);
                let fname = filename_of(&path);
                state.rename_path = Some(path.clone());
                state.rename_buf = fname;
                state.rename_request_focus = true;
                *req = Some(SftpRequest::CreateFile(path));
            }
            ui.add(
                egui::TextEdit::singleline(&mut state.search_query)
                    .desired_width(110.0)
                    .hint_text(&lang.fe_search_hint),
            );
        });
    });
}

// ─── Contenu ─────────────────────────────────────────────────────────────────

fn render_content(
    state: &mut FileExplorerState,
    ui: &mut Ui,
    lang: &crate::i18n::Lang,
    username: &str,
    current_uid: Option<u32>,
    req: &mut Option<SftpRequest>,
) {
    if state.loading {
        ui.centered_and_justified(|ui| {
            ui.horizontal(|ui| { ui.spinner(); ui.label(&lang.fe_loading); });
        });
        return;
    }

    // Erreur de listage (accès refusé, répertoire inexistant, etc.)
    if let Some(err) = &state.dir_error.clone() {
        ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(24.0);
                ui.label(egui::RichText::new(ph::LOCK).size(40.0).color(Color32::from_rgb(200, 80, 80)));
                ui.add_space(8.0);
                ui.label(egui::RichText::new(err).color(Color32::from_rgb(220, 100, 100)).strong());
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(format!("{} {}", lang.fe_path_label, state.current_path))
                        .small().weak(),
                );
                ui.add_space(12.0);
                if ui.button(format!("{} {}", ph::ARROW_UP, lang.fe_nav_up)).clicked() {
                    *req = state.navigate_up();
                }
            });
        });
        return;
    }

    let filter = state.search_query.to_lowercase();
    let visible: Vec<RemoteEntry> = state.entries.iter()
        .filter(|e| filter.is_empty() || e.name.to_lowercase().contains(&filter))
        .cloned().collect();

    if visible.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new(
                if state.search_query.is_empty() { &lang.fe_empty } else { &lang.fe_no_results }
            ).weak());
        });
        return;
    }

    state.item_rects.clear();

    match state.view_mode {
        ViewMode::List => render_list(state, ui, lang, &visible, username, current_uid, req),
        ViewMode::Grid => render_grid(state, ui, lang, &visible, username, current_uid, req),
    }

    // Dessin du lasso
    if let (Some(start), Some(end)) = (state.lasso_start, state.lasso_end) {
        let r = Rect::from_two_pos(start, end);
        ui.painter().rect_stroke(r, 2.0, Stroke::new(1.0, Color32::from_rgb(100, 160, 255)));
        ui.painter().rect_filled(r, 2.0, Color32::from_rgba_unmultiplied(100, 160, 255, 22));
    }
}

// ─── Vue liste ────────────────────────────────────────────────────────────────

fn render_list(
    state: &mut FileExplorerState,
    ui: &mut Ui,
    lang: &crate::i18n::Lang,
    visible: &[RemoteEntry],
    username: &str,
    current_uid: Option<u32>,
    req: &mut Option<SftpRequest>,
) {
    // En-têtes
    egui::Grid::new("fe_hdr").num_columns(5).spacing([6.0, 2.0]).show(ui, |ui| {
        ui.label(egui::RichText::new("●").weak().small());
        ui.label(egui::RichText::new(&lang.fe_col_name).strong().small());
        ui.label(egui::RichText::new(&lang.fe_col_perms).strong().small());
        ui.label(egui::RichText::new(&lang.fe_col_size).strong().small());
        ui.label(egui::RichText::new(&lang.fe_col_modified).strong().small());
        ui.end_row();
    });
    ui.separator();

    ScrollArea::vertical().id_salt("fe_list").show(ui, |ui| {
        // Interaction de fond (lasso + menu ctx) sans avancer le curseur de layout.
        let bg_rect = ui.available_rect_before_wrap();
        let bg = ui.interact(bg_rect, ui.id().with("list_bg"), egui::Sense::click_and_drag());
        handle_bg(state, &bg, lang, visible, req);

        egui::Grid::new("fe_list_rows").num_columns(5).spacing([6.0, 2.0]).min_row_height(20.0)
            .show(ui, |ui| {
                for entry in visible {
                    let color  = access_color(entry, username, current_uid);
                    let is_sel = state.selected.contains(&entry.path);
                    let perm_s = entry.permissions.map(fmt_perms).unwrap_or_else(|| "?????????".into());

                    // Dot couleur
                    ui.label(egui::RichText::new("●").color(color).small());

                    // Nom ou champ de renommage
                    let icon = if entry.is_dir { ph::FOLDER } else { ph::FILE_TEXT };
                    if state.rename_path.as_deref() == Some(entry.path.as_str()) {
                        if let Some(new_name) = render_rename(state, ui, &entry.path) {
                            let to = format!("{}/{new_name}", state.current_path.trim_end_matches('/'));
                            state.add_toast(crate::i18n::Lang::fmt_name(&lang.fe_toast_renamed, &new_name));
                            *req = Some(SftpRequest::Rename { from: entry.path.clone(), to });
                        }
                        // Placeholders pour les colonnes restantes
                        ui.label(""); ui.label(""); ui.label("");
                    } else {
                        let label = format!("{icon} {}", entry.name);
                        let rt = egui::RichText::new(&label).small();
                        let rt = if is_sel { rt.strong() } else { rt };
                        let resp = ui.selectable_label(is_sel, rt);

                        let tip = format!(
                            "{}: {}\n{}: {}\n{} ({}): {}\n{}: {}\n{}: {}",
                            lang.fe_hover_path, entry.path,
                            lang.fe_hover_perms, perm_s,
                            lang.fe_hover_access, username,
                            access_label(entry, username, current_uid, lang),
                            lang.fe_hover_size,
                            if entry.is_dir { "—".into() } else { fmt_size(entry.size) },
                            lang.fe_hover_modified,
                            entry.modified.map(fmt_date).unwrap_or_else(|| "—".into()),
                        );
                        let item_rect = resp.rect;
                        state.item_rects.push((entry.path.clone(), item_rect));
                        handle_item(state, &entry.path, entry.is_dir, &resp, req);
                        let resp = resp.on_hover_text(tip);
                        resp.context_menu(|ui| ctx_menu(state, ui, lang, Some(&entry.path), entry.is_dir, req));

                        ui.label(egui::RichText::new(&perm_s).monospace().small().color(color));
                        let sz = if entry.is_dir { "—".into() } else { fmt_size(entry.size) };
                        ui.label(egui::RichText::new(&sz).small());
                        let dt = entry.modified.map(fmt_date).unwrap_or_else(|| "—".into());
                        ui.label(egui::RichText::new(&dt).small().weak());
                    }
                    ui.end_row();
                }
            });
    });
}

// ─── Vue grille ───────────────────────────────────────────────────────────────

/// Largeur d'une tuile incluant la marge interne du Frame (6px×2) et l'espacement inter-tuile.
const TILE_INNER_W: f32 = 90.0;
const TILE_INNER_H: f32 = 84.0;
const TILE_MARGIN:  f32 = 6.0;
const TILE_SPACING: f32 = 6.0;
/// Largeur totale occupée par une tuile dans la grille.
const TILE_TOTAL_W: f32 = TILE_INNER_W + TILE_MARGIN * 2.0 + TILE_SPACING;

fn render_grid(
    state: &mut FileExplorerState,
    ui: &mut Ui,
    lang: &crate::i18n::Lang,
    visible: &[RemoteEntry],
    username: &str,
    current_uid: Option<u32>,
    req: &mut Option<SftpRequest>,
) {
    // Calcule le nombre de colonnes depuis la largeur disponible *avant* le scroll.
    // On le capte ici pour qu'il soit stable même après ouverture de la barre de défilement.
    let available_w = ui.available_width();
    let cols = ((available_w / TILE_TOTAL_W) as usize).max(1);

    ScrollArea::vertical().id_salt("fe_grid").show(ui, |ui| {
        let bg_rect = ui.available_rect_before_wrap();
        let bg = ui.interact(bg_rect, ui.id().with("grid_bg"), egui::Sense::click_and_drag());
        handle_bg(state, &bg, lang, visible, req);

        egui::Grid::new("fe_grid_content")
            .num_columns(cols)
            .spacing([TILE_SPACING, TILE_SPACING])
            .show(ui, |ui| {
                for (idx, entry) in visible.iter().enumerate() {
                    let color  = access_color(entry, username, current_uid);
                    let is_sel = state.selected.contains(&entry.path);
                    let icon   = if entry.is_dir { ph::FOLDER } else { ph::FILE_TEXT };
                    let perm_s = entry.permissions.map(fmt_perms).unwrap_or_else(|| "?????????".into());

                    let bg_fill = if is_sel {
                        ui.visuals().selection.bg_fill.linear_multiply(0.6)
                    } else {
                        Color32::TRANSPARENT
                    };
                    let stroke_c = if is_sel {
                        ui.visuals().selection.bg_fill
                    } else {
                        Color32::from_rgba_unmultiplied(255, 255, 255, 12)
                    };

                    let tile = egui::Frame::none()
                        .fill(bg_fill)
                        .stroke(Stroke::new(1.0, stroke_c))
                        .inner_margin(egui::Margin::same(TILE_MARGIN))
                        .rounding(5.0)
                        .show(ui, |ui| {
                            ui.allocate_ui(Vec2::new(TILE_INNER_W, TILE_INNER_H), |ui| {
                                ui.vertical_centered(|ui| {
                                    ui.label(egui::RichText::new("●").color(color).small());
                                    ui.label(egui::RichText::new(icon).size(28.0));
                                    if state.rename_path.as_deref() == Some(entry.path.as_str()) {
                                        if let Some(new_name) = render_rename(state, ui, &entry.path) {
                                            let to = format!("{}/{new_name}", state.current_path.trim_end_matches('/'));
                                            state.add_toast(crate::i18n::Lang::fmt_name(&lang.fe_toast_renamed, &new_name));
                                            *req = Some(SftpRequest::Rename { from: entry.path.clone(), to });
                                        }
                                    } else {
                                        // Tronque les noms longs avec une ellipse.
                                        let short = truncate_name(&entry.name, 14);
                                        ui.label(egui::RichText::new(short).small());
                                    }
                                });
                            });
                        });

                    let resp = tile.response.interact(egui::Sense::click_and_drag());
                    state.item_rects.push((entry.path.clone(), resp.rect));

                    let tip = format!(
                        "{}\n{}: {}\n{} ({}): {}\n{}: {}\n{}: {}",
                        entry.path,
                        lang.fe_hover_perms, perm_s,
                        lang.fe_hover_access, username,
                        access_label(entry, username, current_uid, lang),
                        lang.fe_hover_size,
                        if entry.is_dir { "—".into() } else { fmt_size(entry.size) },
                        lang.fe_hover_modified,
                        entry.modified.map(fmt_date).unwrap_or_else(|| "—".into()),
                    );
                    handle_item(state, &entry.path, entry.is_dir, &resp, req);
                    let resp = resp.on_hover_text(tip);
                    resp.context_menu(|ui| ctx_menu(state, ui, lang, Some(&entry.path), entry.is_dir, req));

                    // Fin de ligne après chaque `cols` tuiles.
                    if (idx + 1) % cols == 0 {
                        ui.end_row();
                    }
                }
                // Termine la dernière ligne incomplète si nécessaire.
                if !visible.is_empty() && visible.len() % cols != 0 {
                    ui.end_row();
                }
            });
    });
}

/// Tronque un nom de fichier à `max_chars` caractères en ajoutant "…".
fn truncate_name(name: &str, max_chars: usize) -> String {
    let chars: Vec<char> = name.chars().collect();
    if chars.len() <= max_chars {
        name.to_string()
    } else {
        format!("{}…", chars[..max_chars - 1].iter().collect::<String>())
    }
}

// ─── Renommage inline ─────────────────────────────────────────────────────────

fn render_rename(state: &mut FileExplorerState, ui: &mut Ui, path: &str) -> Option<String> {
    let id = egui::Id::new(("rename", path));
    let resp = ui.add(
        egui::TextEdit::singleline(&mut state.rename_buf)
            .id(id).desired_width(120.0),
    );
    if state.rename_request_focus { resp.request_focus(); state.rename_request_focus = false; }
    let validated = resp.lost_focus() && ui.input(|i| !i.key_pressed(Key::Escape))
        || ui.input(|i| i.key_pressed(Key::Enter));
    let cancelled = ui.input(|i| i.key_pressed(Key::Escape));
    if cancelled { state.rename_path = None; return None; }
    if validated {
        let name = state.rename_buf.trim().to_string();
        state.rename_path = None;
        if !name.is_empty() { return Some(name); }
    }
    None
}

// ─── Menu contextuel ─────────────────────────────────────────────────────────

fn ctx_menu(
    state: &mut FileExplorerState,
    ui: &mut Ui,
    lang: &crate::i18n::Lang,
    path: Option<&str>,
    is_dir: bool,
    req: &mut Option<SftpRequest>,
) {
    if let Some(p) = path {
        if is_dir {
            if ui.button(&lang.fe_ctx_open).clicked() {
                *req = Some(state.navigate_to(p.to_string())); ui.close_menu();
            }
        } else if ui.button(&lang.fe_ctx_download).clicked() {
            *req = Some(SftpRequest::Download { remote: p.to_string() }); ui.close_menu();
        }
        if ui.button(&lang.fe_ctx_rename).clicked() {
            let name = state.entries.iter().find(|e| e.path == p)
                .map(|e| e.name.clone()).unwrap_or_default();
            state.rename_path = Some(p.to_string());
            state.rename_buf = name;
            state.rename_request_focus = true;
            ui.close_menu();
        }
        ui.separator();
        if ui.button(&lang.fe_ctx_copy).clicked() {
            state.clipboard = Some(ClipEntry { op: ClipOp::Copy, paths: vec![p.to_string()] });
            state.add_toast(lang.fe_toast_copied.clone()); ui.close_menu();
        }
        if ui.button(&lang.fe_ctx_cut).clicked() {
            state.clipboard = Some(ClipEntry { op: ClipOp::Cut, paths: vec![p.to_string()] });
            state.add_toast(lang.fe_toast_cut.clone()); ui.close_menu();
        }
        ui.separator();
        let n_sel = if state.selected.contains(p) { state.selected.len() } else { 1 };
        let del_label = crate::i18n::Lang::fmt_n(&lang.fe_ctx_delete, n_sel);
        if ui.add(egui::Button::new(
            egui::RichText::new(&del_label).color(egui::Color32::from_rgb(220, 70, 70))
        )).clicked() {
            let paths = if state.selected.contains(p) {
                state.selected.iter().cloned().collect()
            } else { vec![p.to_string()] };
            state.add_toast(crate::i18n::Lang::fmt_n(&lang.fe_toast_deleted, paths.len()));
            state.selected.clear();
            *req = Some(SftpRequest::DeletePaths(paths));
            ui.close_menu();
        }
        ui.separator();
        if ui.button(&lang.fe_ctx_copy_path).clicked() {
            ui.output_mut(|o| o.copied_text = p.to_string());
            state.add_toast(lang.fe_toast_path_copied.clone()); ui.close_menu();
        }
    } else {
        ui.label(egui::RichText::new(&lang.fe_ctx_create).strong().small());
        ui.separator();
        if ui.button(&lang.fe_ctx_new_folder).clicked() {
            let base = format!("{}/new_folder", state.current_path.trim_end_matches('/'));
            let p = unique_name(&state.entries, &base);
            state.rename_path = Some(p.clone());
            state.rename_buf = filename_of(&p);
            state.rename_request_focus = true;
            *req = Some(SftpRequest::Mkdir(p));
            ui.close_menu();
        }
        if ui.button(&lang.fe_ctx_new_file).clicked() {
            let base = format!("{}/new_file", state.current_path.trim_end_matches('/'));
            let p = unique_name(&state.entries, &base);
            state.rename_path = Some(p.clone());
            state.rename_buf = filename_of(&p);
            state.rename_request_focus = true;
            *req = Some(SftpRequest::CreateFile(p));
            ui.close_menu();
        }
    }

    if state.clipboard.is_some() {
        ui.separator();
        let lbl = match state.clipboard.as_ref().map(|c| &c.op) {
            Some(ClipOp::Copy) => lang.fe_ctx_paste_copy.as_str(),
            Some(ClipOp::Cut)  => lang.fe_ctx_paste_move.as_str(),
            None               => lang.fe_ctx_paste.as_str(),
        };
        if ui.button(lbl).clicked() {
            if let Some(clip) = state.clipboard.clone() {
                let dest = state.current_path.clone();
                let n = clip.paths.len();
                if clip.op == ClipOp::Cut { state.clipboard = None; }
                state.add_toast(crate::i18n::Lang::fmt_n(&lang.fe_toast_pasted, n));
                *req = Some(SftpRequest::MovePaths { paths: clip.paths, dest });
            }
            ui.close_menu();
        }
    }
}

// ─── Barre de statut ─────────────────────────────────────────────────────────

fn render_status_bar(
    state: &FileExplorerState,
    ui: &mut Ui,
    lang: &crate::i18n::Lang,
    username: &str,
) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(&state.current_path).small().weak().monospace());
        ui.separator();
        ui.label(egui::RichText::new(
            crate::i18n::Lang::fmt_n(&lang.fe_items, state.entries.len())
        ).small().weak());
        if !state.selected.is_empty() {
            ui.separator();
            ui.label(egui::RichText::new(
                crate::i18n::Lang::fmt_n(&lang.fe_selected, state.selected.len())
            ).small());
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(format!("👤 {username}")).small().weak());
        });
    });
}

// ─── Toasts ───────────────────────────────────────────────────────────────────

fn render_toasts(state: &FileExplorerState, ui: &mut Ui) {
    if state.toasts.is_empty() { return; }
    let rect = ui.clip_rect();
    let mut y = rect.bottom() - 32.0;
    for toast in state.toasts.iter().rev().take(3) {
        let age = toast.born.elapsed().as_secs_f32();
        let alpha = ((3.0 - age) / 0.5).clamp(0.0, 1.0);
        let bg_a  = (200.0 * alpha) as u8;
        let txt_a = (255.0 * alpha) as u8;
        let w = 250.0_f32;
        let toast_rect = Rect::from_min_size(
            egui::pos2(rect.right() - w - 8.0, y - 22.0),
            Vec2::new(w, 20.0),
        );
        ui.painter().rect_filled(toast_rect, 4.0, Color32::from_rgba_unmultiplied(30, 30, 50, bg_a));
        ui.painter().text(
            toast_rect.center(),
            egui::Align2::CENTER_CENTER,
            &toast.msg,
            FontId::proportional(11.0),
            Color32::from_rgba_unmultiplied(230, 230, 230, txt_a),
        );
        y -= 26.0;
    }
}

// ─── Interaction fond (lasso + menu) ─────────────────────────────────────────

fn handle_bg(
    state: &mut FileExplorerState,
    bg: &egui::Response,
    lang: &crate::i18n::Lang,
    visible: &[RemoteEntry],
    req: &mut Option<SftpRequest>,
) {
    if bg.drag_started() {
        if let Some(pos) = bg.interact_pointer_pos() {
            state.lasso_start = Some(pos);
            state.lasso_end   = Some(pos);
            state.selected.clear();
        }
    }
    if bg.dragged() {
        if let Some(pos) = bg.interact_pointer_pos() {
            state.lasso_end = Some(pos);
            if let (Some(s), Some(e)) = (state.lasso_start, state.lasso_end) {
                let lasso = Rect::from_two_pos(s, e);
                state.selected = state.item_rects.iter()
                    .filter(|(_, r)| r.intersects(lasso))
                    .map(|(p, _)| p.clone())
                    .collect();
            }
        }
    }
    if bg.drag_stopped() {
        state.lasso_start = None;
        state.lasso_end   = None;
    }
    if bg.clicked() {
        state.selected.clear();
        state.last_active = None;
    }
    bg.context_menu(|ui| ctx_menu(state, ui, lang, None, false, req));
}

// ─── Interaction item ─────────────────────────────────────────────────────────

fn handle_item(
    state: &mut FileExplorerState,
    path: &str,
    is_dir: bool,
    resp: &egui::Response,
    req: &mut Option<SftpRequest>,
) {
    if resp.double_clicked() {
        if is_dir {
            *req = Some(state.navigate_to(path.to_string()));
        }
        return;
    }
    if resp.clicked() {
        let ctrl  = resp.ctx.input(|i| i.modifiers.ctrl || i.modifiers.command);
        let shift = resp.ctx.input(|i| i.modifiers.shift);
        if ctrl {
            if state.selected.contains(path) { state.selected.remove(path); }
            else { state.selected.insert(path.to_string()); }
        } else if shift {
            if let Some(last) = state.last_active.clone() {
                let order: Vec<&str> = state.item_rects.iter().map(|(p, _)| p.as_str()).collect();
                let ia = order.iter().position(|p| *p == last.as_str());
                let ib = order.iter().position(|p| *p == path);
                if let (Some(a), Some(b)) = (ia, ib) {
                    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
                    for i in lo..=hi { state.selected.insert(order[i].to_string()); }
                }
            } else {
                state.selected.insert(path.to_string());
            }
        } else {
            state.selected.clear();
            state.selected.insert(path.to_string());
        }
        state.last_active = Some(path.to_string());
    }
}

// ─── Utilitaires ─────────────────────────────────────────────────────────────

fn unique_name(entries: &[RemoteEntry], base: &str) -> String {
    if !entries.iter().any(|e| e.path == base) { return base.to_string(); }
    for n in 2..=99 {
        let c = format!("{base} ({n})");
        if !entries.iter().any(|e| e.path == c) { return c; }
    }
    format!("{base} (99)")
}

fn filename_of(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}
