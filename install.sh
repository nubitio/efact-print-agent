#!/usr/bin/env bash
# efact-printer-agent installer — Linux & macOS
# Usage: curl -fsSL https://raw.githubusercontent.com/nubitio/efact-print-agent/main/install.sh | bash
set -euo pipefail

REPO="nubitio/efact-print-agent"
BINARY="efact-printer-agent"
INSTALL_DIR="/usr/local/bin"
CONFIG_DIR="${HOME}/.config/efact-printer-agent"

# ── helpers ──────────────────────────────────────────────────────────────────
info()  { printf '\033[0;34m[efact-printer-agent]\033[0m %s\n' "$*"; }
ok()    { printf '\033[0;32m[efact-printer-agent]\033[0m %s\n' "$*"; }
err()   { printf '\033[0;31m[efact-printer-agent]\033[0m %s\n' "$*" >&2; exit 1; }

need() { command -v "$1" &>/dev/null || err "Required tool not found: $1"; }
need curl
need tar

# ── detect OS / arch ─────────────────────────────────────────────────────────
OS="$(uname -s)"
ARCH="$(uname -m)"

case "${OS}" in
  Linux)
    case "${ARCH}" in
      x86_64) ASSET="efact-printer-agent-linux-x86_64.tar.gz" ;;
      *)      err "Unsupported architecture: ${ARCH}" ;;
    esac
    ;;
  Darwin)
    case "${ARCH}" in
      x86_64)  ASSET="efact-printer-agent-macos-x86_64.tar.gz" ;;
      arm64)   ASSET="efact-printer-agent-macos-arm64.tar.gz" ;;
      *)       err "Unsupported architecture: ${ARCH}" ;;
    esac
    ;;
  *) err "Unsupported OS: ${OS}" ;;
esac

# ── Linux: ensure libappindicator3 is present (required for tray icon) ────────
if [ "${OS}" = "Linux" ]; then
  if ! ldconfig -p 2>/dev/null | grep -q 'libayatana-appindicator3\|libappindicator3'; then
    info "libappindicator3 not found — attempting to install..."
    if command -v apt-get &>/dev/null; then
      sudo apt-get install -y libayatana-appindicator3-1 2>/dev/null \
        || sudo apt-get install -y libappindicator3-1
    elif command -v dnf &>/dev/null; then
      sudo dnf install -y libappindicator-gtk3
    elif command -v pacman &>/dev/null; then
      sudo pacman -S --noconfirm libappindicator-gtk3
    else
      err "Cannot install libappindicator3 automatically. Please install it manually and re-run this script."
    fi
    ok "libappindicator3 installed."
  fi
fi

# ── latest release tag ────────────────────────────────────────────────────────
info "Fetching latest release..."
TAG="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
  | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')"

[ -n "${TAG}" ] || err "Could not determine latest release tag."
info "Latest release: ${TAG}"

# ── download & install ────────────────────────────────────────────────────────
TMP="$(mktemp -d)"
trap 'rm -rf "${TMP}"' EXIT

DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${TAG}/${ASSET}"
info "Downloading ${ASSET}..."
curl -fsSL "${DOWNLOAD_URL}" -o "${TMP}/${ASSET}"

tar -xzf "${TMP}/${ASSET}" -C "${TMP}"

# May need sudo to write to /usr/local/bin
if [ -w "${INSTALL_DIR}" ]; then
  install -m 755 "${TMP}/${BINARY}" "${INSTALL_DIR}/${BINARY}"
else
  info "Installing to ${INSTALL_DIR} (sudo required)..."
  sudo install -m 755 "${TMP}/${BINARY}" "${INSTALL_DIR}/${BINARY}"
fi

# ── default config ────────────────────────────────────────────────────────────
if [ ! -f "${CONFIG_DIR}/config.toml" ]; then
  mkdir -p "${CONFIG_DIR}"
  cp "${TMP}/config.toml" "${CONFIG_DIR}/config.toml"
  info "Default config written to ${CONFIG_DIR}/config.toml"
fi

# ── autostart (systemd on Linux, LaunchAgent on macOS) ───────────────────────
setup_systemd() {
  SERVICE_FILE="${HOME}/.config/systemd/user/efact-printer-agent.service"
  mkdir -p "$(dirname "${SERVICE_FILE}")"
  cat > "${SERVICE_FILE}" <<EOF
[Unit]
Description=efact Printer Agent
After=network.target

[Service]
ExecStart=${INSTALL_DIR}/${BINARY}
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF
  systemctl --user daemon-reload
  systemctl --user enable --now efact-printer-agent
  ok "systemd user service enabled and started."
}

setup_launchagent() {
  PLIST="${HOME}/Library/LaunchAgents/io.nubit.efact-printer-agent.plist"
  LABEL="io.nubit.efact-printer-agent"
  DOMAIN="gui/$(id -u)"
  cat > "${PLIST}" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>io.nubit.efact-printer-agent</string>
  <key>ProgramArguments</key>
  <array>
    <string>${INSTALL_DIR}/${BINARY}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardOutPath</key>
  <string>${HOME}/Library/Logs/efact-printer-agent.log</string>
  <key>StandardErrorPath</key>
  <string>${HOME}/Library/Logs/efact-printer-agent.log</string>
</dict>
</plist>
EOF

  # `launchctl load` is unreliable on newer macOS versions and may emit
  # confusing I/O errors during reinstalls. Refresh the user agent explicitly.
  launchctl bootout "${DOMAIN}"/"${LABEL}" &>/dev/null || true
  launchctl bootstrap "${DOMAIN}" "${PLIST}"
  launchctl kickstart -k "${DOMAIN}"/"${LABEL}"
  ok "LaunchAgent registered and started."
}

if [ "${OS}" = "Linux" ] && command -v systemctl &>/dev/null; then
  setup_systemd
elif [ "${OS}" = "Darwin" ]; then
  setup_launchagent
fi

ok "efact-printer-agent ${TAG} installed successfully."
ok "Agent running on http://localhost:8765"
ok "Config: ${CONFIG_DIR}/config.toml"
