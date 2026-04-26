/// Fenêtre de sélection de langue (🌐).
/// Liste toutes les langues disponibles (embarquées + fichiers sur disque).
use crate::app::BetterSshApp;
use crate::app::LangRepoStatus;
use crate::i18n;
use egui::Context;

pub fn render(app: &mut BetterSshApp, ctx: &Context) {
    app.ensure_remote_langs_loaded();

    let mut open = app.show_lang_window;
    let mut close_requested = false;
    let title = app.lang.lang_win_title.clone();
    let lang_dir = app.work_dir.join("lang");
    let rows = merged_lang_rows(app);
    let (status_text, status_color) = repo_status_line(app);
    let active_name = app.lang_files.iter()
        .find(|file| file.stem == app.lang_chosen)
        .map(|file| file.name.clone())
        .unwrap_or_else(|| app.lang_chosen.clone());

    egui::Window::new(&title)
        .open(&mut open)
        .resizable(false)
        .fixed_size([520.0, 420.0])
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(&app.lang.lang_active_label)
                    .small()
                    .weak(),
            );
            ui.label(egui::RichText::new(active_name).strong());
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.colored_label(status_color, egui::RichText::new(status_text).small());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(&app.lang.lang_refresh_btn)
                    .on_hover_text(&app.lang.lang_refresh_btn_hint)
                    .clicked()
                    {
                        app.refresh_remote_langs();
                    }
                });
            });

            ui.separator();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(240.0)
                .show(ui, |ui| {
                    if rows.is_empty() {
                        ui.label(
                            egui::RichText::new(&app.lang.lang_local_empty)
                                .small()
                                .weak(),
                        );
                    }

                    for row in &rows {
                        let is_active = row.stem == app.lang_chosen;
                        let status = app.lang_repo_status.clone();
                        let busy = matches!(status, LangRepoStatus::Loading | LangRepoStatus::Downloading(_));
                        let downloading_this = matches!(status, LangRepoStatus::Downloading(ref stem) if stem == &row.stem);

                        let row_resp = egui::Frame::none()
                            .fill(if is_active {
                                ui.visuals().selection.bg_fill
                            } else {
                                egui::Color32::TRANSPARENT
                            })
                            .inner_margin(egui::Margin::symmetric(6.0, 4.0))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(&row.name)
                                            .strong_if(is_active),
                                    );
                                    if row.is_default {
                                        ui.label(egui::RichText::new(&app.lang.lang_badge_default).small().weak());
                                    }
                                    if row.is_builtin {
                                        ui.label(egui::RichText::new(&app.lang.lang_builtin_badge).small().weak());
                                    }
                                    if row.is_local {
                                        ui.label(egui::RichText::new(&app.lang.lang_local_badge).small().weak());
                                    }
                                    if row.is_downloaded {
                                        ui.label(egui::RichText::new(&app.lang.lang_downloaded_badge).small().weak());
                                    }
                                    if row.is_repo {
                                        ui.label(egui::RichText::new(&app.lang.lang_remote_badge).small().weak());
                                    }
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(
                                            egui::RichText::new(&row.lang_code)
                                                .monospace()
                                                .small()
                                                .weak(),
                                        );
                                    });
                                })
                                .response
                            })
                            .inner;

                        if row_resp.clicked() && !busy {
                            if (row.is_available || row.is_repo) && !downloading_this {
                                app.select_lang(row.stem.clone());
                            }
                        }

                        ui.separator();
                    }
                });

            ui.separator();
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(&app.lang.lang_local_path_label)
                    .small()
                    .weak(),
            );
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(lang_dir.display().to_string())
                        .monospace()
                        .small(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(&app.lang.lang_open_btn)
                        .on_hover_text(&app.lang.lang_open_btn_hint)
                        .clicked()
                    {
                        let _ = std::fs::create_dir_all(&lang_dir);
                        open_in_explorer(&lang_dir);
                    }
                });
            });

            ui.separator();
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(&app.lang.lang_close_btn).clicked() {
                        close_requested = true;
                    }
                });
            });
        });

    if close_requested {
        open = false;
    }
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

#[derive(Clone)]
struct LangRow {
    stem: String,
    name: String,
    lang_code: String,
    is_available: bool,
    is_builtin: bool,
    is_local: bool,
    is_downloaded: bool,
    is_repo: bool,
    is_default: bool,
}

fn merged_lang_rows(app: &BetterSshApp) -> Vec<LangRow> {
    let mut rows: Vec<LangRow> = app.lang_files.iter().map(|file| LangRow {
        stem: file.stem.clone(),
        name: file.name.clone(),
        lang_code: file.lang_code.clone(),
        is_available: true,
        is_builtin: !file.from_disk,
        is_local: file.from_disk,
        is_downloaded: false,
        is_repo: false,
        is_default: file.stem == i18n::detect_system_lang_stem(),
    }).collect();

    for remote in &app.remote_lang_files {
        if let Some(existing) = rows.iter_mut().find(|row| row.stem == remote.stem) {
            existing.is_repo = true;
            if existing.is_local {
                existing.is_downloaded = true;
            }
        } else {
            rows.push(LangRow {
                stem: remote.stem.clone(),
                name: remote.name.clone(),
                lang_code: remote.lang_code.clone(),
                is_available: false,
                is_builtin: false,
                is_local: false,
                is_downloaded: false,
                is_repo: true,
                is_default: remote.stem == i18n::detect_system_lang_stem(),
            });
        }
    }

    rows.sort_by(|a, b| a.name.cmp(&b.name));
    rows
}

fn repo_status_line(app: &BetterSshApp) -> (String, egui::Color32) {
    match &app.lang_repo_status {
        LangRepoStatus::Idle => (
            app.lang.lang_status_ready.clone(),
            egui::Color32::from_rgb(120, 120, 120),
        ),
        LangRepoStatus::Loading => (
            app.lang.lang_remote_loading.clone(),
            egui::Color32::from_rgb(80, 160, 220),
        ),
        LangRepoStatus::Offline => (
            app.lang.lang_status_offline.clone(),
            egui::Color32::from_rgb(220, 160, 80),
        ),
        LangRepoStatus::Error(err) => (
            format!("{}: {}", app.lang.lang_remote_error, err),
            egui::Color32::from_rgb(220, 80, 80),
        ),
        LangRepoStatus::Downloading(stem) => (
            format!("{} {stem}...", app.lang.lang_status_installing),
            egui::Color32::from_rgb(80, 160, 220),
        ),
        LangRepoStatus::Installed(stem) => (
            format!("{} {stem}", app.lang.lang_status_installed),
            egui::Color32::from_rgb(80, 180, 100),
        ),
    }
}
