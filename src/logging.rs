/// Système de journalisation fichier pour le débogage.
/// Ne journalise jamais : mots de passe, clés vault, contenu chiffré.
use anyhow::Result;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024; // 5 MB
const KEEP_BYTES: u64 = 2 * 1024 * 1024;    // garder les 2 derniers MB

pub fn setup(log_path: PathBuf) -> Result<()> {
    truncate_if_needed(&log_path);

    let log_file = fern::log_file(&log_path)?;

    let mut dispatch = fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{}] [{:<5}] [{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                record.level(),
                record.target(),
                message
            ))
        })
        // Filtre global : debug+ pour notre code, warn+ pour les dépendances bruyantes
        .level(log::LevelFilter::Warn)
        .level_for("betterssh", log::LevelFilter::Debug)
        .level_for("russh", log::LevelFilter::Warn)
        .level_for("russh_sftp", log::LevelFilter::Warn)
        .level_for("eframe", log::LevelFilter::Warn)
        .level_for("egui", log::LevelFilter::Warn)
        .chain(log_file);

    // En mode debug, on garde aussi la sortie stderr
    #[cfg(debug_assertions)]
    {
        dispatch = dispatch.chain(std::io::stderr());
    }

    dispatch.apply()?;
    log::info!("=== BetterSSH démarré — journal : {} ===", log_path.display());
    Ok(())
}

fn truncate_if_needed(path: &PathBuf) {
    let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if size <= MAX_LOG_BYTES {
        return;
    }

    // Lire les derniers KEEP_BYTES octets et réécrire le fichier
    if let Ok(data) = fs::read(path) {
        let start = data.len().saturating_sub(KEEP_BYTES as usize);
        // Trouver le début de la prochaine ligne pour éviter un enregistrement tronqué
        let start = data[start..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|p| start + p + 1)
            .unwrap_or(start);

        let mut header = format!(
            "[{}] [INFO ] [betterssh::logging] --- journal tronqué (fichier > 5 MB) ---\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f")
        )
        .into_bytes();
        header.extend_from_slice(&data[start..]);

        let _ = fs::write(path, header);
    }
}
