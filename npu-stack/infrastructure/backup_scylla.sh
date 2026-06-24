#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# backup_scylla.sh — Non-blocking, thread-safe ScyllaDB snapshot script.
#
# Creates a point-in-time snapshot of the ScyllaDB genomic data keyspace
# using nodetool snapshot. Safe to run while the database is serving reads/writes.
#
# Usage:
#   ./backup_scylla.sh [keyspace] [snapshot_name]
#
# Cron example (daily at 3 AM):
#   0 3 * * * /opt/npu-stack/infrastructure/backup_scylla.sh genex daily-$(date +\%Y\%m\%d)

set -euo pipefail

KEYSPACE="${1:-genex}"
SNAPSHOT_NAME="${2:-snapshot-$(date +%Y%m%d-%H%M%S)}"
BACKUP_ROOT="${SHOGGOTH_BACKUP_ROOT:-/mnt/shoggoth-backups/scylla}"
RETENTION_DAYS="${SHOGGOTH_BACKUP_RETENTION_DAYS:-30}"

NODETOOL="${SCYLLA_NODETOOL:-nodetool}"

echo "=== Shoggoth ScyllaDB Backup ==="
echo "  Keyspace:      ${KEYSPACE}"
echo "  Snapshot:      ${SNAPSHOT_NAME}"
echo "  Backup Root:   ${BACKUP_ROOT}"
echo "  Retention:     ${RETENTION_DAYS} days"
echo ""

# ── 1. Take the snapshot (non-blocking) ────────────────────────────────────────

echo "[1/4] Taking snapshot '${SNAPSHOT_NAME}' of keyspace '${KEYSPACE}'..."
${NODETOOL} snapshot "${KEYSPACE}" -t "${SNAPSHOT_NAME}"
echo "  Snapshot created."

# ── 2. Locate snapshot files ───────────────────────────────────────────────────

SNAPSHOT_DIR=$(find /var/lib/scylla/data/"${KEYSPACE}" -type d -name "${SNAPSHOT_NAME}" | head -1)
if [ -z "${SNAPSHOT_DIR}" ]; then
    echo "ERROR: Could not locate snapshot directory for ${SNAPSHOT_NAME}"
    exit 1
fi
echo "[2/4] Snapshot located at: ${SNAPSHOT_DIR}"

# ── 3. Copy to backup storage ──────────────────────────────────────────────────

BACKUP_PATH="${BACKUP_ROOT}/${KEYSPACE}/${SNAPSHOT_NAME}"
mkdir -p "${BACKUP_PATH}"

echo "[3/4] Copying snapshot to ${BACKUP_PATH}..."
# Use rsync for incremental copy with progress.
rsync -avh --progress "${SNAPSHOT_DIR}/" "${BACKUP_PATH}/"
echo "  Copy complete."

# ── 4. Cleanup old snapshots ───────────────────────────────────────────────────

echo "[4/4] Cleaning up snapshots older than ${RETENTION_DAYS} days..."
find "${BACKUP_ROOT}/${KEYSPACE}" -maxdepth 1 -type d -mtime "+${RETENTION_DAYS}" -exec rm -rf {} \; 2>/dev/null || true

# Also clean up the Scylla-internal snapshot to free disk space.
${NODETOOL} clearsnapshot "${KEYSPACE}" -t "${SNAPSHOT_NAME}" 2>/dev/null || true

echo ""
echo "=== Backup Complete ==="
echo "  Path:    ${BACKUP_PATH}"
echo "  Size:    $(du -sh "${BACKUP_PATH}" | cut -f1)"

