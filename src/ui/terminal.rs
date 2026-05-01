/// Widget terminal interactif.
/// Parse les séquences ANSI/VT100 et affiche le texte avec les couleurs correspondantes.
/// La saisie est gérée par un champ de texte egui en bas du panneau.
use egui::{Color32, FontId, Key, Modifiers, ScrollArea, Ui};

// ─── Constantes ───────────────────────────────────────────────────────────────

/// Nombre maximal de lignes conservées dans le scrollback.
const MAX_LINES: usize = 10_000;

/// Préréglages de taille de police avec leur label et leur taille en points.
pub const FONT_PRESETS: &[(&str, f32)] = &[
    ("Minuscule",   9.0),
    ("Petite",     11.0),
    ("Normale",    13.0),
    ("Confortable",15.0),
    ("Grande",     18.0),
    ("Énorme",     22.0),
];

// ─── Structures de rendu ──────────────────────────────────────────────────────

/// Une ligne de terminal composée de plusieurs segments colorés (spans).
#[derive(Debug, Clone)]
pub struct TermLine {
    pub spans: Vec<TermSpan>,
}

/// Un segment de texte avec ses attributs visuels SGR (couleur, gras).
#[derive(Debug, Clone)]
pub struct TermSpan {
    pub text: String,
    /// Couleur de premier plan (ANSI foreground).
    pub fg: Color32,
    /// Couleur de fond optionnelle (ANSI background).
    pub bg: Option<Color32>,
    /// true si l'attribut gras (SGR 1) est actif.
    pub bold: bool,
}

impl Default for TermSpan {
    fn default() -> Self {
        Self {
            text: String::new(),
            fg: Color32::from_rgb(204, 204, 204), // gris clair par défaut
            bg: None,
            bold: false,
        }
    }
}

// ─── Transfert de fichier par glisser-déposer ─────────────────────────────────

/// Fichier déposé sur le terminal en attente de confirmation.
#[derive(Clone)]
pub struct PendingUpload {
    pub filename: String,
    pub content: Vec<u8>,
    /// Chemin distant éditable dans la popup de confirmation.
    pub remote_path: String,
}

// ─── État du terminal ─────────────────────────────────────────────────────────

pub struct TerminalState {
    /// Lignes complètes (terminées par \n) dans le scrollback.
    pub lines: Vec<TermLine>,
    /// Texte saisi par l'utilisateur (non encore envoyé).
    pub input: String,
    /// Taille actuelle de la police en points.
    pub font_size: f32,
    /// true = le scroll suit automatiquement le bas (défilé par nouvelle sortie).
    pub scroll_to_bottom: bool,
    /// true = affiche la popup de recherche dans l'historique (Ctrl+R).
    pub show_history_search: bool,
    /// Requête de recherche tapée dans la popup.
    pub history_search_query: String,
    /// Fichier glissé-déposé en attente de confirmation de transfert.
    pub dropped_file: Option<PendingUpload>,
    /// Upload confirmé par l'utilisateur, prêt à être envoyé par l'appelant.
    pub upload_confirmed: Option<PendingUpload>,

    // ── État interne du parseur ANSI ────────────────────────────────────────
    /// Octets reçus mais pas encore parsés (séquences incomplètes).
    ansi_buf: Vec<u8>,
    /// Couleur de texte active (SGR foreground).
    current_fg: Color32,
    /// Couleur de fond active (SGR background), None si par défaut.
    current_bg: Option<Color32>,
    /// Attribut gras actif (SGR 1).
    current_bold: bool,
    /// Ligne en cours de construction (pas encore terminée par \n).
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
            dropped_file: None,
            upload_confirmed: None,
            ansi_buf: Vec::new(),
            current_fg: Color32::from_rgb(204, 204, 204),
            current_bg: None,
            current_bold: false,
            current_line: Vec::new(),
        }
    }

    /// Injecte des octets bruts reçus du canal SSH dans le parseur ANSI.
    pub fn feed(&mut self, data: &[u8]) {
        self.ansi_buf.extend_from_slice(data);
        self.process_buffer();
    }

    // ─── Parseur ANSI interne ─────────────────────────────────────────────────

    /// Ajoute un caractère à la ligne courante en fusionnant les spans de même couleur.
    fn push_char(&mut self, ch: char) {
        // Réutilise le dernier span si les attributs sont identiques (optimisation mémoire).
        if let Some(span) = self.current_line.last_mut() {
            if span.fg == self.current_fg
                && span.bg == self.current_bg
                && span.bold == self.current_bold
            {
                span.text.push(ch);
                return;
            }
        }
        // Sinon, crée un nouveau span avec les attributs courants.
        self.current_line.push(TermSpan {
            text: ch.to_string(),
            fg: self.current_fg,
            bg: self.current_bg,
            bold: self.current_bold,
        });
    }

    /// Valide la ligne courante et commence une nouvelle.
    fn newline(&mut self) {
        let spans = std::mem::take(&mut self.current_line);
        self.lines.push(TermLine { spans });
        // Purge les vieilles lignes pour éviter une consommation mémoire illimitée.
        if self.lines.len() > MAX_LINES {
            self.lines.remove(0);
        }
        self.scroll_to_bottom = true;
    }

    /// Boucle de parsing : traite tous les octets disponibles dans `ansi_buf`.
    fn process_buffer(&mut self) {
        let mut i = 0;
        while i < self.ansi_buf.len() {
            let b = self.ansi_buf[i];
            match b {
                b'\r' => { i += 1; } // retour chariot seul → ignoré
                b'\n' => { self.newline(); i += 1; }
                b'\x08' => {
                    // Backspace : supprime le dernier caractère du span courant.
                    if let Some(span) = self.current_line.last_mut() {
                        span.text.pop();
                    }
                    i += 1;
                }
                0x1b => {
                    // Séquence d'échappement ESC — attend au moins un octet de plus.
                    if i + 1 >= self.ansi_buf.len() { break; }
                    match self.ansi_buf[i + 1] {
                        b'[' => {
                            // CSI (Control Sequence Introducer) : ESC [ params cmd
                            let start = i + 2;
                            let mut end = start;
                            // Les paramètres CSI sont des chiffres et des ';'.
                            while end < self.ansi_buf.len()
                                && !self.ansi_buf[end].is_ascii_alphabetic()
                            {
                                end += 1;
                            }
                            if end >= self.ansi_buf.len() { break; } // séquence incomplète
                            let cmd = self.ansi_buf[end] as char;
                            let params_str = std::str::from_utf8(&self.ansi_buf[start..end])
                                .unwrap_or("")
                                .to_string();
                            if cmd == 'm' {
                                self.apply_sgr(&params_str);
                            }
                            i = end + 1;
                        }
                        _ => { i += 2; } // ESC + octet inconnu → ignore les deux
                    }
                }
                b'\t' => {
                    // Tabulation → 4 espaces (approximation simple)
                    for _ in 0..4 { self.push_char(' '); }
                    i += 1;
                }
                0x20..=0x7E => {
                    // Caractère ASCII imprimable (code point unique = octet unique).
                    self.push_char(b as char);
                    i += 1;
                }
                // ── Séquences UTF-8 multi-octets ─────────────────────────────
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
                _ => { i += 1; } // octet de continuation ou contrôle non géré → ignore
            }
        }
        // Conserve les octets non traités pour la prochaine frame.
        self.ansi_buf.drain(..i);
    }

    /// Applique les codes SGR (couleurs, attributs) à l'état courant du terminal.
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

    /// Remet les attributs visuels à leurs valeurs par défaut.
    fn reset_attrs(&mut self) {
        self.current_fg = Color32::from_rgb(204, 204, 204);
        self.current_bg = None;
        self.current_bold = false;
    }
}

// ─── Rendu egui ──────────────────────────────────────────────────────────────

/// Retourne les octets à envoyer au serveur si l'utilisateur a validé une saisie
/// ou appuyé sur un raccourci de contrôle. `None` sinon.
pub fn render(state: &mut TerminalState, ui: &mut Ui) -> Option<Vec<u8>> {
    let bg = Color32::from_rgb(20, 20, 30);
    let mut to_send: Option<Vec<u8>> = None;

    // ID stable utilisé pour détecter si le champ de saisie du terminal a le focus
    // (persisté d'une frame à l'autre dans la mémoire egui).
    let input_id = egui::Id::new("terminal_input_field");
    let terminal_focused = ui.ctx().memory(|m| m.has_focus(input_id));

    // ── Ctrl+Scroll : zoom police ─────────────────────────────────────────────
    let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
    if ui.input(|i| i.modifiers.ctrl) && scroll_delta != 0.0 {
        state.font_size = (state.font_size + scroll_delta * 0.05).clamp(8.0, 32.0);
    }

    // ── Touches de contrôle terminal (uniquement quand le terminal a le focus) ─
    // Traitées AVANT le TextEdit pour que celui-ci ne les consomme pas.
    if terminal_focused {
        // Ctrl+R → bascule la recherche historique
        if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::R)) {
            state.show_history_search = !state.show_history_search;
        }
        // Ctrl+C → SIGINT (tue le processus en cours)
        else if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::C)) {
            state.input.clear();
            to_send = Some(vec![0x03]);
        }
        // Ctrl+D → EOF (ferme le shell / fin de fichier)
        else if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::D)) {
            to_send = Some(vec![0x04]);
        }
        // Ctrl+Z → SIGTSTP (suspend le processus)
        else if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Z)) {
            to_send = Some(vec![0x1a]);
        }
        // Ctrl+L → efface l'écran (équivalent de `clear`)
        else if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::L)) {
            to_send = Some(vec![0x0c]);
        }
        // Ctrl+U → efface la ligne courante
        else if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::U)) {
            state.input.clear();
            to_send = Some(vec![0x15]);
        }
        // Flèche haut → historique précédent (ESC [ A)
        else if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp)) {
            to_send = Some(b"\x1b[A".to_vec());
        }
        // Flèche bas → historique suivant (ESC [ B)
        else if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowDown)) {
            to_send = Some(b"\x1b[B".to_vec());
        }
        // Tab → complétion automatique côté serveur
        else if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab)) {
            to_send = Some(vec![0x09]);
        }
    }

    // ── Glisser-déposer : détecte les fichiers déposés sur la fenêtre ──────────
    // Accepte les dépôts pendant que cet onglet est actif.
    if state.dropped_file.is_none() {
        let dropped = ui.ctx().input(|i| i.raw.dropped_files.clone());
        if let Some(file) = dropped.into_iter().next() {
            // Lit le contenu : soit fourni directement (navigateur), soit lu depuis le chemin.
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

    egui::Frame::none()
        .fill(bg)
        .inner_margin(egui::Margin::same(6.0))
        .show(ui, |ui| {
            // ── Barre de recherche historique ────────────────────────────────
            if state.show_history_search {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("(reverse-i-search):").color(Color32::YELLOW));
                    ui.text_edit_singleline(&mut state.history_search_query);
                    if ui.small_button("✕").clicked() {
                        state.show_history_search = false;
                        state.history_search_query.clear();
                    }
                });
            }

            let font_id = FontId::monospace(state.font_size);
            let available = ui.available_size();

            // ── Zone de défilement principale ────────────────────────────────
            ScrollArea::vertical()
                .auto_shrink([false; 2])
                .stick_to_bottom(state.scroll_to_bottom)
                .show(ui, |ui| {
                    ui.set_min_size(available);

                    for line in &state.lines {
                        ui.horizontal_wrapped(|ui| {
                            for span in &line.spans {
                                let mut rt = egui::RichText::new(&span.text)
                                    .font(font_id.clone())
                                    .color(span.fg);
                                if span.bold { rt = rt.strong(); }
                                ui.label(rt);
                            }
                            if line.spans.is_empty() {
                                ui.label(egui::RichText::new(" ").font(font_id.clone()));
                            }
                        });
                    }

                    if !state.current_line.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            for span in &state.current_line {
                                ui.label(
                                    egui::RichText::new(&span.text)
                                        .font(font_id.clone())
                                        .color(span.fg),
                                );
                            }
                        });
                    }
                });

            // ── Ligne de saisie ───────────────────────────────────────────────
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("❯ ")
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
                if !response.has_focus() {
                    response.request_focus();
                }

                // Entrée → envoie la commande si aucun raccourci de contrôle n'a déjà été capturé.
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
                    if ui.button("✕ Annuler").clicked()     { cancelled = true; }
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

/// Formate une taille en octets en chaîne lisible (Ko / Mo).
fn format_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} o", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} Ko", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} Mo", bytes as f64 / (1024.0 * 1024.0))
    }
}
