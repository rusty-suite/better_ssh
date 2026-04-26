/// Fenêtre de sélection de langue (🌐).
/// Liste toutes les langues disponibles (embarquées + fichiers sur disque).
use crate::app::BetterSshApp;
use crate::i18n;
use egui::Context;

pub fn render(app: &mut BetterSshApp, ctx: &Context) {
    let mut open = app.show_lang_window;
    let title = app.lang.lang_win_title.clone();

    egui::Window::new(&title)
        .open(&mut open)
        .fixed_size([420.0, 300.0])
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            // Bouton "Ouvrir le dossier lang\"
            ui.horizontal(|ui| {
                if ui.button(&app.lang.lang_open_btn)
                    .on_hover_text(&app.lang.lang_open_btn_hint)
                    .clicked()
                {
                    let lang_dir = app.work_dir.join("lang");
                    let _ = std::fs::create_dir_all(&lang_dir);
                    open_in_explorer(&lang_dir);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(
                            egui::special_emojis::GITHUB.to_string()
                                + " rusty-suite/better_ssh"
                        )
                        .small()
                        .weak(),
                    );
                });
            });

            ui.separator();

            let system_stem = i18n::detect_system_lang_stem();

            egui::ScrollArea::vertical().show(ui, |ui| {
                let files = app.lang_files.clone();
                for file in &files {
                    let is_active  = file.stem == app.lang_chosen;
                    let is_default = file.stem == system_stem;

                    let row_resp = egui::Frame::none()
                        .fill(if is_active {
                            ui.visuals().selection.bg_fill
                        } else {
                            egui::Color32::TRANSPARENT
                        })
                        .inner_margin(egui::Margin::symmetric(6.0, 3.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                // Nom
                                let label = egui::RichText::new(&file.name)
                                    .strong_if(is_active);
                                ui.label(label);

                                // Badge [défaut]
                                if is_default {
                                    ui.label(
                                        egui::RichText::new(&app.lang.lang_badge_default)
                                            .small()
                                            .color(egui::Color32::from_rgb(80, 180, 100)),
                                    );
                                }

                                // Badge [Actif]
                                if is_active {
                                    ui.label(
                                        egui::RichText::new(&app.lang.lang_active_label)
                                            .small()
                                            .color(egui::Color32::from_rgb(80, 160, 220)),
                                    );
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            egui::RichText::new(&file.lang_code)
                                                .monospace()
                                                .small()
                                                .weak(),
                                        );
                                    },
                                );
                            })
                            .response
                        })
                        .inner;

                    // Clic sur la ligne → active la langue
                    let full_rect = {
                        let r = row_resp.rect;
                        egui::Rect::from_min_size(
                            egui::pos2(0.0, r.min.y),
                            egui::vec2(ui.available_width(), r.height()),
                        )
                    };
                    let _ = full_rect; // rect étendu non utilisé ici
                    if row_resp.clicked() && !is_active {
                        let stem = file.stem.clone();
                        app.reload_lang(&stem);
                    }

                    ui.add_space(2.0);
                }

                ui.add_space(8.0);
                ui.separator();

                // Message si seule l'EN embarquée est disponible et aucune n'est sur disque
                let has_disk = files.iter().any(|f| f.from_disk);
                if !has_disk {
                    ui.label(
                        egui::RichText::new(&app.lang.lang_no_internet_msg)
                            .small()
                            .weak()
                            .italics(),
                    );
                }
            });
        });

    app.show_lang_window = open;
}

/// Ouvre le répertoire dans l'explorateur de fichiers natif.
fn open_in_explorer(path: &std::path::Path) {
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

// ─── Extension RichText pour strong_if ───────────────────────────────────────

trait RichTextExt {
    fn strong_if(self, cond: bool) -> Self;
}

impl RichTextExt for egui::RichText {
    fn strong_if(self, cond: bool) -> Self {
        if cond { self.strong() } else { self }
    }
}
