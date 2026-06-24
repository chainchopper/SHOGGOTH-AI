#!/usr/bin/env bash
# Shoggoth Mesh Machine — macOS Installer
#
# Installs the Shoggoth node agent as a launchd service on macOS.
# Apple Silicon Macs serve as edge viewport clients and lightweight compute.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/chainchopper/shoggoth-backbone/main/scripts/install-macos.sh | bash
#   curl -fsSL https://raw.githubusercontent.com/chainchopper/shoggoth-backbone/main/scripts/install-macos.sh | bash -s -- --orchestrator 192.168.1.100

set -euo pipefail

ORCHESTRATOR_ADDR="${1:-localhost:9100}"
INSTALL_DIR="${HOME}/.shoggoth"

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║     SHOGGOTH MESH MACHINE — macOS Installer                 ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

# ── 1. Check Docker Desktop ───────────────────────────────────────────────────
if ! command -v docker &> /dev/null; then
    echo "Docker Desktop is required. Install from: https://www.docker.com/products/docker-desktop/"
    exit 1
fi

# ── 2. Create directories ─────────────────────────────────────────────────────
mkdir -p "${INSTALL_DIR}" "${INSTALL_DIR}/certs"

# ── 3. Pull image ─────────────────────────────────────────────────────────────
echo "Pulling Shoggoth node agent..."
docker pull "ghcr.io/chainchopper/shoggoth-node-agent:latest"

# ── 4. Install launchd plist ──────────────────────────────────────────────────
PLIST_PATH="${HOME}/Library/LaunchAgents/com.shoggoth.node-agent.plist"

cat > "${PLIST_PATH}" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.shoggoth.node-agent</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/docker</string>
        <string>run</string>
        <string>--rm</string>
        <string>--name</string>
        <string>shoggoth-node-agent</string>
        <string>--network</string>
        <string>host</string>
        <string>-e</string>
        <string>SHOGGOTH_NODE_ID=$(hostname)</string>
        <string>-e</string>
        <string>SHOGGOTH_ORCHESTRATOR_ADDR=${ORCHESTRATOR_ADDR}</string>
        <string>-e</string>
        <string>RUST_LOG=shoggoth=info</string>
        <string>ghcr.io/chainchopper/shoggoth-node-agent:latest</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>${INSTALL_DIR}/node-agent.log</string>
    <key>StandardErrorPath</key>
    <string>${INSTALL_DIR}/node-agent.err</string>
</dict>
</plist>
EOF

launchctl load "${PLIST_PATH}" 2>/dev/null || true

echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║     INSTALLATION COMPLETE                                    ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""
echo "  Node agent running as launchd service."
echo "  Logs: ${INSTALL_DIR}/node-agent.log"
echo ""
echo "  To uninstall:"
echo "    launchctl unload ${PLIST_PATH} && rm ${PLIST_PATH}"
