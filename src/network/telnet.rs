/// Session Telnet basée sur une connexion TCP brute.
/// Réutilise SessionCommand et SessionEvent de la session SSH pour s'intégrer
/// dans le même système d'onglets sans modifier la logique de l'UI.
use anyhow::Context;
use crossbeam_channel::{Receiver, Sender};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

use crate::ssh::session::{SessionCommand, SessionEvent};

// ─── Session publique ─────────────────────────────────────────────────────────

/// Connexion Telnet : wrapping des canaux de communication vers la task async.
/// Expose la même interface que SshSession pour une intégration transparente.
pub struct TelnetSession {
    pub cmd_tx: Sender<SessionCommand>,
    pub event_rx: Receiver<SessionEvent>,
}

impl TelnetSession {
    /// Établit une connexion Telnet vers `host:port` en arrière-plan.
    pub fn connect(host: String, port: u16) -> Self {
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded::<SessionCommand>();
        let (event_tx, event_rx) = crossbeam_channel::unbounded::<SessionEvent>();

        let etx = event_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = run_telnet(host, port, cmd_rx, etx.clone()).await {
                let _ = etx.send(SessionEvent::Error(e.to_string()));
            }
        });

        Self { cmd_tx, event_rx }
    }

    /// Envoie des octets bruts vers le serveur Telnet.
    pub fn send_input(&self, data: Vec<u8>) {
        let _ = self.cmd_tx.send(SessionCommand::SendInput(data));
    }

    /// Tente de lire un événement sans bloquer.
    pub fn try_recv(&self) -> Option<SessionEvent> {
        self.event_rx.try_recv().ok()
    }

    /// Ferme la connexion.
    pub fn disconnect(&self) {
        let _ = self.cmd_tx.send(SessionCommand::Disconnect);
    }
}

// ─── Boucle de connexion async ────────────────────────────────────────────────

async fn run_telnet(
    host: String,
    port: u16,
    cmd_rx: Receiver<SessionCommand>,
    event_tx: Sender<SessionEvent>,
) -> anyhow::Result<()> {
    let addr = format!("{}:{}", host, port);
    let mut stream = timeout(Duration::from_secs(30), TcpStream::connect(&addr))
        .await
        .context("timeout de connexion Telnet")?
        .context("connexion TCP échouée")?;

    let _ = event_tx.send(SessionEvent::Connected);

    let mut buf = [0u8; 4096];
    let mut iac_state = IacState::Normal;

    loop {
        // Traite les commandes de l'UI sans bloquer.
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                SessionCommand::SendInput(data) => {
                    stream.write_all(&data).await?;
                }
                SessionCommand::Disconnect => {
                    let _ = event_tx.send(SessionEvent::Disconnected(
                        "Déconnecté par l'utilisateur".into(),
                    ));
                    return Ok(());
                }
                // Resize et SftpUpload ne s'appliquent pas au Telnet.
                _ => {}
            }
        }

        // Lecture des données réseau avec timeout pour rester réactif aux commandes UI.
        match timeout(Duration::from_millis(50), stream.read(&mut buf)).await {
            Err(_) => continue,
            Ok(Ok(0)) => {
                let _ = event_tx.send(SessionEvent::Disconnected("Connexion fermée".into()));
                return Ok(());
            }
            Ok(Ok(n)) => {
                let filtered = filter_iac(&buf[..n], &mut iac_state);
                if !filtered.is_empty() {
                    let _ = event_tx.send(SessionEvent::Data(filtered));
                }
            }
            Ok(Err(e)) => return Err(e.into()),
        }
    }
}

// ─── Filtrage des séquences IAC ───────────────────────────────────────────────

/// Machine à états pour le décodage des séquences IAC du protocole Telnet.
enum IacState {
    Normal,
    /// Après 0xFF : attend le code de commande.
    Iac,
    /// Après WILL/WONT/DO/DONT (0xFB–0xFE) : attend le code d'option.
    IacOption,
}

/// Supprime les séquences IAC du flux reçu pour n'afficher que le texte utile.
/// 0xFF 0xFF → octet littéral 0xFF (séquence d'échappement IAC).
/// 0xFF 0xFB–0xFE X → séquence de négociation 3 octets → supprimée.
/// 0xFF X → commande 2 octets → supprimée.
fn filter_iac(data: &[u8], state: &mut IacState) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for &b in data {
        match state {
            IacState::Normal => {
                if b == 0xFF {
                    *state = IacState::Iac;
                } else {
                    out.push(b);
                }
            }
            IacState::Iac => {
                match b {
                    0xFF => {
                        out.push(0xFF); // IAC IAC = octet 0xFF littéral
                        *state = IacState::Normal;
                    }
                    0xFB..=0xFE => {
                        *state = IacState::IacOption; // WILL/WONT/DO/DONT
                    }
                    _ => {
                        *state = IacState::Normal; // commande 2 octets inconnue
                    }
                }
            }
            IacState::IacOption => {
                *state = IacState::Normal; // consomme le code d'option
            }
        }
    }
    out
}
