#!/usr/bin/env bash
# Shoggoth Mesh Machine — One-Line Install (Linux)
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/chainchopper/shoggoth-backbone/main/scripts/install-linux.sh | bash
#
# Options:
#   curl -fsSL https://raw.githubusercontent.com/chainchopper/shoggoth-backbone/main/scripts/install-linux.sh | bash -s -- --role orchestrator
#   curl -fsSL https://raw.githubusercontent.com/chainchopper/shoggoth-backbone/main/scripts/install-linux.sh | bash -s -- --role node-agent --orchestrator 192.168.1.100
#   curl -fsSL https://raw.githubusercontent.com/chainchopper/shoggoth-backbone/main/scripts/install-linux.sh | bash -s -- --role all-in-one

set -euo pipefail

SHOGGOTH_VERSION="${SHOGGOTH_VERSION:-latest}"
SHOGGOTH_ROLE="${1:-orchestrator}"
ORCHESTRATOR_ADDR="${2:-localhost:9100}"
INSTALL_DIR="${SHOGGOTH_INSTALL_DIR:-/opt/shoggoth}"

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${GREEN}"
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║     SHOGGOTH MESH MACHINE — Linux Installer                 ║"
echo "║     Version: ${SHOGGOTH_VERSION}                                    ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo -e "${NC}"

# ── 1. Prerequisites ──────────────────────────────────────────────────────────
echo -e "${YELLOW}[1/6] Checking prerequisites...${NC}"

if ! command -v docker &> /dev/null; then
    echo "Installing Docker..."
    curl -fsSL https://get.docker.com | bash
    sudo usermod -aG docker "$USER"
fi

if ! command -v nvidia-smi &> /dev/null; then
    echo "Installing NVIDIA drivers..."
    sudo apt-get update && sudo apt-get install -y nvidia-driver-570
fi

# NVIDIA Container Toolkit
if ! dpkg -l | grep -q nvidia-container-toolkit; then
    curl -fsSL https://nvidia.github.io/libnvidia-container/gpgkey | sudo gpg --dearmor -o /usr/share/keyrings/nvidia-container-toolkit-keyring.gpg
    curl -s -L https://nvidia.github.io/libnvidia-container/stable/deb/nvidia-container-toolkit.list | \
        sed 's#deb https://#deb [signed-by=/usr/share/keyrings/nvidia-container-toolkit-keyring.gpg] https://#g' | \
        sudo tee /etc/apt/sources.list.d/nvidia-container-toolkit.list
    sudo apt-get update && sudo apt-get install -y nvidia-container-toolkit
    sudo nvidia-ctk runtime configure --runtime=docker
    sudo systemctl restart docker
fi

echo -e "${GREEN}  ✓ Prerequisites OK${NC}"

# ── 2. Create directories ─────────────────────────────────────────────────────
echo -e "${YELLOW}[2/6] Creating installation directories...${NC}"
sudo mkdir -p "${INSTALL_DIR}/bin" "${INSTALL_DIR}/certs" "${INSTALL_DIR}/data"
sudo chown -R "$(whoami)":"$(whoami)" "${INSTALL_DIR}"
echo -e "${GREEN}  ✓ ${INSTALL_DIR}${NC}"

# ── 3. Pull images ────────────────────────────────────────────────────────────
echo -e "${YELLOW}[3/6] Pulling Shoggoth images...${NC}"
docker pull "ghcr.io/chainchopper/shoggoth-orchestrator:${SHOGGOTH_VERSION}" &
docker pull "ghcr.io/chainchopper/shoggoth-node-agent:${SHOGGOTH_VERSION}" &
wait
echo -e "${GREEN}  ✓ Images pulled${NC}"

# ── 4. Generate certs ─────────────────────────────────────────────────────────
echo -e "${YELLOW}[4/6] Generating TLS certificates...${NC}"
mkdir -p "${INSTALL_DIR}/certs"
openssl req -x509 -newkey rsa:2048 -keyout "${INSTALL_DIR}/certs/shoggoth.key" \
    -out "${INSTALL_DIR}/certs/shoggoth.crt" -days 365 -nodes \
    -subj "/CN=shoggoth.local" 2>/dev/null
echo -e "${GREEN}  ✓ Certificates generated${NC}"

# ── 5. Install systemd service ────────────────────────────────────────────────
echo -e "${YELLOW}[5/6] Installing systemd service...${NC}"

if [ "${SHOGGOTH_ROLE}" = "orchestrator" ] || [ "${SHOGGOTH_ROLE}" = "all-in-one" ]; then
    sudo tee /etc/systemd/system/shoggoth-orchestrator.service > /dev/null <<EOF
[Unit]
Description=Shoggoth Orchestrator
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=$(whoami)
ExecStart=/usr/bin/docker run --rm --name shoggoth-orchestrator --network host --privileged \\
    -v /dev/dri:/dev/dri:ro -v /dev/shm/shoggoth:/dev/shm/shoggoth \\
    -v ${INSTALL_DIR}/certs:/opt/shoggoth/certs:ro \\
    -e RUST_LOG=shoggoth=info \\
    ghcr.io/chainchopper/shoggoth-orchestrator:${SHOGGOTH_VERSION}
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF
    sudo systemctl daemon-reload
    sudo systemctl enable --now shoggoth-orchestrator
    echo -e "${GREEN}  ✓ Orchestrator service installed${NC}"
fi

if [ "${SHOGGOTH_ROLE}" = "node-agent" ] || [ "${SHOGGOTH_ROLE}" = "all-in-one" ]; then
    sudo tee /etc/systemd/system/shoggoth-node-agent.service > /dev/null <<EOF
[Unit]
Description=Shoggoth Node Agent
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=$(whoami)
ExecStart=/usr/bin/docker run --rm --name shoggoth-node-agent --network host --privileged --gpus all \\
    -v /dev/dri:/dev/dri:ro \\
    -e SHOGGOTH_NODE_ID=\$(hostname) \\
    -e SHOGGOTH_ORCHESTRATOR_ADDR=${ORCHESTRATOR_ADDR} \\
    -e RUST_LOG=shoggoth=info \\
    ghcr.io/chainchopper/shoggoth-node-agent:${SHOGGOTH_VERSION}
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF
    sudo systemctl daemon-reload
    sudo systemctl enable --now shoggoth-node-agent
    echo -e "${GREEN}  ✓ Node agent service installed${NC}"
fi

# ── 6. Done ───────────────────────────────────────────────────────────────────
echo ""
echo -e "${GREEN}╔══════════════════════════════════════════════════════════════╗"
echo "║     INSTALLATION COMPLETE                                    ║"
echo "╚══════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo "  Role:     ${SHOGGOTH_ROLE}"
echo "  Version:  ${SHOGGOTH_VERSION}"
echo ""
echo "  Next steps:"
echo "    Check status:   sudo systemctl status shoggoth-*"
echo "    View logs:      sudo journalctl -u shoggoth-orchestrator -f"
echo "    Dashboard:      http://localhost:1420"
echo ""
echo "  To add more nodes, run on each machine:"
echo "    curl -fsSL https://raw.githubusercontent.com/chainchopper/shoggoth-backbone/main/scripts/install-linux.sh | bash -s -- --role node-agent --orchestrator <ORCHESTRATOR_IP>"
