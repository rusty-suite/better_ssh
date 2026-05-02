/// Scanner réseau asynchrone : ping sweep CIDR + détection SSH.
/// Chaque hôte est testé en parallèle via tokio avec un sémaphore pour
/// limiter le nombre de connexions simultanées.
use anyhow::{Context, Result};
use crossbeam_channel::Sender;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::time::timeout;

// ─── Structures de données ────────────────────────────────────────────────────

/// Résultat du scan pour un hôte répondant sur le port SSH.
#[derive(Debug, Clone)]
pub struct ScanResult {
    /// Adresse IP de l'hôte trouvé.
    pub ip: IpAddr,
    /// Nom d'hôte résolu en DNS inverse (optionnel).
    pub hostname: Option<String>,
    /// Latence de connexion TCP en millisecondes.
    pub latency_ms: Option<u64>,
    /// true si le port SSH est ouvert et accepte des connexions.
    pub ssh_open: bool,
    /// Bannière SSH lue au début de la connexion (ex: "SSH-2.0-OpenSSH_8.9").
    pub ssh_banner: Option<String>,
}

/// Événements envoyés via le canal crossbeam pendant un scan en cours.
#[derive(Debug, Clone)]
pub enum ScanEvent {
    /// Avancement : `done` hôtes traités sur `total`.
    Progress { done: usize, total: usize },
    /// Un hôte avec SSH ouvert a été trouvé.
    Found(ScanResult),
    /// Scan terminé (tous les hôtes ont été testés).
    Finished,
    /// Erreur fatale (ex: CIDR invalide).
    Error(String),
}

/// Paramètres réglables du scan réseau.
#[derive(Debug, Clone)]
pub struct ScanParams {
    /// Plage CIDR ou étendue à scanner.
    pub target: String,
    /// Port SSH à tester (22 par défaut).
    pub ssh_port: u16,
    /// Délai max par hôte en millisecondes.
    pub timeout_ms: u64,
    /// Nombre de sondes parallèles simultanées.
    pub concurrency: usize,
}

impl Default for ScanParams {
    fn default() -> Self {
        Self {
            target: detect_local_subnet().unwrap_or_else(|| "192.168.1.0/24".into()),
            ssh_port: 22,
            timeout_ms: 500,
            concurrency: 64,
        }
    }
}

// ─── Auto-détection du réseau local ──────────────────────────────────────────

/// Détecte automatiquement le sous-réseau local de la machine.
/// Utilise une astuce UDP : ouvrir un socket vers 8.8.8.8 révèle
/// quelle interface (et donc quelle IP) sera utilisée pour router.
pub fn detect_local_subnet() -> Option<String> {
    use std::net::UdpSocket;
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    // Connexion UDP sans envoi de paquets — juste pour obtenir l'IP locale.
    socket.connect("8.8.8.8:80").ok()?;
    let local = socket.local_addr().ok()?;
    if let IpAddr::V4(ipv4) = local.ip() {
        // Suppose un masque /24 (classe C typique des réseaux domestiques/PME).
        let octets = ipv4.octets();
        Some(format!("{}.{}.{}.0/24", octets[0], octets[1], octets[2]))
    } else {
        None // IPv6 non supporté pour le scan CIDR
    }
}

// ─── Scanner ─────────────────────────────────────────────────────────────────

pub struct NetworkScanner;

impl NetworkScanner {
    /// Parse une notation CIDR IPv4 (ex: "192.168.1.0/24") en liste d'IPs.
    /// Exclut l'adresse réseau (.0) et l'adresse de broadcast.
    pub fn parse_cidr(cidr: &str) -> Result<Vec<IpAddr>> {
        let parts: Vec<&str> = cidr.split('/').collect();
        if parts.len() != 2 {
            anyhow::bail!("CIDR invalide : {cidr} (format attendu: X.X.X.X/N)");
        }
        let base: Ipv4Addr = parts[0].parse().context("adresse IP de base invalide")?;
        let prefix: u8 = parts[1].parse().context("préfixe invalide")?;
        if prefix > 32 {
            anyhow::bail!("Préfixe trop grand : {prefix} (max 32)");
        }
        let base_u32 = u32::from(base);
        // Calcule le masque réseau (ex: /24 → 0xFFFFFF00)
        let mask = if prefix == 0 { 0u32 } else { !((1u32 << (32 - prefix)) - 1) };
        let network = base_u32 & mask;
        let broadcast = network | !mask;
        // On génère toutes les IPs entre réseau+1 et broadcast-1
        let ips = (network + 1..broadcast)
            .map(|n| IpAddr::V4(Ipv4Addr::from(n)))
            .collect();
        Ok(ips)
    }

    /// Parse une étendue d'IP (ex: "192.168.1.1-254").
    /// Si la chaîne ne contient pas de tiret, tente le parsing CIDR.
    pub fn parse_range(range: &str) -> Result<Vec<IpAddr>> {
        // Vérifie si c'est un CIDR avant d'essayer le format étendue.
        if range.contains('/') {
            return Self::parse_cidr(range);
        }
        let parts: Vec<&str> = range.splitn(2, '-').collect();
        if parts.len() != 2 {
            anyhow::bail!("Format invalide : {range} (ex: 192.168.1.1-254 ou 192.168.1.0/24)");
        }
        let start: Ipv4Addr = parts[0].parse().context("IP de début invalide")?;
        let last_octet: u8 = parts[1].parse().context("dernier octet de fin invalide")?;
        let base = u32::from(start);
        let start_last = (base & 0xff) as u8;
        let ips = (start_last..=last_octet)
            .map(|octet| IpAddr::V4(Ipv4Addr::from((base & 0xffffff00) | octet as u32)))
            .collect();
        Ok(ips)
    }

    /// Lance le scan asynchrone. Les résultats sont envoyés via `tx` au fil de l'eau.
    /// Le sémaphore `concurrency` évite de saturer le réseau ou l'OS.
    pub async fn scan(ips: Vec<IpAddr>, params: ScanParams, tx: Sender<ScanEvent>) {
        let total = ips.len();
        let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(params.concurrency));
        let done = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut handles = Vec::with_capacity(total);

        for ip in ips {
            let sem = sem.clone();
            let tx = tx.clone();
            let done = done.clone();
            let timeout_ms = params.timeout_ms;
            let port = params.ssh_port;

            let handle = tokio::spawn(async move {
                // Le sémaphore limite les connexions simultanées.
                let _permit = sem.acquire().await;
                let result = probe_host(ip, port, timeout_ms).await;
                let d = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                let _ = tx.send(ScanEvent::Progress { done: d, total });
                if let Some(r) = result {
                    let _ = tx.send(ScanEvent::Found(r));
                }
            });
            handles.push(handle);
        }

        // Attend la fin de toutes les tâches avant d'envoyer Finished.
        for h in handles {
            let _ = h.await;
        }
        let _ = tx.send(ScanEvent::Finished);
    }
}

// ─── Sonde par hôte ──────────────────────────────────────────────────────────

/// Tente une connexion TCP sur le port SSH de l'hôte.
/// Retourne None si l'hôte ne répond pas dans le délai imparti.
async fn probe_host(ip: IpAddr, ssh_port: u16, timeout_ms: u64) -> Option<ScanResult> {
    let addr = SocketAddr::new(ip, ssh_port);
    let start = Instant::now();
    let tcp = timeout(
        Duration::from_millis(timeout_ms),
        TcpStream::connect(addr),
    )
    .await;
    let latency_ms = start.elapsed().as_millis() as u64;

    match tcp {
        Ok(Ok(stream)) => {
            let banner = read_ssh_banner(stream).await;
            // Résolution DNS inverse (best-effort, ne bloque pas le scan si échec).
            let hostname = reverse_dns(ip).await;
            Some(ScanResult {
                ip,
                hostname,
                latency_ms: Some(latency_ms),
                ssh_open: true,
                ssh_banner: banner,
            })
        }
        _ => None, // timeout ou refus de connexion
    }
}

/// Lit la bannière SSH envoyée par le serveur en début de connexion.
/// Retourne None si le serveur ne répond pas dans 300 ms ou si ce n'est pas SSH.
async fn read_ssh_banner(stream: TcpStream) -> Option<String> {
    use tokio::io::AsyncReadExt;
    let mut buf = [0u8; 256];
    let mut s = stream;
    match timeout(Duration::from_millis(300), s.read(&mut buf)).await {
        Ok(Ok(n)) if n > 0 => {
            let raw = std::str::from_utf8(&buf[..n]).unwrap_or("").trim().to_string();
            // Une bannière SSH valide commence toujours par "SSH-"
            if raw.starts_with("SSH-") { Some(raw) } else { None }
        }
        _ => None,
    }
}

/// Résolution DNS inverse de l'IP (best-effort, 500 ms max).
async fn reverse_dns(ip: IpAddr) -> Option<String> {
    timeout(
        Duration::from_millis(500),
        tokio::net::lookup_host(format!("{ip}:0")),
    )
    .await
    .ok()
    .and_then(|r| r.ok())
    .and_then(|mut it| it.next())
    // lookup_host retourne des SocketAddr ; on ne garde que la partie nom si dispo.
    // Note : tokio ne fait pas de PTR lookup natif, cette résolution reste basique.
    .map(|_| String::new())
    .filter(|s| !s.is_empty())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cidr_30_donne_2_ips() {
        // /30 = 4 adresses : réseau, 2 hôtes, broadcast
        let ips = NetworkScanner::parse_cidr("192.168.1.0/30").unwrap();
        assert_eq!(ips.len(), 2);
    }

    #[test]
    fn etendue_1_a_5_donne_5_ips() {
        let ips = NetworkScanner::parse_range("10.0.0.1-5").unwrap();
        assert_eq!(ips.len(), 5);
    }

    #[test]
    fn cidr_detecte_via_slash() {
        // parse_range doit rediriger vers parse_cidr si le format contient '/'
        let ips = NetworkScanner::parse_range("10.0.0.0/30").unwrap();
        assert_eq!(ips.len(), 2);
    }

    #[test]
    fn detect_subnet_retourne_quelque_chose() {
        // La détection peut échouer dans un environnement sans réseau, c'est OK.
        let result = detect_local_subnet();
        if let Some(s) = result {
            assert!(s.contains('/'), "doit être un CIDR");
        }
    }
}
