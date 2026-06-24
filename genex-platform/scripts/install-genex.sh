#!/usr/bin/env bash
# GENEx Platform — One-Line Install
#
# Installs the GENEx genomics processing appliance.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/chainchopper/genex-platform/main/scripts/install-genex.sh | bash
#   curl -fsSL https://raw.githubusercontent.com/chainchopper/genex-platform/main/scripts/install-genex.sh | bash -s -- --scylla-nodes "192.168.1.10,192.168.1.11,192.168.1.12"

set -euo pipefail

GENEX_VERSION="${GENEX_VERSION:-latest}"
SCYLLA_NODES="${1:-localhost}"
ADMIN_KEY="${GENEX_ADMIN_KEY:-genex-admin-$(openssl rand -hex 8)}"
INSTALL_DIR="${GENEX_INSTALL_DIR:-/opt/genex}"

GREEN='\033[0;32m'
AMBER='\033[0;33m'
NC='\033[0m'

echo -e "${AMBER}"
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║     GENEx — Genomics Processing Appliance                    ║"
echo "║     Version: ${GENEX_VERSION}                                    ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo -e "${NC}"

# ── 1. Prerequisites ──────────────────────────────────────────────────────────
echo -e "${AMBER}[1/5] Checking prerequisites...${NC}"
if ! command -v docker &> /dev/null; then
    echo "Docker is required. Install with: curl -fsSL https://get.docker.com | bash"
    exit 1
fi
echo -e "${GREEN}  ✓ Docker available${NC}"

# ── 2. Create directories ─────────────────────────────────────────────────────
echo -e "${AMBER}[2/5] Creating installation directories...${NC}"
sudo mkdir -p "${INSTALL_DIR}/data" "${INSTALL_DIR}/config"
sudo chown -R "$(whoami)":"$(whoami)" "${INSTALL_DIR}"
echo -e "${GREEN}  ✓ ${INSTALL_DIR}${NC}"

# ── 3. Config ─────────────────────────────────────────────────────────────────
echo -e "${AMBER}[3/5] Creating configuration...${NC}"
cat > "${INSTALL_DIR}/config/genex.toml" <<EOF
[genex]
scylla_nodes = "${SCYLLA_NODES}"
scylla_keyspace = "genex"
reference_genome = "${INSTALL_DIR}/data/reference/hg38.fa"
admin_key = "${ADMIN_KEY}"

[escrow]
ledger_url = "http://localhost:8545"
verification_nodes = 3

[shoggoth]
orchestrator_socket = "/dev/shm/shoggoth/orchestrator.sock"
EOF
echo -e "${GREEN}  ✓ Configuration written${NC}"

# ── 4. Pull images ────────────────────────────────────────────────────────────
echo -e "${AMBER}[4/5] Pulling GENEx images...${NC}"
docker pull "ghcr.io/chainchopper/genex-daemon:${GENEX_VERSION}" &
docker pull "ghcr.io/chainchopper/genex-admin:${GENEX_VERSION}" &
docker pull scylladb/scylla:6.2 &
wait
echo -e "${GREEN}  ✓ Images pulled${NC}"

# ── 5. Start services ─────────────────────────────────────────────────────────
echo -e "${AMBER}[5/5] Starting GENEx services...${NC}"

cat > "${INSTALL_DIR}/docker-compose.yml" <<EOFDOCKER
version: "3.9"
services:
  scylla:
    image: scylladb/scylla:6.2
    container_name: genex-scylla
    restart: unless-stopped
    network_mode: host
    volumes:
      - genex-scylla-data:/var/lib/scylla
    environment:
      - SCYLLA_CLUSTER_NAME=genex
      - SCYLLA_LISTEN_ADDRESS=0.0.0.0
    command: ["--smp=8", "--memory=16G", "--developer-mode=0"]
  genex-daemon:
    image: ghcr.io/chainchopper/genex-daemon:${GENEX_VERSION}
    container_name: genex-daemon
    restart: unless-stopped
    network_mode: host
    privileged: true
    volumes:
      - /dev/shm/shoggoth:/dev/shm/shoggoth
      - ${INSTALL_DIR}/data:/data
      - ${INSTALL_DIR}/config:/etc/genex:ro
    environment:
      - GENEX_SCYLLA_NODES=${SCYLLA_NODES}
      - GENEX_SCYLLA_KEYSPACE=genex
      - RUST_LOG=genex=info
    depends_on:
      - scylla
  genex-admin:
    image: ghcr.io/chainchopper/genex-admin:${GENEX_VERSION}
    container_name: genex-admin-panel
    restart: unless-stopped
    network_mode: host
    environment:
      - GENEX_API_URL=http://localhost:9200
      - GENEX_ADMIN_KEY=${ADMIN_KEY}
    depends_on:
      - genex-daemon
volumes:
  genex-scylla-data:
EOFDOCKER

cd "${INSTALL_DIR}" && docker compose up -d

echo ""
echo -e "${AMBER}╔══════════════════════════════════════════════════════════════╗"
echo "║     GENEx INSTALLATION COMPLETE                              ║"
echo "╚══════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo "  Admin panel:  http://localhost:9201"
echo "  Admin key:    ${ADMIN_KEY}"
echo ""
echo "  Save this key. It will not be shown again."
