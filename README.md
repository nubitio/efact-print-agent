# efact-printer-agent

Local USB printer agent for [efact](https://github.com/nubitio). Receives raw ESC/POS bytes from the browser and forwards them directly to a thermal printer over USB — no QZ Tray, no Java, no browser extensions required.

## How it works

```
efact frontend  →  POST /print (raw ESC/POS)  →  efact-printer-agent  →  USB thermal printer
```

The agent runs as a background process on the client machine, listening on `localhost:8765`. The efact frontend detects it via the `feature_local_agent_print` tenant flag and posts print jobs directly.

## Installation

### Linux / macOS

```bash
curl -fsSL https://raw.githubusercontent.com/nubitio/efact-print-agent/main/install.sh | bash
```

### Windows (PowerShell)

```powershell
iwr -useb https://raw.githubusercontent.com/nubitio/efact-print-agent/main/install.ps1 | iex
```

The installer will:
- Download the correct binary for your OS and architecture
- Install it to a local directory and add it to PATH
- Write a default `config.toml`
- Register autostart (systemd user service on Linux, LaunchAgent on macOS, Task Scheduler on Windows)

## API

| Method | Endpoint    | Description                              |
|--------|-------------|------------------------------------------|
| GET    | `/health`   | Liveness check → `{ status, version }`  |
| GET    | `/printers` | List detected thermal printers           |
| POST   | `/print`    | Send raw ESC/POS bytes to printer        |

`POST /print` expects `Content-Type: application/octet-stream` and returns `204 No Content` on success.

## Configuration

The agent loads `config.toml` from the first location found:

1. Next to the binary
2. `~/.config/efact-printer-agent/config.toml` (Linux/macOS)
3. `%APPDATA%\efact-printer-agent\config.toml` (Windows)

```toml
# HTTP port (default: 8765)
port = 8765

# Pin a specific printer by USB IDs (hex). If omitted, the first
# recognized thermal printer is used automatically.
# Run GET /printers to find your device's VID and PID.
# usb_vendor_id = "04b8"   # Epson
# usb_product_id = "0202"

# Write chunk size in bytes (default: 4096)
# chunk_size = 4096
```

### Supported printers

Any USB HID thermal printer is supported. The following vendors are detected automatically:

| Vendor        | VID    |
|---------------|--------|
| Epson         | `04B8` |
| Star Micronics| `0519` |
| Bixolon       | `1504` |
| Citizen       | `1CBE` |
| Sewoo         | `0DD4` |
| Rongta        | `20D1` |
| Xprinter      | `0FE6`, `6868` |

If your printer is not detected automatically, add its `usb_vendor_id` and `usb_product_id` to `config.toml`.

## Building from source

```bash
git clone https://github.com/nubitio/efact-print-agent.git
cd efact-print-agent
cargo build --release
# binary at target/release/efact-printer-agent
```

**Requirements:** Rust 1.75+, `libudev-dev` on Linux (`sudo apt install libudev-dev`).

## Releases

Binaries for all platforms are built automatically on every tagged release via GitHub Actions.

| Platform        | Download                                        |
|-----------------|-------------------------------------------------|
| Linux x86_64    | `efact-printer-agent-linux-x86_64.tar.gz`      |
| macOS x86_64    | `efact-printer-agent-macos-x86_64.tar.gz`      |
| macOS ARM64     | `efact-printer-agent-macos-arm64.tar.gz`        |
| Windows x86_64  | `efact-printer-agent-windows-x86_64.zip`        |

→ [Latest release](https://github.com/nubitio/efact-print-agent/releases/latest)

## License

MIT
