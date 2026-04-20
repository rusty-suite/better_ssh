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
                            // On ne traite que 'm' (SGR = Select Graphic Rendition).
                            // Les autres commandes (déplacement curseur, etc.) sont ignorées.
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
                // Décodage complet pour éviter l'affichage de 'Ã©' à la place de 'é'.
                0xC0..=0xDF => {
                    // 2 octets (ex: é U+00E9 → 0xC3 0xA9)
                    if i + 2 > self.ansi_buf.len() { break; }
                    // Copie en String pour libérer le borrow sur ansi_buf avant push_char.
                    let s = std::str::from_utf8(&self.ansi_buf[i..i + 2])
                        .unwrap_or("\u{FFFD}").to_string();
                    for ch in s.chars() { self.push_char(ch); }
                    i += 2;
                }
                0xE0..=0xEF => {
                    // 3 octets (ex: € U+20AC → 0xE2 0x82 0xAC)
                    if i + 3 > self.ansi_buf.len() { break; }
                    let s = std::str::from_utf8(&self.ansi_buf[i..i + 3])
                        .unwrap_or("\u{FFFD}").to_string();
                    for ch in s.chars() { self.push_char(ch); }
                    i += 3;
                }
                0xF0..=0xF7 => {
                    // 4 octets (ex: 😀 U+1F600 → 0xF0 0x9F 0x98 0x80)
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
            // ESC[m ou ESC[0m → réinitialise tous les attributs.
            self.reset_attrs();
            return;
        }

        let mut j = 0;
        while j < codes.len() {
            match codes[j] {
                0  => self.reset_attrs(),
                1  => self.current_bold = true,
                22 => self.current_bold = false,
                // Couleurs 3/4 bits standard (foreground 30–37 / bright 90–97)
                30 => self.current_fg = Color32::from_rgb(0, 0, 0),
                31 => self.current_fg = Color32::from_rgb(205, 49, 49),
                32 => self.current_fg = Color32::from_rgb(13, 188, 121),
                33 => self.current_fg = Color32::from_rgb(229, 229, 16),
                34 => self.current_fg = Color32::from_rgb(36, 114, 200),
                35 => self.current_fg = Color32::from_rgb(188, 63, 188),
                36 => self.current_fg = Color32::from_rgb(17, 168, 205),
                37 => self.current_fg = Color32::from_rgb(229, 229, 229),
                39 => self.current_fg = Color32::from_rgb(204, 204, 204), // défaut
                90 => self.current_fg = Color32::from_rgb(102, 102, 102),
                91 => self.current_fg = Color32::from_rgb(241, 76, 76),
                92 => self.current_fg = Color32::from_rgb(35, 209, 139),
                93 => self.current_fg = Color32::from_rgb(245, 245, 67),
                94 => self.current_fg = Color32::from_rgb(59, 142, 234),
                95 => self.current_fg = Color32::from_rgb(214, 112, 214),
                96 => self.current_fg = Color32::from_rgb(41, 184, 219),
                97 => self.current_fg = Color32::from_rgb(255, 255, 255),
                // Couleur 24 bits : ESC[38;2;R;G;Bm
                38 if j + 4 < codes.len() && codes[j + 1] == 2 => {
                    self.current_fg =
                        Color32::from_rgb(codes[j + 2], codes[j + 3], codes[j + 4]);
                    j += 4;
                }
                _ => {} // code SGR non supporté → ignoré silencieusement
            }
            j += 1;
        }
    }

    /// Remet les attributs visuels à leurs valeurs par défaut (fond noir, texte gris).
    fn reset_attrs(&mut self) {
        self.current_fg = Color32::from_rgb(204, 204, 204);
        self.current_bg = None;
        self.current_bold = false;
    }
}

// ─── Rendu egui ──────────────────────────────────────────────────────────────

/// Retourne les octets à envoyer au serveur SSH si l'utilisateur a validé une saisie,
/// `None` sinon (pas de saisie cette frame).
/// `modal_open` : si true, le terminal ne vole pas le focus (un dialogue est ouvert).
pub fn render(state: &mut TerminalState, ui: &mut Ui, modal_open: bool) -> Option<Vec<u8>> {
    // Couleur de fond du terminal (inspirée du thème Dracula).
    let bg = Color32::from_rgb(20, 20, 30);

    // Octets à retourner à l'appelant pour envoi SSH (rempli si l'utilisateur valide une saisie).
    let mut to_send: Option<Vec<u8>> = None;

    // Ctrl+Scroll pour ajuster la taille de police à la volée.
    let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
    if ui.input(|i| i.modifiers.ctrl) && scroll_delta != 0.0 {
        state.font_size = (state.font_size + scroll_delta * 0.05).clamp(8.0, 32.0);
    }

    // Ctrl+R → bascule la popup de recherche dans l'historique.
    if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::R)) {
        state.show_history_search = !state.show_history_search;
    }

    egui::Frame::none()
        .fill(bg)
        .inner_margin(egui::Margin::same(6.0))
        .show(ui, |ui| {
            // ── Barre de recherche historique (si active) ─────────────────────
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

                    // Affiche les lignes complètes du scrollback.
                    for line in &state.lines {
                        ui.horizontal_wrapped(|ui| {
                            for span in &line.spans {
                                let mut rt = egui::RichText::new(&span.text)
                                    .font(font_id.clone())
                                    .color(span.fg);
                                if span.bold { rt = rt.strong(); }
                                ui.label(rt);
                            }
                            // Ligne vide → espace pour maintenir la hauteur de ligne.
                            if line.spans.is_empty() {
                                ui.label(egui::RichText::new(" ").font(font_id.clone()));
                            }
                        });
                    }

                    // Affiche la ligne en cours (SSH) + local echo (saisie utilisateur).
                    ui.horizontal_wrapped(|ui| {
                        for span in &state.current_line {
                            let mut rt = egui::RichText::new(&span.text)
                                .font(font_id.clone())
                                .color(span.fg);
                            if span.bold { rt = rt.strong(); }
                            ui.label(rt);
                        }
                        // Local echo : affiche ce que l'utilisateur tape (avant envoi) + curseur █.
                        let echo = format!("{}█", state.input);
                        ui.label(
                            egui::RichText::new(echo)
                                .font(font_id.clone())
                                .color(Color32::WHITE),
                        );
                    });
                });

            // ── Champ de saisie invisible (capture clavier uniquement) ────────
            let response = ui.add_sized(
                [0.0, 0.0],
                egui::TextEdit::singleline(&mut state.input)
                    .font(egui::TextStyle::Monospace)
                    .frame(false)
                    .text_color(Color32::TRANSPARENT),
            );
            // Ne vole le focus que si aucun dialogue modal n'est ouvert.
            // Sinon les champs texte des dialogues (IP, utilisateur…) seraient inutilisables.
            if !modal_open && !response.has_focus() {
                response.request_focus();
            }

            // Entrée validée → envoie la ligne au serveur (avec \n) et vide le champ.
            if response.has_focus()
                && ui.input(|i| i.key_pressed(Key::Enter))
            {
                let mut cmd = state.input.clone();
                cmd.push('\n');
                to_send = Some(cmd.into_bytes());
                state.input.clear();
            }
        });

    to_send
}
