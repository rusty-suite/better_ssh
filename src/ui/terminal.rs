/// Widget terminal interactif.
/// Parse les séquences ANSI/VT100 et affiche le texte avec les couleurs correspondantes.
/// La saisie est gérée par un champ de texte egui en bas du panneau.
use egui::{Color32, FontId, Key, Modifiers, ScrollArea, Ui};

// ─── Constantes ───────────────────────────────────────────────────────────────

const MAX_LINES: usize = 10_000;

pub const FONT_PRESETS: &[(&str, f32)] = &[
    ("Minuscule",    9.0),
    ("Petite",      11.0),
    ("Normale",     13.0),
    ("Confortable", 15.0),
    ("Grande",      18.0),
    ("Énorme",      22.0),
];

// ─── Structures de rendu ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TermLine {
    pub spans: Vec<TermSpan>,
}

#[derive(Debug, Clone)]
pub struct TermSpan {
    pub text: String,
    pub fg: Color32,
    pub bg: Option<Color32>,
    pub bold: bool,
}

impl Default for TermSpan {
    fn default() -> Self {
        Self {
            text: String::new(),
            fg: Color32::from_rgb(204, 204, 204),
            bg: None,
            bold: false,
        }
    }
}

// ─── Transfert de fichier par glisser-déposer ─────────────────────────────────

#[derive(Clone)]
pub struct PendingUpload {
    pub filename: String,
    pub content: Vec<u8>,
    pub remote_path: String,
}

// ─── État du terminal ─────────────────────────────────────────────────────────

pub struct TerminalState {
    pub lines: Vec<TermLine>,
    pub input: String,
    pub font_size: f32,
    pub scroll_to_bottom: bool,
    pub show_history_search: bool,
    pub history_search_query: String,
    /// Dernier texte copié depuis la sortie du terminal (sélection souris + Ctrl+C).
    pub selected_text: String,
    /// Contenu du presse-papiers système (mis à jour au clic droit pour le menu "Coller").
    pub clipboard_mirror: String,
    /// Fichier glissé-déposé en attente de confirmation.
    pub dropped_file: Option<PendingUpload>,
    /// Upload confirmé, prêt à être envoyé par l'appelant.
    pub upload_confirmed: Option<PendingUpload>,

    ansi_buf: Vec<u8>,
    current_fg: Color32,
    current_bg: Option<Color32>,
    current_bold: bool,
    current_line: Vec<TermSpan>,
}

impl TerminalState {
    pub fn new() -> Self {
        Self {
            lines: vec![TermLine { spans: vec![] }],
            input: String::new(),
            font_size: 13.0,
            scroll_to_bottom: true,
            show_history_search: false,
            history_search_query: String::new(),
            selected_text: String::new(),
            clipboard_mirror: String::new(),
            dropped_file: None,
            upload_confirmed: None,
            ansi_buf: Vec::new(),
            current_fg: Color32::from_rgb(204, 204, 204),
            current_bg: None,
            current_bold: false,
            current_line: Vec::new(),
        }
    }

    pub fn feed(&mut self, data: &[u8]) {
        self.ansi_buf.extend_from_slice(data);
        self.process_buffer();
    }

    fn push_char(&mut self, ch: char) {
        if let Some(span) = self.current_line.last_mut() {
            if span.fg == self.current_fg
                && span.bg == self.current_bg
                && span.bold == self.current_bold
            {
                span.text.push(ch);
                return;
            }
        }
        self.current_line.push(TermSpan {
            text: ch.to_string(),
            fg: self.current_fg,
            bg: self.current_bg,
            bold: self.current_bold,
        });
    }

    fn newline(&mut self) {
        let spans = std::mem::take(&mut self.current_line);
        self.lines.push(TermLine { spans });
        if self.lines.len() > MAX_LINES {
            self.lines.remove(0);
        }
        self.scroll_to_bottom = true;
    }

    fn process_buffer(&mut self) {
        let mut i = 0;
        while i < self.ansi_buf.len() {
            let b = self.ansi_buf[i];
            match b {
                b'\r' => { i += 1; }
                b'\n' => { self.newline(); i += 1; }
                b'\x08' => {
                    if let Some(span) = self.current_line.last_mut() {
                        span.text.pop();
                    }
                    i += 1;
                }
                0x1b => {
                    if i + 1 >= self.ansi_buf.len() { break; }
                    match self.ansi_buf[i + 1] {
                        b'[' => {
                            let start = i + 2;
                            let mut end = start;
                            while end < self.ansi_buf.len()
                                && !self.ansi_buf[end].is_ascii_alphabetic()
                            {
                                end += 1;
                            }
                            if end >= self.ansi_buf.len() { break; }
                            let cmd = self.ansi_buf[end] as char;
                            let params_str = std::str::from_utf8(&self.ansi_buf[start..end])
                                .unwrap_or("")
                                .to_string();
                            if cmd == 'm' {
                                self.apply_sgr(&params_str);
                            }
                            i = end + 1;
                        }
                        _ => { i += 2; }
                    }
                }
                b'\t' => {
                    for _ in 0..4 { self.push_char(' '); }
                    i += 1;
                }
                0x20..=0x7E => {
                    self.push_char(b as char);
                    i += 1;
                }
                0xC0..=0xDF => {
                    if i + 2 > self.ansi_buf.len() { break; }
                    let s = std::str::from_utf8(&self.ansi_buf[i..i + 2])
                        .unwrap_or("\u{FFFD}").to_string();
                    for ch in s.chars() { self.push_char(ch); }
                    i += 2;
                }
                0xE0..=0xEF => {
                    if i + 3 > self.ansi_buf.len() { break; }
                    let s = std::str::from_utf8(&self.ansi_buf[i..i + 3])
                        .unwrap_or("\u{FFFD}").to_string();
                    for ch in s.chars() { self.push_char(ch); }
                    i += 3;
                }
                0xF0..=0xF7 => {
                    if i + 4 > self.ansi_buf.len() { break; }
                    let s = std::str::from_utf8(&self.ansi_buf[i..i + 4])
                        .unwrap_or("\u{FFFD}").to_string();
                    for ch in s.chars() { self.push_char(ch); }
                    i += 4;
                }
                _ => { i += 1; }
            }
        }
        self.ansi_buf.drain(..i);
    }

    fn apply_sgr(&mut self, params: &str) {
        let codes: Vec<u8> = params
            .split(';')
            .filter_map(|s| s.parse().ok())
            .collect();

        if codes.is_empty() {
            self.reset_attrs();
            return;
        }

        let mut j = 0;
        while j < codes.len() {
            match codes[j] {
                0  => self.reset_attrs(),
                1  => self.current_bold = true,
                22 => self.current_bold = false,
                30 => self.current_fg = Color32::from_rgb(0, 0, 0),
                31 => self.current_fg = Color32::from_rgb(205, 49, 49),
                32 => self.current_fg = Color32::from_rgb(13, 188, 121),
                33 => self.current_fg = Color32::from_rgb(229, 229, 16),
                34 => self.current_fg = Color32::from_rgb(36, 114, 200),
                35 => self.current_fg = Color32::from_rgb(188, 63, 188),
                36 => self.current_fg = Color32::from_rgb(17, 168, 205),
                37 => self.current_fg = Color32::from_rgb(229, 229, 229),
                39 => self.current_fg = Color32::from_rgb(204, 204, 204),
                90 => self.current_fg = Color32::from_rgb(102, 102, 102),
                91 => self.current_fg = Color32::from_rgb(241, 76, 76),
                92 => self.current_fg = Color32::from_rgb(35, 209, 139),
                93 => self.current_fg = Color32::from_rgb(245, 245, 67),
                94 => self.current_fg = Color32::from_rgb(59, 142, 234),
                95 => self.current_fg = Color32::from_rgb(214, 112, 214),
                96 => self.current_fg = Color32::from_rgb(41, 184, 219),
                97 => self.current_fg = Color32::from_rgb(255, 255, 255),
                38 if j + 4 < codes.len() && codes[j + 1] == 2 => {
                    self.current_fg =
                        Color32::from_rgb(codes[j + 2], codes[j + 3], codes[j + 4]);
                    j += 4;
                }
                _ => {}
            }
            j += 1;
        }
    }

    fn reset_attrs(&mut self) {
        self.current_fg = Color32::from_rgb(204, 204, 204);
        self.current_bg = None;
        self.current_bold = false;
    }
}

// ─── Rendu egui ──────────────────────────────────────────────────────────────

pub fn render(state: &mut TerminalState, ui: &mut Ui) -> Option<Vec<u8>> {
    let bg = Color32::from_rgb(20, 20, 30);
    let mut to_send: Option<Vec<u8>> = None;

    let input_id = egui::Id::new("terminal_input_field");
    let terminal_focused = ui.ctx().memory(|m| m.has_focus(input_id));

    // ── Ctrl+Scroll : zoom police ─────────────────────────────────────────────
    let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
    if ui.input(|i| i.modifiers.ctrl) && scroll_delta != 0.0 {
        state.font_size = (state.font_size + scroll_delta * 0.05).clamp(8.0, 32.0);
    }

    // ── Touches de contrôle (hors Ctrl+C, traité après les labels) ────────────
    // Ctrl+C est intentionnellement absent ici : il doit être évalué APRÈS
    // que les labels sélectionnables aient eu la chance de copier du texte.
    if terminal_focused {
        if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::R)) {
            state.show_history_search = !state.show_history_search;
        } else if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::D)) {
            to_send = Some(vec![0x04]);
        } else if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Z)) {
            to_send = Some(vec![0x1a]);
        } else if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::L)) {
            to_send = Some(vec![0x0c]);
        } else if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::U)) {
            state.input.clear();
            to_send = Some(vec![0x15]);
        } else if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp)) {
            to_send = Some(b"\x1b[A".to_vec());
        } else if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowDown)) {
            to_send = Some(b"\x1b[B".to_vec());
        } else if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab)) {
            to_send = Some(vec![0x09]);
        }
    }

    // ── Glisser-déposer ───────────────────────────────────────────────────────
    if state.dropped_file.is_none() {
        let dropped = ui.ctx().input(|i| i.raw.dropped_files.clone());
        if let Some(file) = dropped.into_iter().next() {
            let content_opt: Option<Vec<u8>> = if let Some(bytes) = &file.bytes {
                Some(bytes.to_vec())
            } else if let Some(path) = &file.path {
                std::fs::read(path).ok()
            } else {
                None
            };

            if let Some(content) = content_opt {
                let filename = file
                    .path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| {
                        if file.name.is_empty() { "fichier".into() } else { file.name.clone() }
                    });
                state.dropped_file = Some(PendingUpload {
                    remote_path: format!("/tmp/{}", filename),
                    filename,
                    content,
                });
            }
        }
    }

    // ── Lecture du presse-papiers au clic droit (pour le menu contextuel) ─────
    // N'est exécutée qu'une fois par clic droit, pas à chaque frame.
    if ui.input(|i| i.pointer.secondary_pressed()) {
        if let Ok(mut cb) = arboard::Clipboard::new() {
            state.clipboard_mirror = cb.get_text().unwrap_or_default();
        }
    }

    let font_id = FontId::monospace(state.font_size);

    let frame_resp = egui::Frame::none()
        .fill(bg)
        .inner_margin(egui::Margin::same(6.0))
        .show(ui, |ui| {
            // ── Barre de recherche historique ────────────────────────────────
            if state.show_history_search {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("(reverse-i-search):").color(Color32::YELLOW));
                    ui.text_edit_singleline(&mut state.history_search_query);
                    if ui.small_button("X").clicked() {
                        state.show_history_search = false;
                        state.history_search_query.clear();
                    }
                });
            }

            let available = ui.available_size();

            // ── Zone de défilement : sortie du terminal ───────────────────────
            // Chaque ligne est un Label unique (LayoutJob multi-couleur) afin que
            // la sélection souris couvre toute la ligne, pas seulement un span.
            ScrollArea::vertical()
                .auto_shrink([false; 2])
                .stick_to_bottom(state.scroll_to_bottom)
                .show(ui, |ui| {
                    ui.set_min_size(available);

                    for line in &state.lines {
                        if line.spans.is_empty() {
                            // Ligne vide : espace pour maintenir la hauteur.
                            ui.label(egui::RichText::new(" ").font(font_id.clone()));
                        } else {
                            ui.add(
                                egui::Label::new(line_to_job(line, &font_id))
                                    .selectable(true),
                            );
                        }
                    }

                    // Ligne en cours de saisie côté serveur (non terminée par \n).
                    if !state.current_line.is_empty() {
                        let partial = TermLine { spans: state.current_line.clone() };
                        ui.add(
                            egui::Label::new(line_to_job(&partial, &font_id))
                                .selectable(true),
                        );
                    }
                });

            // ── Ligne de saisie ───────────────────────────────────────────────
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("> ")
                        .color(Color32::from_rgb(100, 220, 100))
                        .font(font_id.clone()),
                );
                let response = ui.add(
                    egui::TextEdit::singleline(&mut state.input)
                        .id(input_id)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .frame(false)
                        .text_color(Color32::WHITE),
                );

                // PIÈGE – vol de focus : ne PAS appeler request_focus()
                // inconditionnellement.
                //
                // Ordre de rendu dans ui/mod.rs :
                //   render_sidebar()      ← dialogue de profil (egui::Window) rendu ici
                //   render_main_area()    ← terminal rendu ici  (APRÈS la sidebar)
                //
                // Si request_focus() est appelé sans condition, le terminal écrase
                // chaque frame le focus que le dialogue vient d'obtenir, rendant
                // les champs du formulaire (nom, hôte, port…) insaisissables.
                //
                // Règle publique (Memory::focus() est privé en egui 0.29) :
                //   wants_keyboard_input() == true  ET  terminal_focused == false
                //     → un AUTRE widget a le focus → ne pas voler
                //   wants_keyboard_input() == false
                //     → personne n'a le focus → demander le focus (auto-focus)
                //
                // NOTE FUTURE : cette garde est générique ; l'ajout de nouveaux
                // dialogues (Telnet, scan…) ne nécessite aucune modification ici.
                let other_widget_focused =
                    ui.ctx().wants_keyboard_input() && !terminal_focused;
                if !other_widget_focused && !response.has_focus() {
                    response.request_focus();
                }

                // Entrée → envoie la commande si aucun raccourci n'a déjà produit des octets.
                if to_send.is_none()
                    && response.has_focus()
                    && ui.input(|i| i.key_pressed(Key::Enter))
                {
                    let mut cmd = state.input.clone();
                    cmd.push('\n');
                    to_send = Some(cmd.into_bytes());
                    state.input.clear();
                }
            });
        });

    // ── Ctrl+C : copie ou SIGINT ──────────────────────────────────────────────
    // Vérifié APRÈS le rendu des labels pour laisser egui traiter la copie en premier.
    // Si egui a copié du texte cette frame (via la sélection label), on mémorise le
    // texte et on n'envoie PAS SIGINT. Sinon, on envoie SIGINT normalement.
    let just_copied = ui.ctx().output(|o| o.copied_text.clone());
    if !just_copied.is_empty() {
        state.selected_text = just_copied.clone();
    }
    if to_send.is_none() && terminal_focused {
        if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::C)) {
            if just_copied.is_empty() {
                // Rien n'a été sélectionné/copié → interrompre le processus.
                state.input.clear();
                to_send = Some(vec![0x03]);
            }
            // Sinon : la copie a déjà été gérée par le label, rien à faire.
        }
    }

    // ── Menu contextuel clic droit ────────────────────────────────────────────
    // Variables d'action : remplies dans la closure, appliquées après.
    let mut do_copy  = false;
    let mut do_paste = false;
    let selected_snapshot  = state.selected_text.clone();
    let clipboard_snapshot = state.clipboard_mirror.clone();

    frame_resp.response.context_menu(|ui| {
        // "Copier" : visible si du texte a déjà été sélectionné.
        if !selected_snapshot.is_empty() {
            if ui.button("📋 Copier la sélection").clicked() {
                do_copy = true;
                ui.close_menu();
            }
        } else {
            ui.add_enabled(false, egui::Button::new("📋 Copier la sélection"))
                .on_disabled_hover_text("Sélectionnez du texte avec la souris d'abord");
        }

        ui.separator();

        // "Coller" : visible si le presse-papiers contient du texte.
        if !clipboard_snapshot.is_empty() {
            let preview: String = clipboard_snapshot.chars().take(40).collect();
            let label = if clipboard_snapshot.len() > 40 {
                format!("📋 Coller  « {}… »", preview)
            } else {
                format!("📋 Coller  « {} »", preview)
            };
            if ui.button(label).clicked() {
                do_paste = true;
                ui.close_menu();
            }
        } else {
            ui.add_enabled(false, egui::Button::new("📋 Coller"))
                .on_disabled_hover_text("Presse-papiers vide");
        }
    });

    // Application des actions hors closure (pour éviter les conflits de borrow sur state).
    if do_copy {
        ui.ctx().output_mut(|o| o.copied_text = state.selected_text.clone());
    }
    if do_paste {
        state.input.push_str(&state.clipboard_mirror);
    }

    // ── Popup de confirmation de transfert de fichier ─────────────────────────
    if let Some(pending) = &mut state.dropped_file {
        let mut confirmed = false;
        let mut cancelled = false;

        egui::Window::new("📤 Transfert de fichier")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                egui::Grid::new("upload_grid")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        ui.strong("Fichier :");
                        ui.label(&pending.filename);
                        ui.end_row();

                        ui.strong("Taille :");
                        ui.label(format_size(pending.content.len()));
                        ui.end_row();

                        ui.strong("Destination :");
                        ui.text_edit_singleline(&mut pending.remote_path);
                        ui.end_row();
                    });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    if ui.button("✅ Transférer").clicked() { confirmed = true; }
                    if ui.button("Annuler").clicked()         { cancelled = true; }
                });
            });

        if confirmed {
            state.upload_confirmed = state.dropped_file.take();
        } else if cancelled {
            state.dropped_file = None;
        }
    }

    to_send
}

// ─── Utilitaires ──────────────────────────────────────────────────────────────

/// Convertit une TermLine (multi-spans colorés) en LayoutJob egui.
/// Un seul widget Label par ligne → sélection souris couvre toute la ligne.
fn line_to_job(line: &TermLine, font_id: &FontId) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    for span in &line.spans {
        let mut fmt = egui::TextFormat {
            font_id: font_id.clone(),
            color: span.fg,
            ..Default::default()
        };
        if span.bold {
            fmt.color = span.fg; // conserver la couleur même en gras
        }
        job.append(&span.text, 0.0, fmt);
    }
    job
}

fn format_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} o", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} Ko", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} Mo", bytes as f64 / (1024.0 * 1024.0))
    }
}
