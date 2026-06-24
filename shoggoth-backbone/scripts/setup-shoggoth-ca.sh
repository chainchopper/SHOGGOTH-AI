#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# setup-shoggoth-ca.sh — Generate Shoggoth Certificate Authority and node certs.
#
# Creates a self-signed root CA, then issues per-node certificates for mTLS.
# Each node agent and the orchestrator get unique certs signed by this CA.
#
# Usage:
#   ./setup-shoggoth-ca.sh                    # Generate CA + 1 orchestrator cert
#   ./setup-shoggoth-ca.sh --node bc250-01    # Generate a node cert
#   ./setup-shoggoth-ca.sh --all-nodes 12     # Generate certs for 12 BC250 nodes
#
# Output:
#   deploy/certs/
#     shoggoth-ca.crt           # Public CA certificate (distribute to all nodes)
#     shoggoth-ca.key           # CA private key (KEEP SECRET)
#     orchestrator/             # Orchestrator cert + key
#     nodes/bc250-01/           # Per-node cert + key

set -euo pipefail

CERT_DIR="${SHOGGOTH_CERT_DIR:-deploy/certs}"
CA_KEY="${CERT_DIR}/shoggoth-ca.key"
CA_CRT="${CERT_DIR}/shoggoth-ca.crt"
CA_SERIAL="${CERT_DIR}/shoggoth-ca.srl"
CA_DAYS=3650  # 10 years
NODE_DAYS=365  # 1 year

mkdir -p "${CERT_DIR}/orchestrator" "${CERT_DIR}/nodes"

# ── 1. Generate Root CA ───────────────────────────────────────────────────────

if [ ! -f "${CA_KEY}" ]; then
    echo "=== Generating Shoggoth Root CA ==="
    openssl genpkey -algorithm RSA -out "${CA_KEY}" -pkeyopt rsa_keygen_bits:4096
    openssl req -new -x509 -key "${CA_KEY}" -out "${CA_CRT}" -days "${CA_DAYS}" \
        -subj "/C=US/O=Shoggoth Mesh Machine/CN=Shoggoth Root CA"
    echo "01" > "${CA_SERIAL}"
    echo "  CA certificate: ${CA_CRT}"
    echo "  CA key:         ${CA_KEY}"
    echo "  Distribute CA cert to ALL nodes and the orchestrator."
else
    echo "=== Root CA exists, skipping generation ==="
fi

# ── 2. Helper: Issue a certificate ────────────────────────────────────────────

issue_cert() {
    local name="$1"
    local cn="$2"
    local out_dir="$3"

    if [ -f "${out_dir}/${name}.crt" ]; then
        echo "  Certificate for ${name} already exists, skipping."
        return
    fi

    # Generate private key.
    openssl genpkey -algorithm RSA -out "${out_dir}/${name}.key" -pkeyopt rsa_keygen_bits:2048

    # Generate CSR.
    openssl req -new -key "${out_dir}/${name}.key" -out "${out_dir}/${name}.csr" \
        -subj "/C=US/O=Shoggoth Mesh Machine/CN=${cn}"

    # Sign with CA.
    openssl x509 -req -in "${out_dir}/${name}.csr" \
        -CA "${CA_CRT}" -CAkey "${CA_KEY}" -CAserial "${CA_SERIAL}" \
        -out "${out_dir}/${name}.crt" -days "${NODE_DAYS}" \
        -extfile <(printf "subjectAltName=DNS:${cn},DNS:localhost\nkeyUsage=digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth,clientAuth")

    # Clean up CSR.
    rm -f "${out_dir}/${name}.csr"

    echo "  Issued: ${out_dir}/${name}.crt"
}

# ── 3. Orchestrator Certificate ───────────────────────────────────────────────

echo ""
echo "=== Issuing Orchestrator Certificate ==="
issue_cert "orchestrator" "shoggoth-orchestrator.shoggoth.local" "${CERT_DIR}/orchestrator"

# ── 4. Node Certificates ──────────────────────────────────────────────────────

if [ "${1:-}" = "--all-nodes" ]; then
    count="${2:-12}"
    echo ""
    echo "=== Issuing ${count} Node Certificates ==="
    for i in $(seq -w 1 "${count}"); do
        node_dir="${CERT_DIR}/nodes/bc250-${i}"
        mkdir -p "${node_dir}"
        issue_cert "bc250-${i}" "bc250-${i}.shoggoth.local" "${node_dir}"
    done
elif [ "${1:-}" = "--node" ]; then
    node_name="${2:-unknown}"
    node_dir="${CERT_DIR}/nodes/${node_name}"
    mkdir -p "${node_dir}"
    echo ""
    echo "=== Issuing Node Certificate: ${node_name} ==="
    issue_cert "${node_name}" "${node_name}.shoggoth.local" "${node_dir}"
fi

# ── 5. Summary ────────────────────────────────────────────────────────────────

echo ""
echo "=== Certificate Generation Complete ==="
echo "  CA cert:      ${CA_CRT}"
echo "  Orchestrator: ${CERT_DIR}/orchestrator/"
echo "  Nodes:        ${CERT_DIR}/nodes/"
echo ""
echo "  Next steps:"
echo "  1. Copy ${CA_CRT} to all nodes and the orchestrator."
echo "  2. Set SHOGGOTH_CA_CERT=/path/to/shoggoth-ca.crt in node-agent env."
echo "  3. Set SHOGGOTH_TLS_CERT and SHOGGOTH_TLS_KEY for each service."
