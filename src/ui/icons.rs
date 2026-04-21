/// Icônes Phosphor pour l'interface graphique.
///
/// Toutes les icônes utilisent la police Phosphor (zone Unicode privée, PUA),
/// indépendante de la police de texte choisie par l'utilisateur.
/// Importer ce module avec `use crate::ui::icons as ph;` puis `ph::FOLDER`.

// Re-export des constantes Phosphor (variante Regular).
pub use egui_phosphor::regular::*;

use egui::{FontId, RichText};

/// Renvoie un `RichText` à la taille de corps standard (14 px).
#[inline]
pub fn icon(code: &str) -> RichText {
    RichText::new(code)
}

/// Renvoie un `RichText` à taille personnalisée.
#[inline]
pub fn icon_sized(code: &str, size: f32) -> RichText {
    RichText::new(code).size(size)
}

/// Enregistre la police Phosphor dans les définitions de polices egui.
/// À appeler dans `setup_fonts` avant `ctx.set_fonts(fonts)`.
pub fn install(fonts: &mut egui::FontDefinitions) {
    egui_phosphor::add_to_fonts(fonts, egui_phosphor::Variant::Regular);
}
