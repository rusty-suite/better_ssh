/// Panneau de monitoring système distant.
/// Les métriques (CPU, RAM, disques) sont collectées via des commandes SSH
/// standard sans nécessiter d'agent côté serveur.
/// Les données sont affichées sous forme de graphes egui_plot en temps réel.
use egui::Ui;
use egui_plot::{Line, Plot, PlotPoints};

// ─── Constantes ───────────────────────────────────────────────────────────────

/// Nombre de points conservés dans l'historique des graphes (1 point = 1 refresh).
const HISTORY_LEN: usize = 60;

// ─── Structures de données ────────────────────────────────────────────────────

/// État complet du panneau monitoring pour un onglet donné.
pub struct SystemMonitorState {
    /// Historique des valeurs CPU en % (de 0 à 100).
    pub cpu_history: Vec<f64>,
    /// Historique de l'utilisation RAM en % (de 0 à 100).
    pub ram_history: Vec<f64>,
    /// Valeur CPU courante en %.
    pub cpu_percent: f64,
    /// RAM utilisée en mégaoctets.
    pub ram_used_mb: u64,
    /// RAM totale en mégaoctets.
    pub ram_total_mb: u64,
    /// Load average Linux sur 1 min, 5 min, 15 min.
    pub load_avg: [f64; 3],
    /// Uptime du serveur en secondes.
    pub uptime_secs: u64,
    /// Liste des systèmes de fichiers montés avec leur utilisation.
    pub disk_info: Vec<DiskEntry>,
    /// Intervalle de rafraîchissement en secondes (1, 5 ou 10).
    pub refresh_secs: f64,
    /// Timestamp egui de la dernière collecte (pour le timer de refresh).
    pub last_refresh: f64,
}

/// Utilisation d'un système de fichiers monté.
#[derive(Clone)]
pub struct DiskEntry {
    /// Point de montage (ex: "/", "/var", "/home").
    pub mount: String,
    /// Espace utilisé en gigaoctets.
    pub used_gb: f64,
    /// Capacité totale en gigaoctets.
    pub total_gb: f64,
}

impl SystemMonitorState {
    pub fn new() -> Self {
        Self {
            cpu_history: vec![0.0; HISTORY_LEN],
            ram_history: vec![0.0; HISTORY_LEN],
            cpu_percent: 0.0,
            ram_used_mb: 0,
            ram_total_mb: 0,
            load_avg: [0.0; 3],
            uptime_secs: 0,
            disk_info: Vec::new(),
            refresh_secs: 5.0,
            last_refresh: 0.0,
        }
    }

    /// Ajoute un nouvel échantillon CPU/RAM dans les historiques (mode FIFO).
    pub fn push_sample(&mut self, cpu: f64, ram_pct: f64) {
        self.cpu_history.push(cpu);
        self.ram_history.push(ram_pct);
        // Purge les valeurs les plus anciennes pour garder HISTORY_LEN points.
        if self.cpu_history.len() > HISTORY_LEN { self.cpu_history.remove(0); }
        if self.ram_history.len() > HISTORY_LEN { self.ram_history.remove(0); }
        self.cpu_percent = cpu;
    }
}

// ─── Rendu egui ──────────────────────────────────────────────────────────────

pub fn render(state: &mut SystemMonitorState, ui: &mut Ui) {
    // ── En-tête avec métriques textuelles ─────────────────────────────────────
    ui.horizontal(|ui| {
        ui.heading("📊 Moniteur système");
        ui.separator();

        // Uptime formaté en jours/heures/minutes.
        let days  = state.uptime_secs / 86_400;
        let hours = (state.uptime_secs % 86_400) / 3_600;
        let mins  = (state.uptime_secs % 3_600) / 60;
        ui.label(format!("⏱ Uptime : {days}j {hours}h {mins}m"));
        ui.separator();

        // Load average (analogie avec `uptime` Linux).
        ui.label(format!(
            "⚡ Load : {:.2} {:.2} {:.2}",
            state.load_avg[0], state.load_avg[1], state.load_avg[2]
        ));

        // Sélecteur de fréquence de rafraîchissement à droite.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label("Refresh :");
            egui::ComboBox::new("refresh_rate_combo", "")
                .selected_text(format!("{}s", state.refresh_secs as u32))
                .show_ui(ui, |ui| {
                    for s in [1.0_f64, 5.0, 10.0] {
                        ui.selectable_value(&mut state.refresh_secs, s, format!("{s}s"));
                    }
                });
        });
    });

    ui.separator();

    // ── Graphes CPU et RAM côte à côte ────────────────────────────────────────
    ui.columns(2, |cols| {
        // ── Graphe CPU ───────────────────────────────────────────────────────
        let cpu_pts: PlotPoints = state
            .cpu_history
            .iter()
            .enumerate()
            .map(|(i, &v)| [i as f64, v])
            .collect();

        cols[0].label(
            egui::RichText::new(format!("CPU : {:.1}%", state.cpu_percent))
                .strong()
                .color(egui::Color32::from_rgb(70, 130, 180)),
        );
        Plot::new("cpu_plot")
            .height(110.0)
            .include_y(0.0)
            .include_y(100.0)
            .show_axes([false, true])
            .show(&mut cols[0], |plot_ui| {
                plot_ui.line(
                    Line::new(cpu_pts)
                        .color(egui::Color32::from_rgb(70, 130, 180))
                        .name("CPU %"),
                );
            });

        // ── Graphe RAM ───────────────────────────────────────────────────────
        let ram_pct = if state.ram_total_mb > 0 {
            state.ram_used_mb as f64 / state.ram_total_mb as f64 * 100.0
        } else {
            0.0
        };
        let ram_pts: PlotPoints = state
            .ram_history
            .iter()
            .enumerate()
            .map(|(i, &v)| [i as f64, v])
            .collect();

        cols[1].label(
            egui::RichText::new(format!(
                "RAM : {}/{} Mo ({:.1}%)",
                state.ram_used_mb, state.ram_total_mb, ram_pct
            ))
            .strong()
            .color(egui::Color32::from_rgb(180, 70, 130)),
        );
        Plot::new("ram_plot")
            .height(110.0)
            .include_y(0.0)
            .include_y(100.0)
            .show_axes([false, true])
            .show(&mut cols[1], |plot_ui| {
                plot_ui.line(
                    Line::new(ram_pts)
                        .color(egui::Color32::from_rgb(180, 70, 130))
                        .name("RAM %"),
                );
            });
    });

    // ── Utilisation des disques ───────────────────────────────────────────────
    if !state.disk_info.is_empty() {
        ui.separator();
        ui.label(egui::RichText::new("💽 Disques").strong());
        for disk in &state.disk_info {
            let pct = (disk.used_gb / disk.total_gb) as f32;
            // Couleur de la barre : vert < 70 %, orange < 90 %, rouge sinon.
            let color = if pct < 0.7 {
                egui::Color32::from_rgb(80, 180, 80)
            } else if pct < 0.9 {
                egui::Color32::from_rgb(220, 160, 30)
            } else {
                egui::Color32::from_rgb(220, 60, 60)
            };
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{:<20}", disk.mount)).monospace(),
                );
                ui.add(
                    egui::ProgressBar::new(pct)
                        .text(format!("{:.1}/{:.1} Go", disk.used_gb, disk.total_gb))
                        .desired_width(220.0)
                        .fill(color),
                );
            });
        }
    }
}
