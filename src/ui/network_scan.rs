/// Panneau de scan réseau.
/// Permet de découvrir automatiquement les serveurs SSH sur le réseau local,
/// de modifier les paramètres de recherche et de se connecter en un clic.
use crate::network::scanner::{detect_local_subnet, NetworkScanner, ScanEvent, ScanParams, ScanResult};
use crossbeam_channel::Receiver;
use egui::Ui;

// ─── Action retournée au parent ───────────────────────────────────────────────

/// Commande renvoyée par le panneau vers `RustShellApp` après interaction.
pub enum ScanAction {
    /// L'utilisateur a cliqué "Connecter" — transporte le résultat brut du scan.
    /// Le dialogue de saisie des identifiants est géré par `ui/mod.rs` qui a accès
    /// aux profils existants et au vault.
    Connect(ScanResult),
    /// Pas d'action à propager.
    None,
}

// ─── État du panneau ──────────────────────────────────────────────────────────

/// Tout l'état mutable du panneau de scan réseau.
pub struct NetworkScanState {
    /// Paramètres courants du scan (modifiables par l'utilisateur).
    pub params: ScanParams,
    /// Paramètres "usine" calculés au lancement (pour le bouton Réinitialiser).
    default_params: ScanParams,
    /// Résultats accumulés depuis le dernier scan.
    pub results: Vec<ScanResult>,
    /// true si un scan est en cours.
    pub scanning: bool,
    /// (traités, total) pour la barre de progression.
    pub progress: (usize, usize),
    /// Message d'erreur à afficher, le cas échéant.
    pub error: Option<String>,
    /// Récepteur d'événements du scanner (actif pendant le scan).
    event_rx: Option<Receiver<ScanEvent>>,
    /// Affiche/masque le panneau des paramètres avancés.
    show_params: bool,
    /// Filtre texte appliqué à la table des résultats.
    filter: String,
}

impl NetworkScanState {
    /// Crée l'état initial en auto-détectant le réseau local.
    pub fn new() -> Self {
        // Tente la détection automatique ; revient à 192.168.1.0/24 en cas d'échec.
        let default_params = ScanParams {
            target: detect_local_subnet().unwrap_or_else(|| "192.168.1.0/24".into()),
            ..ScanParams::default()
        };
        let params = default_params.clone();
        Self {
            params,
            default_params,
            results: Vec::new(),
            scanning: false,
            progress: (0, 0),
            error: None,
            event_rx: None,
            show_params: true,
            filter: String::new(),
        }
    }

    // ─── Lancement / arrêt du scan ────────────────────────────────────────────

    /// Parse la plage cible et démarre le scan en arrière-plan (tokio::spawn).
    fn start_scan(&mut self) {
        let ips = match NetworkScanner::parse_range(&self.params.target) {
            Ok(i) => i,
            Err(e) => {
                self.error = Some(format!("Plage invalide : {e}"));
                return;
            }
        };

        let (tx, rx) = crossbeam_channel::unbounded::<ScanEvent>();
        self.event_rx = Some(rx);
        self.results.clear();
        self.scanning = true;
        self.progress = (0, ips.len());
        self.error = None;

        let params = self.params.clone();
        tokio::spawn(async move {
            // Le scan tourne dans un tokio task et envoie les événements via tx.
            NetworkScanner::scan(ips, params, tx).await;
        });
    }

    /// Interrompt le scan en cours (les événements restants sont abandonnés).
    fn stop_scan(&mut self) {
        self.scanning = false;
        self.event_rx = None; // le Sender dans la tâche tokio détectera la déconnexion
    }

    /// Réinitialise les paramètres à leurs valeurs par défaut (réseau auto-détecté).
    fn reset_params(&mut self) {
        self.params = self.default_params.clone();
        self.error = None;
    }

    // ─── Collecte des événements asynchrones ──────────────────────────────────

    /// À appeler à chaque frame pour vider le canal d'événements du scanner.
    fn poll_events(&mut self) {
        let rx = match self.event_rx.as_ref() {
            Some(r) => r,
            None => return,
        };
        // On vide tout le canal en une seule frame pour ne pas prendre de retard.
        loop {
            match rx.try_recv() {
                Ok(ScanEvent::Progress { done, total }) => {
                    self.progress = (done, total);
                }
                Ok(ScanEvent::Found(r)) => {
                    self.results.push(r);
                }
                Ok(ScanEvent::Finished) => {
                    self.scanning = false;
                    self.event_rx = None;
                    break;
                }
                Ok(ScanEvent::Error(e)) => {
                    self.error = Some(e);
                    self.scanning = false;
                    self.event_rx = None;
                    break;
                }
                Err(_) => break, // canal vide pour cette frame
            }
        }
    }
}

// ─── Rendu egui ──────────────────────────────────────────────────────────────

/// Affiche le panneau complet et retourne une éventuelle action vers l'app parente.
pub fn render(state: &mut NetworkScanState, ui: &mut Ui) -> ScanAction {
    // Collecte d'abord les résultats arrivés depuis la dernière frame.
    state.poll_events();

    let mut action = ScanAction::None;

    ui.heading("🔍 Scan réseau SSH");
    ui.separator();

    // ── Paramètres ────────────────────────────────────────────────────────────
    render_params_panel(state, ui);

    // ── Messages d'état ───────────────────────────────────────────────────────
    if let Some(err) = &state.error.clone() {
        ui.colored_label(egui::Color32::from_rgb(220, 60, 60), format!("⚠ {err}"));
    }

    if state.scanning || state.progress.1 > 0 {
        let (done, total) = state.progress;
        let pct = if total > 0 { done as f32 / total as f32 } else { 0.0 };
        let eta_s = if done > 0 && pct < 1.0 {
            let elapsed_ratio = 1.0 - pct;
            let remaining = total - done;
            // Estimation grossière basée sur le débit moyen observé.
            format!("  ~{} restants", remaining)
        } else {
            String::new()
        };
        ui.add(
            egui::ProgressBar::new(pct)
                .text(format!("{done}/{total}{eta_s}"))
                .animate(state.scanning),
        );
    }

    ui.separator();

    // ── Table des résultats ───────────────────────────────────────────────────
    if state.results.is_empty() {
        if !state.scanning {
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
                ui.label(egui::RichText::new("Aucun résultat.").weak());
                ui.label("Configurez la plage et appuyez sur ▶ Scanner.");
            });
        }
    } else {
        render_results_table(state, ui, &mut action);
    }

    action
}

/// Affiche le bloc de paramètres (pliable) avec boutons Scanner / Arrêter / Réinitialiser.
fn render_params_panel(state: &mut NetworkScanState, ui: &mut Ui) {
    // En-tête cliquable pour plier/déplier les paramètres.
    ui.horizontal(|ui| {
        let arrow = if state.show_params { "▼" } else { "▶" };
        if ui.button(format!("{arrow} Paramètres")).clicked() {
            state.show_params = !state.show_params;
        }

        // Boutons de contrôle toujours visibles même quand les params sont pliés.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if state.scanning {
                if ui.button("⏹ Arrêter").clicked() {
                    state.stop_scan();
                }
                ui.add_enabled(false, egui::Button::new("⏳ Scan en cours…"));
            } else {
                if ui.button("▶ Scanner").clicked() {
                    state.start_scan();
                }
                if ui.button("🔄 Réinitialiser").on_hover_text(
                    "Remet les paramètres à leurs valeurs détectées automatiquement"
                ).clicked() {
                    state.reset_params();
                }
            }
        });
    });

    if !state.show_params {
        return; // panneau replié
    }

    egui::Frame::none()
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(egui::Margin::same(8.0))
        .outer_margin(egui::Margin::symmetric(0.0, 4.0))
        .show(ui, |ui| {
            egui::Grid::new("scan_params_grid")
                .num_columns(4)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    // ── Plage cible ───────────────────────────────────────────
                    ui.label("Plage cible :");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.params.target)
                            .desired_width(160.0)
                            .hint_text("192.168.1.0/24 ou 192.168.1.1-254"),
                    );
                    // Indicateur du sous-réseau auto-détecté
                    ui.label(
                        egui::RichText::new(format!("(détecté : {})", state.default_params.target))
                            .small()
                            .weak(),
                    );
                    ui.end_row();

                    // ── Port SSH ──────────────────────────────────────────────
                    ui.label("Port SSH :");
                    let mut port_str = state.params.ssh_port.to_string();
                    if ui
                        .add(egui::TextEdit::singleline(&mut port_str).desired_width(60.0))
                        .changed()
                    {
                        state.params.ssh_port = port_str.parse().unwrap_or(22);
                    }
                    ui.label(egui::RichText::new("(22 par défaut)").small().weak());
                    ui.end_row();

                    // ── Délai de timeout ──────────────────────────────────────
                    ui.label("Délai (ms) :");
                    ui.add(
                        egui::Slider::new(&mut state.params.timeout_ms, 100..=5000)
                            .suffix(" ms")
                            .clamping(egui::SliderClamping::Always),
                    );
                    ui.label(
                        egui::RichText::new("Augmenter pour les réseaux lents").small().weak(),
                    );
                    ui.end_row();

                    // ── Concurrence ───────────────────────────────────────────
                    ui.label("Parallélisme :");
                    ui.add(
                        egui::Slider::new(&mut state.params.concurrency, 4..=256)
                            .suffix(" connexions")
                            .clamping(egui::SliderClamping::Always),
                    );
                    ui.label(
                        egui::RichText::new("Réduire si le réseau est instable").small().weak(),
                    );
                    ui.end_row();
                });
        });
}

/// Affiche la table des résultats avec filtre et boutons d'action.
fn render_results_table(
    state: &mut NetworkScanState,
    ui: &mut Ui,
    action: &mut ScanAction,
) {
    // Barre de filtre sur les résultats
    ui.horizontal(|ui| {
        ui.label(format!("{} hôte(s) trouvé(s)", state.results.len()));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("📋 Exporter CSV").clicked() {
                export_csv(&state.results);
            }
            ui.add(
                egui::TextEdit::singleline(&mut state.filter)
                    .desired_width(140.0)
                    .hint_text("Filtrer…"),
            );
            ui.label("🔍");
        });
    });

    let filter = state.filter.to_lowercase();

    egui::ScrollArea::vertical()
        .id_salt("scan_results_scroll")
        .show(ui, |ui| {
            egui::Grid::new("scan_results_grid")
                .num_columns(6)
                .striped(true)
                .min_col_width(80.0)
                .show(ui, |ui| {
                    // En-têtes
                    ui.strong("IP");
                    ui.strong("Hostname");
                    ui.strong("Latence");
                    ui.strong("Bannière SSH");
                    ui.strong("Version");
                    ui.strong("Action");
                    ui.end_row();

                    // Lignes filtrées
                    let mut connect_target: Option<ScanResult> = None;

                    for result in state.results.iter().filter(|r| {
                        filter.is_empty()
                            || r.ip.to_string().contains(&filter)
                            || r.hostname.as_deref().unwrap_or("").to_lowercase().contains(&filter)
                            || r.ssh_banner.as_deref().unwrap_or("").to_lowercase().contains(&filter)
                    }) {
                        ui.label(result.ip.to_string());

                        ui.label(result.hostname.as_deref().unwrap_or("—"));

                        // Colorise la latence : vert < 50 ms, orange < 200 ms, rouge sinon
                        if let Some(ms) = result.latency_ms {
                            let color = if ms < 50 {
                                egui::Color32::from_rgb(100, 200, 100)
                            } else if ms < 200 {
                                egui::Color32::from_rgb(220, 180, 50)
                            } else {
                                egui::Color32::from_rgb(220, 80, 80)
                            };
                            ui.colored_label(color, format!("{ms} ms"));
                        } else {
                            ui.label("—");
                        }

                        // Bannière complète
                        let banner = result.ssh_banner.as_deref().unwrap_or("—");
                        ui.label(banner)
                            .on_hover_text(banner); // tooltip pour les bannières longues

                        // Extrait la version OpenSSH de la bannière (ex: "OpenSSH_8.9")
                        let version = result
                            .ssh_banner
                            .as_deref()
                            .and_then(|b| b.split_whitespace().last())
                            .unwrap_or("—");
                        ui.label(version);

                        // Bouton de connexion directe
                        if ui
                            .button("🔌 Connecter")
                            .on_hover_text("Crée un profil et ouvre une session SSH")
                            .clicked()
                        {
                            connect_target = Some(result.clone());
                        }
                        ui.end_row();
                    }

                    // Traite l'action de connexion en dehors de la boucle (évite le borrow double).
                    if let Some(r) = connect_target {
                        *action = ScanAction::Connect(r);
                    }
                });
        });
}

/// Exporte la liste des résultats en CSV via la boîte de dialogue native.
fn export_csv(results: &[ScanResult]) {
    let mut csv = "IP,Hostname,Latence (ms),Bannière SSH\n".to_string();
    for r in results {
        csv += &format!(
            "{},{},{},{}\n",
            r.ip,
            r.hostname.as_deref().unwrap_or(""),
            r.latency_ms.map(|l| l.to_string()).unwrap_or_default(),
            r.ssh_banner.as_deref().unwrap_or(""),
        );
    }
    if let Some(path) = rfd::FileDialog::new()
        .set_file_name("scan_ssh.csv")
        .save_file()
    {
        let _ = std::fs::write(path, csv);
    }
}
