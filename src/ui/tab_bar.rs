/// Barre d'onglets en haut de la zone centrale.
/// Chaque onglet représente une session SSH avec un indicateur de connexion.
use crate::app::BetterSshApp;
use egui::Ui;

pub fn render(app: &mut BetterSshApp, ui: &mut Ui) {
    let mut to_close: Option<usize> = None;

    ui.horizontal(|ui| {
        for (i, tab) in app.tabs.iter().enumerate() {
            // Indicateur coloré : ● vert = connecté, ● rouge = déconnecté.
            let indicator = if tab.connected {
                egui::RichText::new("●").color(egui::Color32::from_rgb(80, 200, 80))
            } else {
                egui::RichText::new("●").color(egui::Color32::from_rgb(200, 80, 80))
            };

            let selected = i == app.active_tab;

            ui.horizontal(|ui| {
                ui.label(indicator);
                // Clic sur le nom → active l'onglet.
                if ui.selectable_label(selected, tab.profile.display_name()).clicked() {
                    app.active_tab = i;
                }
                // Bouton de fermeture discret.
                if ui.small_button("X")
                    .on_hover_text("Fermer la session (Ctrl+W)")
                    .clicked()
                {
                    to_close = Some(i);
                }
            });

            ui.separator();
        }

        // Bouton d'ajout d'onglet → ouvre le dialogue de nouveau profil.
        if ui.button("+").on_hover_text("Nouvelle connexion (Ctrl+T)").clicked() {
            app.sidebar.show_new_profile = true;
        }
    });

    // Fermeture hors de la boucle pour éviter un borrow mutable pendant l'itération.
    if let Some(idx) = to_close {
        app.close_tab(idx);
    }
}
