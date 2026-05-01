# BetterSSH

A modern, secure SSH client built in Rust. Multi-tab sessions, an integrated SFTP explorer, a network scanner, an encrypted secrets vault, and a clean graphical interface — all in a single native binary.

---

## Features

- **Multi-tab terminal** — open as many SSH sessions as you need, switch with Ctrl+Tab
- **SFTP explorer** — side-by-side file browser with drag-and-drop, rename, delete, download, permissions coloring (F2)
- **Encrypted vault** — host, username and password are never stored in plaintext; encrypted with [age](https://age-encryption.org/) behind a master key
- **Network scanner** — scan a CIDR range for open SSH ports and connect in one click
- **System monitor** — live CPU, RAM and disk graphs pulled from the remote host (F3)
- **Snippet manager** — save and replay frequently used commands (F4)
- **xterm-256color terminal** — ANSI colors, bold, italic, OSC sequences, progress bars (`\r`), `clear` command
- **Context-menu integration** — right-click "Connect with BetterSSH" directly from your file manager

---

## Screenshots

> _Coming soon._

---

## Usage

### Launch

```
betterssh.exe
```

The application opens in GUI mode. No arguments required.

### Keyboard shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+T` | New connection |
| `Ctrl+W` | Close current tab |
| `Ctrl+Tab` | Next tab |
| `Ctrl+Shift+Tab` | Previous tab |
| `F2` | Toggle SFTP file explorer |
| `F3` | Toggle system monitor |
| `F4` | Toggle snippet manager |
| `F5` | Toggle network scanner |
| `Ctrl+,` | Preferences |

### SFTP explorer shortcuts (when explorer is focused)

| Shortcut | Action |
|---|---|
| `Delete` | Delete selected files / folders |
| `F2` | Rename selected item |
| `Ctrl+A` | Select all |
| `Ctrl+C` | Copy |
| `Ctrl+X` | Cut |
| `Ctrl+V` | Paste |

---

## Vault — encrypted secrets

Host, username and password are stored encrypted in `~/.betterssh/vault.toml` using [age](https://age-encryption.org/) with a master passphrase. The master key is **never** written to disk.

When opening a saved profile that contains encrypted data, BetterSSH shows only the vault unlock prompt — no fields are visible until the correct master key is entered. Once unlocked, all fields are decrypted and the connection can proceed.

---

## Network scanner

The built-in scanner probes a CIDR range (e.g. `192.168.1.0/24`) for open SSH ports. Results show hostname, banner and response time. Double-click any result to connect immediately.

---

## Build from source

**Requirements:** Rust 1.75 or later.

```bash
# Clone
git clone https://github.com/rusty-suite/better_ssh.git
cd better_ssh

# Debug build
cargo build

# Release build (optimized, stripped binary)
cargo build --release
```

The binary is placed in `target/release/betterssh` (Linux/macOS) or `target\release\betterssh.exe` (Windows).

### Cross-compile for Linux from Windows

```bash
rustup target add x86_64-unknown-linux-gnu
cargo build --release --target x86_64-unknown-linux-gnu
```

---

## Configuration

All data is stored in `~/.betterssh/`:

| File | Contents |
|---|---|
| `config.toml` | Application settings (theme, font size, network defaults) |
| `profiles.toml` | Saved connection profiles (no secrets) |
| `vault.toml` | Encrypted secrets (host, username, password — age-encrypted) |
| `betterssh.log` | Application log |

---

## Download

Pre-built binaries are available on the [Releases](https://github.com/rusty-suite/better_ssh/releases) page.

| Platform | File |
|---|---|
| Windows x64 | `betterssh-windows-x64.exe` |
| Linux x64 | `betterssh-linux-x64` |

---

## License

PolyForm Noncommercial
