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
        .fixed_size([540.0, 430.0])
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{}:", app.lang.lang_active_label))
                        .small()
                        .weak(),
                );
                ui.label(egui::RichText::new(active_name).strong());
            });
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("●")
                        .color(status_color)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(status_text)
                        .small()
                        .color(status_color),
                );
            });

            ui.separator();
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{}:", app.lang.lang_local_section))
                        .small()
                        .weak(),
                );
                if ui.small_button(&app.lang.lang_refresh_btn)
                    .on_hover_text(&app.lang.lang_refresh_btn_hint)
                    .clicked()
                {
                    app.refresh_remote_langs();
                }
            });
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(245.0)
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
                        let downloading_this = matches!(status, LangRepoStatus::Downloading(ref stem) if stem == &row.stem);
                        let can_select_local = row.is_available && !downloading_this;
                        let can_select_remote = row.is_repo
                            && !row.is_available
                            && !matches!(status, LangRepoStatus::Loading | LangRepoStatus::Downloading(_));
                        let mut row_text = row.name.clone();

                        if row.is_default {
                            row_text.push(' ');
                            row_text.push_str(&app.lang.lang_badge_default);
                        }
                        if row.is_local {
                            row_text.push(' ');
                            row_text.push_str(&app.lang.lang_local_badge);
                        }
                        if row.is_repo {
                            row_text.push(' ');
                            row_text.push_str(&app.lang.lang_remote_badge);
                        }
                        if !row.lang_code.is_empty() {
                            row_text.push_str("  ");
                            row_text.push_str(&row.lang_code);
                        }

                        let desired_size = egui::vec2(ui.available_width(), 24.0);
                        let (rect, row_resp) = ui.allocate_exact_size(desired_size, egui::Sense::click());
                        let visuals = ui.style().interact_selectable(&row_resp, is_active);
                        let text_color = if is_active {
                            visuals.text_color()
                        } else {
                            ui.visuals().text_color()
                        };

                        if ui.is_rect_visible(rect) {
                            ui.painter().rect(
                                rect,
                                visuals.rounding,
                                visuals.bg_fill,
                                visuals.bg_stroke,
                            );
                            let text_pos = egui::pos2(rect.left() + 6.0, rect.center().y);
                            ui.painter().text(
                                text_pos,
                                egui::Align2::LEFT_CENTER,
                                row_text,
                                egui::TextStyle::Small.resolve(ui.style()),
                                text_color,
                            );
                        }

                        if row_resp.clicked() {
                            if can_select_local || can_select_remote {
                                app.select_lang(row.stem.clone());
                            }
                        }
                    }
                });

            ui.separator();
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{}:", app.lang.lang_local_path_label))
                        .small()
                        .weak(),
                );
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(lang_dir.display().to_string())
                            .monospace()
                            .small()
                            .weak(),
                    )
                    .truncate(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button(&app.lang.lang_open_btn)
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
    is_local: bool,
    is_repo: bool,
    is_default: bool,
}

fn merged_lang_rows(app: &BetterSshApp) -> Vec<LangRow> {
    let mut rows: Vec<LangRow> = app.lang_files.iter().map(|file| LangRow {
        stem: file.stem.clone(),
        name: file.name.clone(),
        lang_code: file.lang_code.clone(),
        is_available: true,
        is_local: file.from_disk,
        is_repo: false,
        is_default: file.stem == i18n::detect_system_lang_stem(),
    }).collect();

    for remote in &app.remote_lang_files {
        if let Some(existing) = rows.iter_mut().find(|row| row.stem == remote.stem) {
            existing.is_repo = true;
        } else {
            rows.push(LangRow {
                stem: remote.stem.clone(),
                name: remote.name.clone(),
                lang_code: remote.lang_code.clone(),
                is_available: false,
                is_local: false,
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
            format!("GitHub - {} language(s) repo", app.remote_lang_files.len()),
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
