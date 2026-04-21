/// Widget terminal interactif.
/// Parse les séquences ANSI/VT100 et affiche le texte avec les couleurs correspondantes.
/// La saisie est gérée par un champ de texte egui en bas du panneau.
use egui::{Color32, FontId, Key, Modifiers, ScrollArea, Ui};
use std::collections::VecDeque;

// ─── Constantes ───────────────────────────────────────────────────────────────

/// Nombre maximal de lignes conservées dans le scrollback.
const MAX_LINES: usize = 10_000;

/// Nombre maximal de commandes dans l'historique de session (par onglet).
const MAX_SESSION_HISTORY: usize = 50;

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

    /// Historique de session (max 50 commandes). Index 0 = la plus récente.
    pub session_history: VecDeque<String>,
    /// true = liste déroulante de l'historique visible.
    pub show_history_dropdown: bool,
    /// Index sélectionné dans la liste filtrée (0 = commande la plus récente).
    pub history_dropdown_idx: Option<usize>,
    /// true si le champ de saisie invisible avait le focus à la frame précédente.
    /// Permet de ne consommer les touches que quand le terminal est actif.
    pub input_focused: bool,
    /// true = le shell distant gère le buffer (après Tab ou Ctrl+touche).
    /// Chaque touche est alors envoyée directement au serveur sans buffering local.
    pub server_managed: bool,

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
            session_history: VecDeque::new(),
            show_history_dropdown: false,
            history_dropdown_idx: None,
            input_focused: false,
            server_managed: false,
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
                b'\r' => {
                    // Lookahead : \r\n = fin de ligne normale (le \n gèrera le saut).
                    //              \r seul = retour au début de ligne (écrasement en place).
                    if i + 1 < self.ansi_buf.len() {
                        if self.ansi_buf[i + 1] != b'\n' {
                            // Retour chariot seul : on efface la ligne courante pour
                            // que le prochain contenu la remplace (indicateurs de progression).
                            self.current_line.clear();
                        }
                        i += 1;
                    } else {
                        // Dernier octet du buffer : on ne sait pas si un \n suit.
                        // On attend le prochain chunk plutôt que de décider à l'aveugle.
                        break;
                    }
                }
                b'\n' => { self.newline(); i += 1; }
                b'\x08' => {
                    // Backspace : supprime le dernier caractère du span courant.
                    if let Some(span) = self.current_line.last_mut() {
                        span.text.pop();
                    }
                    i += 1;
                }
                0x07 => { i += 1; } // BEL → ignoré silencieusement
                0x0e | 0x0f => { i += 1; } // SO/SI (shift charset) → ignoré
                0x1b => {
                    // Séquence d'échappement ESC — attend au moins un octet de plus.
                    if i + 1 >= self.ansi_buf.len() { break; }
                    match self.ansi_buf[i + 1] {
                        b'[' => {
                            // CSI (Control Sequence Introducer) : ESC [ params cmd
                            let start = i + 2;
                            let mut end = start;
                            // Les paramètres CSI sont des chiffres, ';', '?' et ' '.
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
                            match cmd {
                                'm' => self.apply_sgr(&params_str),
                                // Erase in Line (K) : efface tout ou partie de la ligne courante.
                                // Sans curseur précis on simplifie : clear de la ligne.
                                'K' => { self.current_line.clear(); }
                                // Erase Display (J) : \e[2J = clear screen complet.
                                'J' => {
                                    let n: u32 = params_str
                                        .trim_start_matches('?')
                                        .parse().unwrap_or(0);
                                    if n >= 2 {
                                        self.lines.clear();
                                    }
                                    self.current_line.clear();
                                }
                                _ => {} // autres séquences CSI ignorées silencieusement
                            }
                            i = end + 1;
                        }
                        b']' => {
                            // OSC (Operating System Command) : ESC ] ... BEL | ESC-backslash
                            // Utilisé par le shell pour mettre à jour le titre de la fenêtre.
                            // On consomme tout jusqu'au terminateur sans rien afficher.
                            let mut end = i + 2;
                            let found = loop {
                                if end >= self.ansi_buf.len() { break false; }
                                if self.ansi_buf[end] == 0x07 {
                                    // BEL termine l'OSC.
                                    end += 1;
                                    break true;
                                }
                                if self.ansi_buf[end] == 0x1b
                                    && end + 1 < self.ansi_buf.len()
                                    && self.ansi_buf[end + 1] == b'\\'
                                {
                                    // ST (ESC \) termine l'OSC.
                                    end += 2;
                                    break true;
                                }
                                end += 1;
                            };
                            if found {
                                i = end;
                            } else {
                                break; // séquence incomplète → attend plus de données
                            }
                        }
                        b'(' | b')' | b'*' | b'+' => {
                            // Désignation de jeu de caractères : ESC ( G — 3 octets.
                            if i + 2 < self.ansi_buf.len() { i += 3; } else { break; }
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

// ─── Helpers historique ───────────────────────────────────────────────────────

/// Retourne la liste filtrée (newest first, index 0) selon le filtre courant.
/// Produit des String clonées pour éviter les problèmes de durée de vie.
fn build_filtered_history(history: &VecDeque<String>, filter: &str) -> Vec<String> {
    let f = filter.trim().to_lowercase();
    history.iter()
        .filter(|e| f.is_empty() || e.to_lowercase().contains(&f))
        .cloned()
        .collect()
}

/// Pousse une commande dans l'historique de session (déduplique les consécutifs).
fn push_session_history(history: &mut VecDeque<String>, cmd: &str) {
    let trimmed = cmd.trim();
    if trimmed.is_empty() { return; }
    if history.front().map(String::as_str) == Some(trimmed) { return; }
    history.push_front(trimmed.to_string());
    if history.len() > MAX_SESSION_HISTORY {
        history.pop_back();
    }
}

// ─── Helpers édition ─────────────────────────────────────────────────────────

/// Supprime le dernier mot du buffer local (équivalent readline Ctrl+W).
/// Respecte les guillemets : efface jusqu'à l'espace non-quoté précédent.
fn delete_last_word(input: &mut String) {
    let chars: Vec<char> = input.chars().collect();
    if chars.is_empty() { return; }

    // Ignore les espaces trailing.
    let mut end = chars.len();
    while end > 0 && chars[end - 1] == ' ' {
        end -= 1;
    }
    if end == 0 { input.clear(); return; }

    // Recule jusqu'au premier espace non précédé d'un backslash ou hors guillemets.
    let mut i = end;
    let mut in_single = false;
    let mut in_double = false;
    // Rescan depuis le début pour connaître l'état des guillemets à position `end`.
    let mut j = 0;
    while j < end {
        match chars[j] {
            '\'' if !in_double => in_single = !in_single,
            '"'  if !in_single => in_double = !in_double,
            '\\' if j + 1 < end => { j += 1; } // skip escaped char
            _ => {}
        }
        j += 1;
    }
    // Recule sur le mot (espaces non-quotés = délimiteurs).
    while i > 0 {
        let c = chars[i - 1];
        if c == ' ' && !in_single && !in_double { break; }
        i -= 1;
    }

    *input = chars[..i].iter().collect();
}

// ─── Rendu egui ──────────────────────────────────────────────────────────────

/// Retourne les octets à envoyer au serveur SSH si l'utilisateur a validé une saisie,
/// `None` sinon (pas de saisie cette frame).
/// `modal_open` : si true, le terminal ne vole pas le focus (un dialogue est ouvert).
pub fn render(state: &mut TerminalState, ui: &mut Ui, modal_open: bool) -> Option<Vec<u8>> {
    // Couleur de fond du terminal (inspirée du thème Dracula).
    let bg = Color32::from_rgb(20, 20, 30);

    // Octets à retourner à l'appelant pour envoi SSH.
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

    // ── Gestion des touches (avant le TextEdit) ──────────────────────────────
    // On ne consomme les touches que si le terminal avait le focus la frame précédente.
    let terminal_active = state.input_focused && !modal_open;

    if terminal_active {
        // egui génère deux événements distincts pour Tab : Key::Tab ET Text("\t").
        // consume_key() supprime le premier mais pas le second ; le TextEdit reçoit
        // alors Text("\t") et insère un caractère tabulation littéral dans state.input.
        // On drainne Text("\t") systématiquement — un tab littéral n'a aucun sens
        // dans un terminal (le shell gère la complétion via le caractère \t envoyé).
        ui.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Text(t) if t == "\t")));

        if state.server_managed {
            // ── Mode piloté par le serveur ────────────────────────────────────
            // Après un Tab, le shell distant gère le buffer de saisie.
            // On envoie chaque touche directement, sans buffering local.
            let mut raw: Vec<u8> = Vec::new();

            // Drain tous les événements Text avant que le TextEdit ne les capte.
            ui.input_mut(|i| i.events.retain(|e| {
                if let egui::Event::Text(t) = e {
                    raw.extend_from_slice(t.as_bytes());
                    false
                } else {
                    true
                }
            }));

            // ── Touches de navigation readline ────────────────────────────────
            if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
                raw.push(b'\n');
                state.server_managed = false;
                state.input.clear();
            }
            if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Backspace)) {
                raw.push(0x7F);
            }
            // Delete (Suppr) → séquence VT delete-char
            if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Delete)) {
                raw.extend_from_slice(b"\x1b[3~");
            }
            if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab)) {
                raw.push(b'\t');
            }
            // Flèches
            if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp)) {
                raw.extend_from_slice(b"\x1b[A");
            }
            if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowDown)) {
                raw.extend_from_slice(b"\x1b[B");
            }
            if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowLeft)) {
                raw.extend_from_slice(b"\x1b[D");
            }
            if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowRight)) {
                raw.extend_from_slice(b"\x1b[C");
            }
            // Home / End → début / fin de ligne (readline: Ctrl+A / Ctrl+E)
            if ui.input_mut(|i| {
                i.consume_key(Modifiers::NONE, Key::Home)
                    || i.consume_key(Modifiers::CTRL, Key::A)
            }) {
                raw.push(0x01); // Ctrl+A
            }
            if ui.input_mut(|i| {
                i.consume_key(Modifiers::NONE, Key::End)
                    || i.consume_key(Modifiers::CTRL, Key::E)
            }) {
                raw.push(0x05); // Ctrl+E
            }
            // Ctrl+W : supprime le mot précédant le curseur
            if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::W)) {
                raw.push(0x17);
            }
            // Ctrl+U : efface du début de ligne jusqu'au curseur
            if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::U)) {
                raw.push(0x15);
            }
            // Ctrl+K : efface du curseur jusqu'à la fin de ligne
            if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::K)) {
                raw.push(0x0b);
            }
            // Ctrl+L : clear screen (équivalent à `clear`)
            if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::L)) {
                raw.push(0x0c);
            }
            // Ctrl+D : EOF (déconnexion ou fin de saisie)
            if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::D)) {
                raw.push(0x04);
            }
            // Ctrl+C → interruption, retour au mode local.
            if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::C)) {
                raw.push(0x03);
                state.server_managed = false;
                state.input.clear();
            }
            // Ctrl+Z : suspend (SIGTSTP)
            if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Z)) {
                raw.push(0x1a);
            }

            if !raw.is_empty() {
                to_send = Some(raw);
            }
        } else {
            // ── Mode local (buffer côté client) ──────────────────────────────

            // Ctrl+C : interrompt et efface le buffer local.
            if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::C)) {
                state.input.clear();
                state.show_history_dropdown = false;
                state.history_dropdown_idx = None;
                to_send = Some(vec![0x03]);
            }

            // Ctrl+U : efface tout le buffer local (équivalent readline).
            if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::U)) {
                state.input.clear();
                state.show_history_dropdown = false;
                state.history_dropdown_idx = None;
            }

            // Ctrl+W : supprime le dernier mot du buffer local.
            if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::W)) {
                delete_last_word(&mut state.input);
                state.show_history_dropdown = false;
                state.history_dropdown_idx = None;
            }

            // Ctrl+L : clear screen (envoyé au serveur même en mode local).
            if ui.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::L)) {
                to_send = Some(vec![0x0c]);
            }

            // Flèche Haut : ouvre ou remonte dans la liste déroulante.
            if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp)) {
                if !state.show_history_dropdown {
                    let hist = build_filtered_history(&state.session_history, &state.input);
                    if !hist.is_empty() {
                        state.show_history_dropdown = true;
                        state.history_dropdown_idx = Some(0);
                    }
                } else {
                    let hist = build_filtered_history(&state.session_history, &state.input);
                    if let Some(idx) = state.history_dropdown_idx {
                        if idx + 1 < hist.len() {
                            state.history_dropdown_idx = Some(idx + 1);
                        }
                    }
                }
            }

            // Flèche Bas : descend ou ferme la liste.
            if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowDown)) {
                if state.show_history_dropdown {
                    match state.history_dropdown_idx {
                        Some(0) | None => {
                            state.show_history_dropdown = false;
                            state.history_dropdown_idx = None;
                        }
                        Some(n) => { state.history_dropdown_idx = Some(n - 1); }
                    }
                }
            }

            // Échap : ferme la liste, ou efface le buffer si la liste est déjà fermée.
            if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                if state.show_history_dropdown {
                    state.show_history_dropdown = false;
                    state.history_dropdown_idx = None;
                } else {
                    state.input.clear();
                }
            }

            // Entrée avec liste ouverte → sélectionne (sans envoyer).
            if state.show_history_dropdown
                && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter))
            {
                let hist = build_filtered_history(&state.session_history, &state.input);
                if let Some(idx) = state.history_dropdown_idx {
                    if let Some(cmd) = hist.get(idx) {
                        state.input = cmd.clone();
                    }
                }
                state.show_history_dropdown = false;
                state.history_dropdown_idx = None;
            }

            // Tab : sélectionne dans la liste OU lance la complétion côté serveur.
            if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab)) {
                if state.show_history_dropdown {
                    let hist = build_filtered_history(&state.session_history, &state.input);
                    if let Some(idx) = state.history_dropdown_idx {
                        if let Some(cmd) = hist.get(idx) {
                            state.input = cmd.clone();
                        }
                    }
                    state.show_history_dropdown = false;
                    state.history_dropdown_idx = None;
                } else {
                    // Envoie le buffer local + \t au shell distant.
                    // Le shell gère la complétion en tenant compte du contexte :
                    // guillemets ouverts, redirections, pipes, etc.
                    // readline reçoit les octets un par un via le PTY et traite
                    // \t comme une demande de complétion (pas comme du texte collé).
                    let mut bytes = state.input.as_bytes().to_vec();
                    bytes.push(b'\t');
                    to_send = Some(bytes);
                    state.input.clear();
                    state.server_managed = true;
                }
            }
        }
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
                        // Local echo : en mode piloté par le serveur, seul le curseur
                        // est affiché (le shell distant gère le buffer et l'affichage).
                        let echo = if state.server_managed {
                            "█".to_string()
                        } else {
                            format!("{}█", state.input)
                        };
                        ui.label(
                            egui::RichText::new(echo)
                                .font(font_id.clone())
                                .color(Color32::WHITE),
                        );
                    });
                });

            // ── Liste déroulante de l'historique ─────────────────────────────
            if state.show_history_dropdown {
                let hist = build_filtered_history(&state.session_history, &state.input);

                // Referme si le filtre n'a plus de résultats.
                if hist.is_empty() {
                    state.show_history_dropdown = false;
                    state.history_dropdown_idx = None;
                } else {
                    // Corrige l'index si le filtre a réduit la liste.
                    if let Some(idx) = state.history_dropdown_idx {
                        if idx >= hist.len() {
                            state.history_dropdown_idx = Some(0);
                        }
                    }

                    let clip = ui.clip_rect();
                    let item_h = state.font_size + 10.0;
                    let visible_count = hist.len().min(8) as f32;
                    let dropdown_h = visible_count * item_h + 10.0;
                    let dropdown_w = (clip.width() - 24.0).clamp(200.0, 640.0);

                    // Positionne la liste juste au-dessus de la ligne d'écho.
                    let pos = egui::pos2(
                        clip.left() + 12.0,
                        clip.bottom() - dropdown_h - item_h * 1.8,
                    );

                    let selected_idx = state.history_dropdown_idx;
                    let mut new_input: Option<String> = None;
                    let mut close_dropdown = false;

                    egui::Area::new(egui::Id::new("hist_dropdown"))
                        .order(egui::Order::Foreground)
                        .fixed_pos(pos)
                        .show(ui.ctx(), |ui| {
                            ui.set_max_width(dropdown_w);
                            egui::Frame::none()
                                .fill(Color32::from_rgb(28, 28, 45))
                                .stroke(egui::Stroke::new(
                                    1.0,
                                    Color32::from_rgb(90, 90, 170),
                                ))
                                .inner_margin(egui::Margin::same(4.0))
                                .show(ui, |ui| {
                                    // En-tête discret.
                                    ui.label(
                                        egui::RichText::new(format!(
                                            " Historique ({}/{})",
                                            selected_idx.map(|i| i + 1).unwrap_or(0),
                                            hist.len()
                                        ))
                                        .small()
                                        .color(Color32::from_rgb(120, 120, 180)),
                                    );
                                    ui.separator();

                                    egui::ScrollArea::vertical()
                                        .max_height(dropdown_h - 32.0)
                                        .show(ui, |ui| {
                                            // Affiche du plus ancien (haut) au plus récent (bas).
                                            let len = hist.len();
                                            for rev_i in 0..len {
                                                let i = len - 1 - rev_i;
                                                let entry = &hist[i];
                                                let selected = selected_idx == Some(i);

                                                let resp = ui.selectable_label(
                                                    selected,
                                                    egui::RichText::new(entry.as_str())
                                                        .font(FontId::monospace(
                                                            state.font_size - 1.0,
                                                        ))
                                                        .color(if selected {
                                                            Color32::WHITE
                                                        } else {
                                                            Color32::from_rgb(200, 200, 220)
                                                        }),
                                                );

                                                // Fait défiler pour garder la sélection visible.
                                                if selected {
                                                    resp.scroll_to_me(Some(egui::Align::Center));
                                                }

                                                // Clic souris → sélectionne.
                                                if resp.clicked() {
                                                    new_input = Some(entry.clone());
                                                    close_dropdown = true;
                                                }
                                            }
                                        });
                                });
                        });

                    if let Some(cmd) = new_input {
                        state.input = cmd;
                    }
                    if close_dropdown {
                        state.show_history_dropdown = false;
                        state.history_dropdown_idx = None;
                    }
                }
            }

            // ── Champ de saisie invisible (capture clavier uniquement) ────────
            let response = ui.add_sized(
                [0.0, 0.0],
                egui::TextEdit::singleline(&mut state.input)
                    .font(egui::TextStyle::Monospace)
                    .frame(false)
                    .text_color(Color32::TRANSPARENT),
            );
            // Le terminal prend le focus seulement si :
            //   1. Aucun dialogue modal n'est ouvert.
            //   2. Aucun autre widget n'a le focus clavier (barre de recherche,
            //      explorateur de fichiers, champs de formulaire, etc.).
            // Cela garantit que chaque widget ne capture que ce qu'il doit.
            if !modal_open {
                let another_widget_has_focus = ui.ctx().memory(|m| {
                    m.focused().is_some_and(|id| id != response.id)
                });
                if !another_widget_has_focus && !response.has_focus() {
                    response.request_focus();
                }
            }
            // Mémorise l'état de focus pour la frame suivante (utilisé par les handlers de touches).
            state.input_focused = response.has_focus();

            // Entrée validée → envoie la ligne au serveur (avec \n) et vide le champ.
            // (Cas dropdown déjà traité avant le Frame par consume_key.)
            if response.has_focus()
                && ui.input(|i| i.key_pressed(Key::Enter))
                && !state.show_history_dropdown
            {
                let cmd = state.input.clone();
                push_session_history(&mut state.session_history, &cmd);
                let mut send_cmd = cmd;
                send_cmd.push('\n');
                to_send = Some(send_cmd.into_bytes());
                state.input.clear();
            }
        });

    to_send
}
