//! Icônes Phosphor de l'interface, fournies par rusty_kit (charte Rusty Suite).
//!
//! Importer ce module avec `use crate::ui::icons as ph;` puis `ph::FOLDER`.
//! La police de texte de la charte (Inter) possède aussi des glyphes dans la zone privée
//! d'Unicode : une icône doit toujours être dessinée avec la police d'icônes du kit.
//!   - icône seule : `ph::rich(ph::FOLDER, 16.0)` ;
//!   - icône + texte : `ph::with_text(ph::FOLDER, &texte)` ;
//!   - icône + texte coloré (états, alertes) : `ph::colored(ph::WARNING, &texte, 12.0, couleur)`.
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, WidgetText};

pub use rusty_ui::icons::phosphor::*;
pub use rusty_ui::icons::{rich, with_text};

/// Taille des icônes seules dans les boutons et libellés courants.
pub const SIZE: f32 = 16.0;

/// Icône suivie d'un texte, tous deux de la couleur donnée.
pub fn colored(icon: &str, text: &str, size: f32, color: Color32) -> WidgetText {
    let mut job = LayoutJob::default();
    job.append(icon, 0.0, TextFormat { font_id: rusty_ui::icons::font(size), color, ..Default::default() });
    job.append(&format!(" {text}"), 0.0, TextFormat { font_id: FontId::proportional(size), color, ..Default::default() });
    job.into()
}
