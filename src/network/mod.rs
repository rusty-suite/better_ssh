/// Module réseau : scan de plages IP, détection SSH, DNS inverse, sessions Telnet.
pub mod scanner;
pub mod telnet;

pub use scanner::{detect_local_subnet, NetworkScanner, ScanEvent, ScanParams, ScanResult};
pub use telnet::TelnetSession;
