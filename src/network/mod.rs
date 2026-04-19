/// Module réseau : scan de plages IP, détection SSH, DNS inverse.
pub mod scanner;

pub use scanner::{detect_local_subnet, NetworkScanner, ScanEvent, ScanParams, ScanResult};
